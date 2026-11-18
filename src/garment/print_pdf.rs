//! The 1:1 print sheet as a PDF: the layout of the print SVG (sewing outlines, cutting
//! contours, notches, internal lines, labels and the 100 mm calibration ruler) on one sheet,
//! or tiled over pages of a paper size. Each tile is framed and named by its row and column,
//! so the pages can be trimmed along the frames and taped edge to edge. Vector paths in
//! millimetres; labels in Helvetica, which every PDF reader has.
use super::{
    GarmentTemplate,
    export::{GAP_MM, LABEL_MM, MARGIN_MM, PrintLayout, print_layout},
};
use glam::DVec2;
use std::fmt::Write as _;

/// The paper to tile the sheet over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Paper {
    #[default]
    A4,
    Letter,
    A3,
    /// One page as large as the whole sheet, for a plotter.
    Sheet,
}

impl Paper {
    pub const ALL: [Self; 4] = [Self::A4, Self::Letter, Self::A3, Self::Sheet];
    pub fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::Letter => "Letter",
            Self::A3 => "A3",
            Self::Sheet => "One sheet",
        }
    }
    /// Portrait width and height, mm; none for one sheet of the whole layout.
    pub fn size_mm(self) -> Option<DVec2> {
        match self {
            Self::A4 => Some(DVec2::new(210., 297.)),
            Self::Letter => Some(DVec2::new(215.9, 279.4)),
            Self::A3 => Some(DVec2::new(297., 420.)),
            Self::Sheet => None,
        }
    }
}

/// Blank border of a tiled page, mm: most printers cannot print to the edge.
pub const PAGE_MARGIN_MM: f64 = 10.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Sewing,
    Cutting,
    Notch,
    Internal,
    Ruler,
}

enum Mark {
    Line {
        points: Vec<DVec2>,
        closed: bool,
        style: Style,
    },
    Text {
        at: DVec2,
        size: f64,
        text: String,
    },
}

/// Everything on the sheet, in sheet millimetres (y downward), as the print SVG draws it.
fn marks(template: &GarmentTemplate, layout: &PrintLayout) -> Result<Vec<Mark>, String> {
    let mut out = vec![];
    let mut y = MARGIN_MM;
    for (panel, (min, size)) in template.panels.iter().zip(&layout.pieces) {
        out.push(Mark::Text {
            at: DVec2::new(MARGIN_MM, y + 5.),
            size: 5.,
            text: format!(
                "{} — {} ({:.1} × {:.1} mm)",
                panel.id, panel.name, size.x, size.y
            ),
        });
        y += LABEL_MM;
        let offset = DVec2::new(MARGIN_MM - min.x, y - min.y);
        let at = |p: DVec2| p * 1000. + offset;
        let line = |points: Vec<DVec2>, closed, style| Mark::Line {
            points,
            closed,
            style,
        };
        if let Some(polygon) = &panel.polygon {
            let mut outer = vec![];
            for i in 0..polygon.edges.len() {
                for (j, p) in polygon.edge_points(i)?.iter().enumerate() {
                    if i > 0 && j == 0 {
                        continue;
                    }
                    outer.push(at(*p));
                }
            }
            out.push(line(outer, true, Style::Sewing));
            for hole in &polygon.holes {
                out.push(line(
                    hole.points.iter().map(|p| at(*p)).collect(),
                    true,
                    Style::Sewing,
                ));
            }
            if let Some(contour) = polygon.cutting_contour()? {
                for ring in std::iter::once(&contour.outer).chain(contour.holes.iter()) {
                    out.push(line(
                        ring.iter().map(|p| at(*p)).collect(),
                        true,
                        Style::Cutting,
                    ));
                }
            }
            for (edge, notches) in &polygon.notches {
                for notch in notches {
                    let marker = polygon.notch_marker(edge, notch)?;
                    out.push(line(
                        marker.iter().map(|p| at(*p)).collect(),
                        false,
                        Style::Notch,
                    ));
                }
            }
            for l in &polygon.lines {
                out.push(line(
                    l.samples()?.iter().map(|p| at(*p)).collect(),
                    false,
                    Style::Internal,
                ));
            }
        } else {
            let (w, h) = (size.x, size.y);
            let corners = [(0., 0.), (w, 0.), (w, h), (0., h)];
            out.push(line(
                corners
                    .iter()
                    .map(|(x, y)| DVec2::new(*x, *y) + offset)
                    .collect(),
                true,
                Style::Sewing,
            ));
        }
        y += size.y + GAP_MM;
    }
    let ruler = y + 8.;
    let segment = |a: DVec2, b: DVec2| Mark::Line {
        points: vec![a, b],
        closed: false,
        style: Style::Ruler,
    };
    out.push(segment(
        DVec2::new(MARGIN_MM, ruler),
        DVec2::new(MARGIN_MM + 100., ruler),
    ));
    for x in [MARGIN_MM, MARGIN_MM + 100.] {
        out.push(segment(
            DVec2::new(x, ruler - 5.),
            DVec2::new(x, ruler + 5.),
        ));
    }
    out.push(Mark::Text {
        at: DVec2::new(MARGIN_MM, ruler + 17.),
        size: 4.,
        text: "100 mm calibration ruler — print at actual size (100%)".into(),
    });
    Ok(out)
}

