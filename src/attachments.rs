//! Files a person gives the Assistant: attached with the paperclip, dropped on the chat or
//! pasted. Each is read here into what a model can take: a picture (always PNG, so every
//! provider and CLI handles it the same way), text, a PDF for providers that read PDFs, or,
//! for the files FashionCraft itself opens (patterns, 3D models, motions, projects), a factual
//! summary and, for patterns, a picture of the pieces. Nothing here changes the project.
use glam::DVec2;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Largest file read, bytes.
pub const MAX_BYTES: usize = 32 << 20;
/// Longest side of a picture sent to a model, pixels: what the models see at most.
pub const MAX_SIDE: u32 = 1568;
/// Most characters of text sent from one file.
pub const MAX_TEXT: usize = 40_000;
/// Largest PDF sent whole, bytes.
pub const MAX_PDF: usize = 24 << 20;

/// What kind of file it is, by who handles it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Document,
    Table,
    Video,
    Audio,
    /// DXF, DXF-AAMA/ASTM or SVG patterns, opened by FashionCraft's importers.
    Pattern,
    /// A 3D model: an avatar, a collider or a trim object.
    Model,
    Motion,
    Fabric,
    Project,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Document => "document",
            Self::Table => "table",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Pattern => "pattern",
            Self::Model => "3D model",
            Self::Motion => "motion",
            Self::Fabric => "fabric",
            Self::Project => "project",
            Self::Other => "file",
        }
    }
}

/// The kind a file's name says it is.
pub fn kind_of(name: &str) -> Kind {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "bmp" | "tga" | "hdr" | "webp" | "heic" | "heif" | "gif"
        | "tif" | "tiff" => Kind::Image,
        "pdf" | "txt" | "md" | "markdown" | "doc" | "docx" | "rtf" | "json" => Kind::Document,
        "csv" | "tsv" | "xls" | "xlsx" => Kind::Table,
        "mp4" | "mov" | "m4v" | "webm" | "avi" | "mkv" => Kind::Video,
        "mp3" | "wav" | "m4a" | "aac" | "ogg" | "flac" | "opus" => Kind::Audio,
        "dxf" | "svg" => Kind::Pattern,
        "obj" | "fbx" | "gltf" | "glb" | "vrm" | "usd" | "usda" | "usdz" | "abc" | "stl"
        | "ply" | "dae" => Kind::Model,
        "bvh" | "pc2" | "mdd" => Kind::Motion,
        "fcfabric" => Kind::Fabric,
        "fcproj" | "fashioncraft" => Kind::Project,
        _ => Kind::Other,
    }
}

/// What the model receives for a file.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    /// A PNG, base64.
    Image {
        png_base64: String,
        width: u32,
        height: u32,
    },
    /// A PDF, base64, for providers that read PDFs; others get only the note.
    Pdf {
        base64: String,
        pages: usize,
    },
    Text {
        text: String,
        truncated: bool,
    },
    /// Read by FashionCraft: what it found, and a picture of it when there is one.
    Summary {
        text: String,
        picture: Option<(String, u32, u32)>,
    },
    /// Not readable here: why.
    Unreadable {
        reason: String,
    },
}

/// One attached file.
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    /// Unique in the conversation: the agent tools name it by this.
    pub name: String,
    pub kind: Kind,
    pub payload: Payload,
    /// Where it was on disk, when it came from a file.
    pub path: Option<PathBuf>,
}

impl Attachment {
    /// A short line for the chat and for the model: name, kind and what was made of it.
    pub fn describe(&self) -> String {
        let what = match &self.payload {
            Payload::Image { width, height, .. } => format!("picture, {width} × {height} px"),
            Payload::Pdf { pages, .. } => format!("PDF, {pages} page(s)"),
            Payload::Text { text, truncated } => format!(
                "{} characters of text{}",
                text.chars().count(),
                if *truncated { ", cut short" } else { "" }
            ),
            Payload::Summary { .. } => "read by FashionCraft".into(),
            Payload::Unreadable { reason } => format!("not readable: {reason}"),
        };
        format!("{} ({}, {what})", self.name, self.kind.label())
    }

    /// Its picture, PNG base64, if it has one to show.
    pub fn picture(&self) -> Option<&str> {
        match &self.payload {
            Payload::Image { png_base64, .. } => Some(png_base64),
            Payload::Summary {
                picture: Some((png, _, _)),
                ..
            } => Some(png),
            _ => None,
        }
    }
}

fn b64(bytes: &[u8]) -> String {
    crate::agent::base64(bytes)
}

