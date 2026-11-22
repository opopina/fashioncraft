//! Writing DXF-AAMA and DXF-ASTM: an R12 drawing with one block per piece.
//!
//! Each piece block holds its cut line (layer 1, closed POLYLINE), sew line (14), cutouts
//! (11) and internal lines (8), a turn point (2) or curve point (3) on every point of those
//! lines, its notches as points on the cut line (4) carrying depth, opening and angle, its
//! grain line (7) and its Piece System Text on layer 1. The Style System Text (style, date,
//! author, sample size, units and, for ASTM, the version line) is written once in the
//! drawing. All text is 7-bit ASCII. Sources: `docs/dxf-aama-astm.md`.
use super::{Dialect, Line, NotchShape, Piece, Ring, Units, layer};
use glam::DVec2;
use std::fmt::Write as _;

/// The drawing-wide facts written once, before the pieces.
#[derive(Clone, Debug, Default)]
pub struct WriteOptions {
    pub dialect: Dialect,
    pub units: Units,
    pub style: String,
    /// The size the pieces are drawn in.
    pub sample_size: String,
    /// `<vendor>;<application>;<release>`.
    pub author: String,
    /// Creation date (`dd-mm-yyyy`) and time (`hh:mm`); `None` leaves them empty.
    pub created: Option<(String, String)>,
}

/// The version line of the ASTM Style System Text. AAMA's own version line has no public
/// description, so AAMA files go without one; a reader tells them by its absence.
pub const ASTM_VERSION: &str = "ASTM/D13 Proposal 1 Version: D 6673-04";
/// How far, in millimetres, a curve's points may be from the curve: FashionCraft samples
/// curves to 0.25 mm, like its 1:1 prints.
const CURVE_TOLERANCE_MM: f64 = 0.25;
/// Text height: 5 mm, as recommended for annotation.
const TEXT_M: f64 = 0.005;
/// Space between pieces laid out in a row, metres.
const GAP_M: f64 = 0.02;

struct Out {
    text: String,
    scale: f64,
    decimals: usize,
}

impl Out {
    fn pair(&mut self, code: i32, value: impl std::fmt::Display) {
        writeln!(self.text, "{code:>3}\n{value}").unwrap();
    }
    /// A length or coordinate in metres, written in the file's unit.
    fn number(&mut self, code: i32, metres: f64) {
        let value = metres / self.scale;
        let mut s = format!("{value:.*}", self.decimals);
        if s.starts_with('-') && s[1..].bytes().all(|b| b == b'0' || b == b'.') {
            s.remove(0);
        }
        self.pair(code, s);
    }
    fn xy(&mut self, code: i32, p: DVec2) {
        self.number(code, p.x);
        self.number(code + 10, p.y);
        self.number(code + 20, 0.);
    }
    fn polyline(&mut self, layer: u32, points: &[DVec2], closed: bool) {
        self.pair(0, "POLYLINE");
        self.pair(8, layer);
        self.pair(66, 1);
        self.xy(10, DVec2::ZERO);
        self.pair(70, i32::from(closed));
        for p in points {
            self.pair(0, "VERTEX");
            self.pair(8, layer);
            self.xy(10, *p);
        }
        self.pair(0, "SEQEND");
        self.pair(8, layer);
    }
    fn line(&mut self, layer: u32, a: DVec2, b: DVec2) {
        self.pair(0, "LINE");
        self.pair(8, layer);
        self.xy(10, a);
        self.xy(11, b);
    }
    fn point(&mut self, layer: u32, p: DVec2) {
        self.pair(0, "POINT");
        self.pair(8, layer);
        self.xy(10, p);
    }
    fn label(&mut self, layer: u32, at: DVec2, text: &str) {
        self.pair(0, "TEXT");
        self.pair(8, layer);
        self.xy(10, at);
        self.number(40, TEXT_M);
        // A release-12 text value holds at most 255 characters.
        let text: String = ascii(text).chars().take(250).collect();
        self.pair(1, text);
    }
    /// The turn and curve points of a line's points.
    fn marks(&mut self, points: &[DVec2], turns: &[bool]) {
        for (p, turn) in points.iter().zip(turns) {
            let layer = if *turn {
                layer::TURN_POINTS
            } else {
                layer::CURVE_POINTS
            };
            self.point(layer, *p);
        }
    }
    fn ring(&mut self, line_layer: u32, ring: &Ring) {
        self.polyline(line_layer, &ring.points, true);
        self.marks(&ring.points, &ring.turns);
    }
}