/// A PDF string literal of `text` in WinAnsi encoding (characters it lacks become '?').
fn pdf_string(text: &str) -> Vec<u8> {
    let mut out = b"(".to_vec();
    for c in text.chars() {
        let byte = match c {
            '—' => 0x97,
            '–' => 0x96,
            '•' => 0x95,
            c if (c as u32) < 0x80 || (0xa0..=0xff).contains(&(c as u32)) => c as u32 as u8,
            _ => b'?',
        };
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out.push(b')');
    out
}

/// A tile row's name: A, B, … Z, AA, AB, …
fn row_name(mut row: usize) -> String {
    let mut name = vec![];
    loop {
        name.push(b'A' + (row % 26) as u8);
        if row < 26 {
            break;
        }
        row = row / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap()
}

/// What a PDF holds.
#[derive(Debug, PartialEq)]
pub struct PdfSummary {
    pub pages: usize,
    pub rows: usize,
    pub columns: usize,
}

/// The template's pieces at 1:1 as a PDF on `paper`: its bytes and its pages.
pub fn print_pattern_pdf(
    template: &GarmentTemplate,
    paper: Paper,
) -> Result<(Vec<u8>, PdfSummary), String> {
    template.compile()?;
    let layout = print_layout(template)?;
    let marks = marks(template, &layout)?;
    const PT: f64 = 72. / 25.4;
    let sheet = DVec2::new(layout.width_mm, layout.height_mm);
    let (page, margin) = match paper.size_mm() {
        Some(size) => (size, PAGE_MARGIN_MM),
        None => (sheet, 0.),
    };
    let tile = page - DVec2::splat(2. * margin);
    let columns = (sheet.x / tile.x - 1e-9).ceil().max(1.) as usize;
    let rows = (sheet.y / tile.y - 1e-9).ceil().max(1.) as usize;
    // Each mark's extent, to leave out tiles with nothing on them.
    let extent = |m: &Mark| match m {
        Mark::Line { points, .. } => points.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        ),
        Mark::Text { at, size, text } => (
            *at - DVec2::new(0., *size),
            *at + DVec2::new(text.chars().count() as f64 * size * 0.6, 0.),
        ),
    };
    let extents: Vec<(DVec2, DVec2)> = marks.iter().map(extent).collect();
    let mut tiles = vec![];
    for r in 0..rows {
        for c in 0..columns {
            let lo = DVec2::new(c as f64, r as f64) * tile;
            let hi = lo + tile;
            if extents
                .iter()
                .any(|(a, b)| a.x <= hi.x && b.x >= lo.x && a.y <= hi.y && b.y >= lo.y)
            {
                tiles.push((r, c, lo));
            }
        }
    }
    let height_pt = page.y * PT;
    let mut contents = vec![];
    for (k, (r, c, lo)) in tiles.iter().enumerate() {
        let mut s = String::new();
        let mut bytes: Vec<u8> = vec![];
        if paper.size_mm().is_some() {
            // The tile's frame, to trim along, and its name.
            writeln!(
                s,
                "0.55 G 0.25 w [] 0 d {:.3} {:.3} {:.3} {:.3} re S",
                margin * PT,
                height_pt - (margin + tile.y) * PT,
                tile.x * PT,
                tile.y * PT
            )
            .unwrap();
            let label = format!(
                "{} · tile {}{} (row {} of {rows}, column {} of {columns}) · page {} of {} · print at actual size (100%)",
                template.name,
                row_name(*r),
                c + 1,
                row_name(*r),
                c + 1,
                k + 1,
                tiles.len(),
            );
            bytes.extend_from_slice(s.as_bytes());
            s.clear();
            bytes.extend_from_slice(
                format!(
                    "BT 0 g /F1 8 Tf {:.3} {:.3} Td ",
                    margin * PT,
                    height_pt - margin * PT * 0.6
                )
                .as_bytes(),
            );
            bytes.extend(pdf_string(&label));
            bytes.extend_from_slice(b" Tj ET\n");
        }
        // Sheet millimetres to page points, y upward, showing this tile.
        writeln!(
            s,
            "q {:.3} {:.3} {:.3} {:.3} re W n {PT:.6} 0 0 {:.6} {:.4} {:.4} cm",
            margin * PT,
            height_pt - (margin + tile.y) * PT,
            tile.x * PT,
            tile.y * PT,
            -PT,
            (margin - lo.x) * PT,
            height_pt - (margin - lo.y) * PT
        )
        .unwrap();
        bytes.extend_from_slice(s.as_bytes());
        for mark in &marks {
            let mut s = String::new();
            match mark {
                Mark::Line {
                    points,
                    closed,
                    style,
                } => {
                    if points.len() < 2 {
                        continue;
                    }
                    let (colour, width, dash) = match style {
                        Style::Sewing => ("0 0 0", 0.3, "[] 0"),
                        Style::Cutting => ("0.776 0.157 0.157", 0.35, "[3 2] 0"),
                        Style::Notch => ("0.776 0.157 0.157", 0.3, "[] 0"),
                        Style::Internal => ("0.333 0.333 0.333", 0.3, "[2 2] 0"),
                        Style::Ruler => ("0 0 0", 0.3, "[] 0"),
                    };
                    write!(s, "{colour} RG {width} w {dash} d").unwrap();
                    for (i, p) in points.iter().enumerate() {
                        write!(
                            s,
                            " {:.3} {:.3} {}",
                            p.x,
                            p.y,
                            if i == 0 { "m" } else { "l" }
                        )
                        .unwrap();
                    }
                    s.push_str(if *closed { " h S\n" } else { " S\n" });
                    bytes.extend_from_slice(s.as_bytes());
                }
                Mark::Text { at, size, text } => {
                    // Upright in the flipped space.
                    bytes.extend_from_slice(
                        format!("BT 0 g /F1 {size} Tf 1 0 0 -1 {:.3} {:.3} Tm ", at.x, at.y)
                            .as_bytes(),
                    );
                    bytes.extend(pdf_string(text));
                    bytes.extend_from_slice(b" Tj ET\n");
                }
            }
        }
        bytes.extend_from_slice(b"Q\n");
        contents.push(bytes);
    }
    // The file: catalog, page tree, font, information, then each page and its content.
    let n = tiles.len();
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n)
                .map(|k| format!("{} 0 R", 5 + 2 * k))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ];
    let mut info = b"<< /Producer (ClothCraft) /Title ".to_vec();
    info.extend(pdf_string(&format!("{} at 1:1", template.name)));
    info.extend_from_slice(b" >>");
    objects.push(info);
    for (k, content) in contents.into_iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.3} {:.3}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                page.x * PT,
                page.y * PT,
                6 + 2 * k
            )
            .into_bytes(),
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend(content);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut pdf = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = vec![];
    for (k, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", k + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    Ok((
        pdf,
        PdfSummary {
            pages: n,
            rows,
            columns,
        },
    ))
}