/// A picture as PNG no larger than [`MAX_SIDE`].
fn png_of(image: image::DynamicImage) -> Result<(String, u32, u32), String> {
    let longest = image.width().max(image.height());
    let image = if longest > MAX_SIDE {
        image.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut png = Vec::new();
    image
        .to_rgba8()
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((b64(&png), image.width(), image.height()))
}

/// A picture from raw RGBA pixels, as the clipboard gives it.
pub fn from_rgba(name: &str, width: u32, height: u32, rgba: Vec<u8>) -> Result<Attachment, String> {
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or("The pasted picture has the wrong size")?;
    let (png_base64, width, height) = png_of(image::DynamicImage::ImageRgba8(image))?;
    Ok(Attachment {
        name: name.into(),
        kind: Kind::Image,
        payload: Payload::Image {
            png_base64,
            width,
            height,
        },
        path: None,
    })
}

/// Reads a file from disk.
pub fn from_path(path: &Path) -> Attachment {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let read = std::fs::metadata(path)
        .map_err(|e| e.to_string())
        .and_then(|m| {
            if m.len() as usize > MAX_BYTES {
                Err(format!("larger than {} MB", MAX_BYTES >> 20))
            } else {
                std::fs::read(path).map_err(|e| e.to_string())
            }
        });
    let mut attachment = match read {
        Ok(bytes) => from_bytes(&name, &bytes),
        Err(reason) => Attachment {
            kind: kind_of(&name),
            name,
            payload: Payload::Unreadable { reason },
            path: None,
        },
    };
    attachment.path = Some(path.to_owned());
    attachment
}

fn text_payload(text: String) -> Payload {
    let truncated = text.chars().count() > MAX_TEXT;
    Payload::Text {
        text: if truncated {
            text.chars().take(MAX_TEXT).collect()
        } else {
            text
        },
        truncated,
    }
}

fn unreadable(reason: impl Into<String>) -> Payload {
    Payload::Unreadable {
        reason: reason.into(),
    }
}

/// Reads a file's bytes by what its name says it is (a picture is also recognised by its
/// first bytes).
pub fn from_bytes(name: &str, bytes: &[u8]) -> Attachment {
    let mut kind = kind_of(name);
    if image::guess_format(bytes).is_ok() && kind == Kind::Other {
        kind = Kind::Image;
    }
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let payload = match kind {
        Kind::Image => match image::load_from_memory(bytes)
            .map_err(|e| e.to_string())
            .and_then(png_of)
        {
            Ok((png_base64, width, height)) => Payload::Image {
                png_base64,
                width,
                height,
            },
            Err(_) => unreadable(format!(
                "FashionCraft reads PNG, JPEG, BMP, TGA and HDR pictures; save this .{ext} as PNG or JPEG"
            )),
        },
        Kind::Document if ext == "pdf" => {
            if bytes.len() > MAX_PDF {
                unreadable(format!("PDFs up to {} MB", MAX_PDF >> 20))
            } else if !bytes.starts_with(b"%PDF") {
                unreadable("not a PDF")
            } else {
                Payload::Pdf {
                    base64: b64(bytes),
                    pages: pdf_pages(bytes),
                }
            }
        }
        Kind::Document | Kind::Table
            if matches!(ext.as_str(), "doc" | "docx" | "rtf" | "xls" | "xlsx") =>
        {
            unreadable(format!(
                "FashionCraft cannot read .{ext} yet; export it as PDF, CSV or plain text"
            ))
        }
        Kind::Document | Kind::Table => match std::str::from_utf8(bytes) {
            Ok(text) => text_payload(text.to_owned()),
            Err(_) => unreadable("not UTF-8 text"),
        },
        Kind::Pattern => pattern_summary(name, &ext, bytes),
        Kind::Video => {
            unreadable("FashionCraft cannot read video yet: attach a few screenshots of it instead")
        }
        Kind::Audio => unreadable(
            "FashionCraft cannot transcribe audio yet: type the request, or use your system's dictation",
        ),
        Kind::Model => Payload::Summary {
            text: format!(
                "A 3D model (.{ext}). FashionCraft opens it with File › Import avatar (a body or a \
                 collider) or File › Open trim; turning a garment mesh into a pattern is not \
                 available."
            ),
            picture: None,
        },
        Kind::Motion => Payload::Summary {
            text: format!(
                "A motion (.{ext}). FashionCraft plays it on the avatar from the animation editor \
                 (Window › Animation editor: Import BVH, or File › Import a point cache)."
            ),
            picture: None,
        },
        Kind::Fabric | Kind::Project => Payload::Summary {
            text: format!(
                "A FashionCraft {} file (.{ext}). Open it from the File menu.",
                kind.label()
            ),
            picture: None,
        },
        Kind::Other => match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => {
                kind = Kind::Document;
                text_payload(text.to_owned())
            }
            _ => unreadable(format!("FashionCraft does not read .{ext} files")),
        },
    };
    Attachment {
        name: name.into(),
        kind,
        payload,
        path: None,
    }
}