/// The layer a notch shape is written on: slits and V notches on 4, the ASTM shapes on
/// 80–83. AAMA has only layer 4.
fn notch_layer(dialect: Dialect, shape: NotchShape) -> u32 {
    match (dialect, shape) {
        (Dialect::Aama, _) | (Dialect::Astm, NotchShape::Slit | NotchShape::V) => layer::NOTCHES,
        (Dialect::Astm, NotchShape::T) => layer::T_NOTCH,
        (Dialect::Astm, NotchShape::Castle) => layer::CASTLE_NOTCH,
        (Dialect::Astm, NotchShape::Check) => layer::CHECK_NOTCH,
        (Dialect::Astm, NotchShape::U) => layer::U_NOTCH,
    }
}

/// 7-bit ASCII: accents dropped from the letters that have them, other characters `?`.
pub fn ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'O',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            '–' | '—' => '-',
            '‘' | '’' => '\'',
            '“' | '”' => '"',
            '·' => '-',
            '×' => 'x',
            c if c.is_ascii() && !c.is_ascii_control() => c,
            _ => '?',
        })
        .collect()
}

/// A block name, `NAME_SIZE` by custom: 31 letters, digits, `-` and `_` at most.
pub(super) fn block_name(name: &str, size: &str, taken: &[String]) -> String {
    let clean = |s: &str| -> String {
        ascii(s)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect()
    };
    let mut base = clean(name);
    if base.trim_matches('_').is_empty() {
        base = "PIECE".into();
    }
    let size = clean(size);
    base.truncate(if size.is_empty() {
        27
    } else {
        26usize.saturating_sub(size.len()).max(12)
    });
    if !size.is_empty() {
        base = format!("{base}_{size}");
        base.truncate(27);
    }
    let mut name = base.clone();
    let mut serial = 2;
    while taken.contains(&name) {
        name = format!("{base}_{serial}");
        serial += 1;
    }
    name
}

fn bounds(points: impl Iterator<Item = DVec2>) -> (DVec2, DVec2) {
    points.fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    )
}

fn check_ring(name: &str, what: &str, ring: &Ring) -> Result<(), String> {
    if ring.points.len() < 3
        || ring.turns.len() != ring.points.len()
        || ring.points.iter().any(|p| !p.is_finite())
    {
        return Err(format!(
            "Piece {name}: its {what} needs 3 or more finite points"
        ));
    }
    Ok(())
}