/// Pages a PDF has, counted roughly from its page objects.
fn pdf_pages(bytes: &[u8]) -> usize {
    let needle = b"/Type/Page";
    let spaced = b"/Type /Page";
    let count = |n: &[u8]| {
        bytes
            .windows(n.len() + 1)
            .filter(|w| &w[..n.len()] == n && w[n.len()] != b's')
            .count()
    };
    (count(needle) + count(spaced)).max(1)
}

/// Named outlines, metres.
type Outlines = Vec<(String, Vec<DVec2>)>;

/// The outlines of a pattern file, metres, with their names; and what kind of file it was.
fn pattern_outlines(ext: &str, text: &str) -> Result<(Outlines, String), String> {
    use crate::garment::{dxf_aama, dxf_import, svg_import};
    if ext == "svg" {
        let drawing = svg_import::read_svg(text, svg_import::Options::default())?;
        let outlines = drawing
            .outlines
            .into_iter()
            .map(|o| (o.name, o.points))
            .collect();
        return Ok((outlines, "SVG pattern".into()));
    }
    if dxf_aama::is_apparel(text) {
        let file = dxf_aama::read(text, dxf_aama::ReadOptions::default())?;
        let mut what = format!("{:?} apparel DXF", file.dialect);
        if !file.style.is_empty() {
            what.push_str(&format!(", style {}", file.style));
        }
        if !file.sample_size.is_empty() {
            what.push_str(&format!(", sample size {}", file.sample_size));
        }
        let outlines = file
            .pieces
            .into_iter()
            .map(|p| (p.name, p.cut.points))
            .collect();
        return Ok((outlines, what));
    }
    let drawing = dxf_import::read_ascii(text, dxf_import::Options::default())?;
    let scale = if drawing.insunits.is_some() {
        "units from the file"
    } else {
        "no units in the file: millimetres assumed"
    };
    let outlines = drawing
        .paths
        .into_iter()
        .filter(|p| p.closed && p.points.len() >= 3)
        .enumerate()
        .map(|(k, p)| (format!("contour {} ({})", k + 1, p.layer), p.points))
        .collect();
    Ok((outlines, format!("CAD DXF, {scale}")))
}

fn pattern_summary(name: &str, ext: &str, bytes: &[u8]) -> Payload {
    let text = String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|e| crate::pattern_import::windows_1252(e.as_bytes()));
    let (outlines, what) = match pattern_outlines(ext, &text) {
        Ok(found) => found,
        Err(e) => return unreadable(format!("the pattern could not be read: {e}")),
    };
    if outlines.is_empty() {
        return unreadable(format!("{what} without closed outlines"));
    }
    let mut lines = vec![format!(
        "{name}: {what}, {} piece(s). FashionCraft's importer can add them (unsewn, beside the \
         body) once the person confirms the scale: use open_attachment.",
        outlines.len()
    )];
    for (piece, points) in outlines.iter().take(40) {
        let (lo, hi) = bounds(points);
        let size = hi - lo;
        let perimeter: f64 = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| a.distance(*b))
            .sum();
        lines.push(format!(
            "- {piece}: {:.1} × {:.1} cm, perimeter {:.1} cm",
            size.x * 100.,
            size.y * 100.,
            perimeter * 100.
        ));
    }
    Payload::Summary {
        text: lines.join("\n"),
        picture: draw_outlines(&outlines, ext != "svg").ok(),
    }
}

fn bounds(points: &[DVec2]) -> (DVec2, DVec2) {
    points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

/// A picture of the pieces, black outlines on white with a 10 cm scale bar. CAD drawings have
/// y up, SVG y down.
fn draw_outlines(
    outlines: &[(String, Vec<DVec2>)],
    y_up: bool,
) -> Result<(String, u32, u32), String> {
    let all: Vec<DVec2> = outlines
        .iter()
        .flat_map(|(_, p)| p.iter().copied())
        .collect();
    let (lo, hi) = bounds(&all);
    let size = (hi - lo).max(DVec2::splat(1e-6));
    let side = 1000.;
    let scale = side / size.x.max(size.y);
    let (w, h) = (
        (size.x * scale).ceil() as u32 + 40,
        (size.y * scale).ceil() as u32 + 60,
    );
    let mut canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
    let to_px = |p: DVec2| {
        let q = (p - lo) * scale;
        DVec2::new(
            q.x + 20.,
            if y_up {
                size.y * scale - q.y + 20.
            } else {
                q.y + 20.
            },
        )
    };
    let mut line = |a: DVec2, b: DVec2| {
        let steps = a.distance(b).ceil().max(1.) as usize;
        for i in 0..=steps {
            let p = a.lerp(b, i as f64 / steps as f64);
            let (x, y) = (p.x.round() as i64, p.y.round() as i64);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1)] {
                let (x, y) = (x + dx, y + dy);
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    canvas.put_pixel(x as u32, y as u32, image::Rgba([20, 20, 20, 255]));
                }
            }
        }
    };
    for (_, points) in outlines {
        for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
            line(to_px(*a), to_px(*b));
        }
    }
    // A 10 cm bar under the drawing.
    let bar = 0.1 * scale;
    let y = f64::from(h) - 25.;
    line(DVec2::new(20., y), DVec2::new(20. + bar, y));
    line(DVec2::new(20., y - 6.), DVec2::new(20., y + 6.));
    line(DVec2::new(20. + bar, y - 6.), DVec2::new(20. + bar, y + 6.));
    png_of(image::DynamicImage::ImageRgba8(canvas))
}

/// The person's message with its attachments, as Anthropic's Messages API takes it: pictures
/// and PDFs as their own blocks, everything else as text before the message.
pub fn anthropic_blocks(text: &str, attachments: &[Attachment]) -> Vec<Value> {
    let mut blocks = vec![];
    for a in attachments {
        match &a.payload {
            Payload::Image { png_base64, .. } => {
                blocks.push(json!({"type": "text", "text": format!("Attached: {}", a.describe())}));
                blocks.push(json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": "image/png", "data": png_base64}
                }));
            }
            Payload::Pdf { base64, .. } => {
                blocks.push(json!({"type": "text", "text": format!("Attached: {}", a.describe())}));
                blocks.push(json!({
                    "type": "document",
                    "source": {"type": "base64", "media_type": "application/pdf", "data": base64},
                    "title": a.name,
                }));
            }
            Payload::Summary {
                picture: Some((png, _, _)),
                ..
            } => {
                blocks.push(json!({"type": "text", "text": as_text(a)}));
                blocks.push(json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": "image/png", "data": png}
                }));
            }
            _ => blocks.push(json!({"type": "text", "text": as_text(a)})),
        }
    }
    blocks.push(json!({"type": "text", "text": text}));
    blocks
}

/// The person's message with its attachments in the OpenAI chat format, which the CLIs also
/// read: pictures as `image_url` parts when the model sees pictures. PDFs are named, not sent:
/// this format's PDF support differs between servers.
pub fn openai_content(text: &str, attachments: &[Attachment], sees_images: bool) -> Value {
    if attachments.is_empty() {
        return json!(text);
    }
    let mut parts = vec![];
    let mut pictures = vec![];
    for a in attachments {
        let picture = a.picture();
        match (&a.payload, picture, sees_images) {
            (_, Some(png), true) => {
                parts.push(if matches!(a.payload, Payload::Image { .. }) {
                    format!("Attached: {} (the picture follows)", a.describe())
                } else {
                    format!("{} (its picture follows)", as_text(a))
                });
                pictures.push(json!({
                    "type": "image_url",
                    "image_url": {"url": format!("data:image/png;base64,{png}")}
                }));
            }
            (Payload::Image { .. }, _, false) => parts.push(format!(
                "Attached: {}. This model cannot see pictures: say so, and ask the person to \
                 describe it.",
                a.describe()
            )),
            (Payload::Pdf { .. }, _, _) => parts.push(format!(
                "Attached: {}. This connection cannot pass PDFs to the model: say so, and ask \
                 for the pages as pictures (screenshots) or the text pasted in.",
                a.describe()
            )),
            _ => parts.push(as_text(a)),
        }
    }
    parts.push(text.to_owned());
    let mut content = vec![json!({"type": "text", "text": parts.join("\n\n")})];
    content.extend(pictures);
    Value::Array(content)
}