/// Write the pieces as an apparel DXF. Pieces are laid out side by side, each in its own
/// block, with the drawing's texts below them.
pub fn write(pieces: &[Piece], options: &WriteOptions) -> Result<Vec<u8>, String> {
    if pieces.is_empty() {
        return Err("There are no pieces to write".into());
    }
    for piece in pieces {
        check_ring(&piece.name, "cut line", &piece.cut)?;
        if let Some(sew) = &piece.sew {
            check_ring(&piece.name, "sew line", sew)?;
        }
        for ring in piece.cutouts.iter().chain(&piece.cutout_sews) {
            check_ring(&piece.name, "cutout", ring)?;
        }
        if piece.quantity.total() == 0 {
            return Err(format!("Piece {}: quantity must be at least 1", piece.name));
        }
    }
    let (scale, decimals) = match options.units {
        Units::Millimetres => (0.001, 2),
        Units::Inches => (0.0254, 4),
    };
    // Each piece moves so its cut line starts at the block origin; the inserts make a row.
    let mut placed = vec![];
    let mut x = 0.;
    let mut extent = (DVec2::ZERO, DVec2::ZERO);
    for piece in pieces {
        let (lo, hi) = bounds(piece.cut.points.iter().copied());
        placed.push((lo, DVec2::new(x, 0.)));
        extent.1 = extent.1.max(DVec2::new(x, 0.) + (hi - lo));
        x += hi.x - lo.x + GAP_M;
    }
    let mut style_text = vec![
        format!("Style Name: {}", options.style),
        format!(
            "Creation Date: {}",
            options.created.as_ref().map_or("", |c| c.0.as_str())
        ),
        format!(
            "Creation Time: {}",
            options.created.as_ref().map_or("", |c| c.1.as_str())
        ),
        format!("Author: {}", options.author),
        format!("Sample Size: {}", options.sample_size),
        "Grade Rule Table:".to_string(),
        format!("Units: {}", options.units.word()),
    ];
    if options.dialect == Dialect::Astm {
        style_text.push(ASTM_VERSION.into());
    }
    let tolerance = CURVE_TOLERANCE_MM * 0.001 / scale;
    style_text.push(format!("Curve Tolerance: {tolerance:.*}", decimals));
    extent.0.y = -(style_text.len() as f64 + 1.) * TEXT_M * 1.6;

    let mut out = Out {
        text: String::new(),
        scale,
        decimals,
    };
    out.pair(999, "FashionCraft apparel pattern export");
    out.pair(0, "SECTION");
    out.pair(2, "HEADER");
    out.pair(9, "$ACADVER");
    out.pair(1, "AC1009");
    out.pair(9, "$INSBASE");
    out.xy(10, DVec2::ZERO);
    out.pair(9, "$EXTMIN");
    out.xy(10, extent.0);
    out.pair(9, "$EXTMAX");
    out.xy(10, extent.1);
    // For general CAD viewers; apparel readers take the units from the `Units:` text.
    out.pair(9, "$MEASUREMENT");
    out.pair(70, i32::from(options.units == Units::Millimetres));
    out.pair(9, "$INSUNITS");
    out.pair(70, options.units.insunits());
    out.pair(0, "ENDSEC");

    out.pair(0, "SECTION");
    out.pair(2, "TABLES");
    out.pair(0, "TABLE");
    out.pair(2, "LTYPE");
    out.pair(70, 1);
    out.pair(0, "LTYPE");
    out.pair(2, "CONTINUOUS");
    out.pair(70, 0);
    out.pair(3, "Solid line");
    out.pair(72, 65);
    out.pair(73, 0);
    out.pair(40, "0.0");
    out.pair(0, "ENDTAB");
    let mut layers = vec![
        layer::BOUNDARY,
        layer::TURN_POINTS,
        layer::CURVE_POINTS,
        layer::NOTCHES,
        layer::GRAIN,
        layer::INTERNAL,
        layer::CUTOUTS,
        layer::SEW,
        layer::ANNOTATION,
    ];
    if options.dialect == Dialect::Astm {
        layers.extend([
            layer::T_NOTCH,
            layer::CASTLE_NOTCH,
            layer::CHECK_NOTCH,
            layer::U_NOTCH,
        ]);
    }
    out.pair(0, "TABLE");
    out.pair(2, "LAYER");
    out.pair(70, layers.len() + 1);
    for name in std::iter::once("0".to_string()).chain(layers.iter().map(u32::to_string)) {
        out.pair(0, "LAYER");
        out.pair(2, name);
        out.pair(70, 0);
        out.pair(62, 7);
        out.pair(6, "CONTINUOUS");
    }
    out.pair(0, "ENDTAB");
    out.pair(0, "TABLE");
    out.pair(2, "STYLE");
    out.pair(70, 1);
    out.pair(0, "STYLE");
    out.pair(2, "STANDARD");
    out.pair(70, 0);
    out.pair(40, "0.0");
    out.pair(41, "1.0");
    out.pair(50, "0.0");
    out.pair(71, 0);
    out.number(42, TEXT_M);
    out.pair(3, "txt");
    out.pair(4, "");
    out.pair(0, "ENDTAB");
    out.pair(0, "ENDSEC");

    out.pair(0, "SECTION");
    out.pair(2, "BLOCKS");
    let mut names: Vec<String> = vec![];
    for (piece, (lo, _)) in pieces.iter().zip(&placed) {
        let name = block_name(&piece.name, &piece.size, &names);
        names.push(name.clone());
        let shift = |p: DVec2| p - *lo;
        let moved = |r: &Ring| Ring {
            points: r.points.iter().map(|p| shift(*p)).collect(),
            turns: r.turns.clone(),
        };
        out.pair(0, "BLOCK");
        out.pair(8, 0);
        out.pair(2, &name);
        out.pair(70, 0);
        out.xy(10, DVec2::ZERO);
        out.pair(3, &name);
        out.ring(layer::BOUNDARY, &moved(&piece.cut));
        if let Some(sew) = &piece.sew {
            out.ring(layer::SEW, &moved(sew));
        }
        for ring in &piece.cutouts {
            out.ring(layer::CUTOUTS, &moved(ring));
        }
        for ring in &piece.cutout_sews {
            out.ring(layer::SEW, &moved(ring));
        }
        for Line {
            points,
            turns,
            closed,
        } in &piece.internal
        {
            if points.len() >= 2 {
                let points: Vec<_> = points.iter().map(|p| shift(*p)).collect();
                out.polyline(layer::INTERNAL, &points, *closed);
                out.marks(&points, turns);
            }
        }
        // A notch is a point on the cut line: Z its depth, thickness its opening, angle the
        // direction it cuts in, counter-clockwise from +X.
        for notch in &piece.notches {
            let direction = notch.tip - notch.base;
            out.pair(0, "POINT");
            out.pair(8, notch_layer(options.dialect, notch.shape));
            let base = shift(notch.base);
            out.number(10, base.x);
            out.number(20, base.y);
            out.number(30, direction.length());
            if notch.width > 0. {
                out.number(39, notch.width);
            }
            out.pair(
                50,
                format!("{:.4}", direction.y.atan2(direction.x).to_degrees()),
            );
        }
        if let Some([a, b]) = piece.grain {
            out.line(layer::GRAIN, shift(a), shift(b));
        }
        for p in &piece.drill_holes {
            out.point(layer::DRILL_HOLES, shift(*p));
        }
        // The Piece System Text stacks downward from the middle of the piece; other texts
        // go on the annotation layer.
        let (a, b) = bounds(piece.sew_line().points.iter().map(|p| shift(*p)));
        let mut at = DVec2::new(a.x + (b.x - a.x) * 0.25, (a.y + b.y) * 0.5 + TEXT_M * 2.);
        let quantity = piece.quantity;
        let mut texts = vec![
            (layer::BOUNDARY, format!("Piece Name: {}", piece.name)),
            (
                layer::BOUNDARY,
                format!("Quantity: {},{}", quantity.right, quantity.left),
            ),
        ];
        if !piece.size.is_empty() {
            texts.push((layer::BOUNDARY, format!("Size: {}", piece.size)));
        }
        if !piece.material.is_empty() {
            texts.push((layer::BOUNDARY, format!("Material: {}", piece.material)));
        }
        texts.extend(
            piece
                .annotations
                .iter()
                .map(|a| (layer::ANNOTATION, a.clone())),
        );
        for (layer, text) in texts {
            out.label(layer, at, &text);
            at.y -= TEXT_M * 1.6;
        }
        out.pair(0, "ENDBLK");
        out.pair(8, 0);
    }
    out.pair(0, "ENDSEC");

    out.pair(0, "SECTION");
    out.pair(2, "ENTITIES");
    let mut at = DVec2::new(0., -TEXT_M * 1.6);
    for text in style_text {
        out.label(layer::BOUNDARY, at, &text);
        at.y -= TEXT_M * 1.6;
    }
    for (name, (_, insert)) in names.iter().zip(&placed) {
        out.pair(0, "INSERT");
        out.pair(8, layer::BOUNDARY);
        out.pair(2, name);
        out.xy(10, *insert);
    }
    out.pair(0, "ENDSEC");
    out.pair(0, "EOF");
    Ok(out.text.into_bytes())
}