/// What a non-picture attachment says, as text for the model.
fn as_text(a: &Attachment) -> String {
    match &a.payload {
        Payload::Text { text, truncated } => format!(
            "Attached file {} ({}){}:\n<file name=\"{}\">\n{}\n</file>",
            a.name,
            a.kind.label(),
            if *truncated { ", only its start" } else { "" },
            a.name,
            text
        ),
        Payload::Summary { text, .. } => format!("Attached {} {}: {text}", a.kind.label(), a.name),
        Payload::Unreadable { reason } => format!(
            "Attached {} could not be read ({reason}). Tell the person so.",
            a.name
        ),
        _ => format!("Attached: {}", a.describe()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 10, 10, 255]))
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn pictures_become_png_no_larger_than_models_see() {
        let mut jpeg = Vec::new();
        image::RgbImage::from_pixel(4000, 2000, image::Rgb([1, 2, 3]))
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let a = from_bytes("photo.JPG", &jpeg);
        let Payload::Image {
            png_base64,
            width,
            height,
        } = &a.payload
        else {
            panic!("{a:?}");
        };
        assert_eq!((*width, *height), (MAX_SIDE, MAX_SIDE / 2));
        let bytes = crate::agent::unbase64(png_base64).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Png
        );
        // A picture is recognised by its bytes even without its extension.
        assert_eq!(from_bytes("clipboard", &png(3, 3)).kind, Kind::Image);
        let pasted = from_rgba("pasted.png", 2, 1, vec![255; 8]).unwrap();
        assert!(pasted.picture().is_some());
        assert!(matches!(
            from_bytes("x.heic", b"....").payload,
            Payload::Unreadable { .. }
        ));
    }

    #[test]
    fn text_tables_and_pdfs_are_kept_within_limits() {
        let csv = "size,bust\nS,84\nM,88\n";
        let a = from_bytes("chart.csv", csv.as_bytes());
        assert_eq!(a.kind, Kind::Table);
        assert_eq!(
            a.payload,
            Payload::Text {
                text: csv.into(),
                truncated: false
            }
        );
        let long = "x".repeat(MAX_TEXT + 10);
        assert!(matches!(
            from_bytes("notes.md", long.as_bytes()).payload,
            Payload::Text {
                truncated: true,
                ..
            }
        ));
        let pdf =
            b"%PDF-1.4\n1 0 obj <</Type /Pages>> 2 0 obj <</Type /Page>> 3 0 obj <</Type/Page>>";
        assert_eq!(
            from_bytes("spec.pdf", pdf).payload,
            Payload::Pdf {
                base64: b64(pdf),
                pages: 2,
            }
        );
        assert!(matches!(
            from_bytes("spec.docx", b"PK").payload,
            Payload::Unreadable { .. }
        ));
        assert!(matches!(
            from_bytes("clip.mov", b"x").payload,
            Payload::Unreadable { .. }
        ));
        // Unknown but plain text is read as text.
        assert_eq!(from_bytes("notes.log", b"hello").kind, Kind::Document);
    }

    #[test]
    fn a_pattern_is_summarised_with_its_measurements_and_a_picture() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300mm" height="200mm" viewBox="0 0 300 200">
            <path id="front" d="M 10 10 L 110 10 L 110 160 L 10 160 Z"/>
            <path id="sleeve" d="M 150 10 L 250 10 L 230 90 L 170 90 Z"/></svg>"#;
        let a = from_bytes("blouse.svg", svg.as_bytes());
        let Payload::Summary { text, picture } = &a.payload else {
            panic!("{a:?}");
        };
        assert!(text.contains("2 piece(s)"), "{text}");
        assert!(text.contains("10.0 × 15.0 cm"), "{text}");
        assert!(picture.is_some());
        assert!(matches!(
            from_bytes("broken.dxf", b"nonsense").payload,
            Payload::Unreadable { .. }
        ));
    }

    #[test]
    fn messages_carry_attachments_for_each_api() {
        let picture = from_bytes("sketch.png", &png(4, 4));
        let table = from_bytes("chart.csv", b"size,bust\nM,88\n");
        let pdf = from_bytes("spec.pdf", b"%PDF-1.4 /Type /Page");
        let all = [picture, table, pdf];
        let blocks = anthropic_blocks("Make this", &all);
        let kinds: Vec<&str> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["text", "image", "text", "text", "document", "text"]);
        assert_eq!(blocks.last().unwrap()["text"], "Make this");
        let open = openai_content("Make this", &all, true);
        assert_eq!(open[1]["type"], "image_url");
        assert!(
            open[1]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        let said = open[0]["text"].as_str().unwrap();
        assert!(said.contains("<file name=\"chart.csv\">") && said.contains("cannot pass PDFs"));
        let blind = openai_content("Make this", &all, false);
        assert_eq!(blind.as_array().unwrap().len(), 1);
        assert!(
            blind[0]["text"]
                .as_str()
                .unwrap()
                .contains("cannot see pictures")
        );
        assert_eq!(openai_content("Hi", &[], true), json!("Hi"));
    }
}
