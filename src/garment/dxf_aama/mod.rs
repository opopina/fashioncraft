//! DXF-AAMA and DXF-ASTM (ASTM D6673) apparel pattern exchange: the format layer.
//!
//! The two dialects are the files pattern CAD systems, plotters and cutters pass pieces in.
//! Both are ASCII DXF release 12 drawings in which each pattern piece is one BLOCK, placed in
//! the drawing by an INSERT, with its lines sorted onto numbered layers (cut line on 1, turn
//! and curve points on 2 and 3, notches on 4, grain line on 7, internal lines on 8, internal
//! cutouts on 11, drill holes on 13, sew line on 14, annotation on 15) and its name, size and
//! quantity written as `Key: value` texts. ASTM D6673 keeps the AAMA layers and adds layers
//! for notch shapes (80–83). [`write`] writes pieces in either dialect; [`read`] reads both,
//! tells which one a file is, and lists what it could not take instead of dropping it.
//!
//! This module only knows the file: pieces are plain geometry in metres with DXF's +Y up.
//! [`template`] turns garment panels into pieces and pieces back into panels.
//!
//! Clean-room: written from public prose descriptions of the formats, never from a vendor's
//! files or code; ASTM D6673 itself is a paid standard that was not consulted. Where each rule
//! comes from is listed in `docs/dxf-aama-astm.md`.
use glam::DVec2;

mod read;
pub mod template;
mod write;

pub use read::{ReadOptions, detect, is_apparel, load, read};
pub use write::{WriteOptions, ascii, write};

/// Which of the two apparel DXF dialects a file follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    /// The AAMA DXF interchange format (American Apparel Manufacturers Association, 1997).
    #[default]
    Aama,
    /// ASTM D6673, the AAMA format as an ASTM practice, with notch-shape layers.
    Astm,
}

impl Dialect {
    pub const ALL: [Self; 2] = [Self::Aama, Self::Astm];
    pub fn label(self) -> &'static str {
        match self {
            Self::Aama => "DXF-AAMA",
            Self::Astm => "DXF-ASTM (D6673)",
        }
    }
}

/// The unit a file's coordinates are in: metric files use millimetres, english files inches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Units {
    #[default]
    Millimetres,
    Inches,
}

impl Units {
    pub const ALL: [Self; 2] = [Self::Millimetres, Self::Inches];
    pub fn metres(self) -> f64 {
        match self {
            Self::Millimetres => 0.001,
            Self::Inches => 0.0254,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Millimetres => "Metric (mm)",
            Self::Inches => "English (inches)",
        }
    }
    /// The word the `Units:` text uses for this unit.
    fn word(self) -> &'static str {
        match self {
            Self::Millimetres => "METRIC",
            Self::Inches => "ENGLISH",
        }
    }
    /// `$INSUNITS` code: 4 millimetres, 1 inches.
    fn insunits(self) -> i32 {
        match self {
            Self::Millimetres => 4,
            Self::Inches => 1,
        }
    }
}

/// The numbered layers of the AAMA layer table; ASTM D6673 adds 80–83.
pub mod layer {
    pub const BOUNDARY: u32 = 1;
    pub const TURN_POINTS: u32 = 2;
    pub const CURVE_POINTS: u32 = 3;
    pub const NOTCHES: u32 = 4;
    pub const GRADE_REFERENCE: u32 = 5;
    pub const MIRROR: u32 = 6;
    pub const GRAIN: u32 = 7;
    pub const INTERNAL: u32 = 8;
    pub const STRIPE: u32 = 9;
    pub const PLAID: u32 = 10;
    pub const CUTOUTS: u32 = 11;
    pub const DRILL_HOLES: u32 = 13;
    pub const SEW: u32 = 14;
    pub const ANNOTATION: u32 = 15;
    pub const T_NOTCH: u32 = 80;
    pub const CASTLE_NOTCH: u32 = 81;
    pub const CHECK_NOTCH: u32 = 82;
    pub const U_NOTCH: u32 = 83;
}

/// The shape a cutter gives a notch. AAMA has one notch layer, read as slits; ASTM adds a
/// layer per shape. FashionCraft draws slits and V notches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotchShape {
    Slit,
    V,
    T,
    Castle,
    Check,
    U,
}

impl NotchShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Slit => "slit",
            Self::V => "V",
            Self::T => "T",
            Self::Castle => "castle",
            Self::Check => "check",
            Self::U => "U",
        }
    }
}

/// One notch: from its base on the cut line, `depth` metres into the piece along `tip - base`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotchMark {
    pub base: DVec2,
    /// The notch's far end: `base` moved by its depth along its direction.
    pub tip: DVec2,
    /// Opening at the cut line, metres; zero for a slit.
    pub width: f64,
    pub shape: NotchShape,
}

/// How many of a piece to cut: as drawn (right) and mirrored (left), the `R,L` of the
/// `Quantity:` text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quantity {
    pub right: u32,
    pub left: u32,
}

impl Default for Quantity {
    fn default() -> Self {
        Self { right: 1, left: 0 }
    }
}

impl Quantity {
    pub fn total(self) -> u32 {
        self.right + self.left
    }
    /// `3` or `1,1`; `None` when it is neither or cuts nothing.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split(',').map(|p| p.trim().parse::<u32>());
        let right = parts.next()?.ok()?;
        let left = match parts.next() {
            Some(left) => left.ok()?,
            None => 0,
        };
        (parts.next().is_none() && right + left > 0).then_some(Self { right, left })
    }
}

/// A closed line, its first point not repeated, with which points are turn points (corners)
/// rather than curve points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ring {
    pub points: Vec<DVec2>,
    pub turns: Vec<bool>,
}

/// An internal line: open, or closed with its first point not repeated; turn flags as in
/// [`Ring`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub points: Vec<DVec2>,
    pub turns: Vec<bool>,
    pub closed: bool,
}

/// One pattern piece, metres, +Y up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Piece {
    pub name: String,
    pub size: String,
    pub quantity: Quantity,
    /// What the piece is cut from; FashionCraft writes the fabric's name.
    pub material: String,
    /// The cut line (layer 1).
    pub cut: Ring,
    /// The sew line (layer 14); `None` when it is the cut line (no seam allowance).
    pub sew: Option<Ring>,
    /// Internal cutouts' cut lines (layer 11).
    pub cutouts: Vec<Ring>,
    /// Internal cutouts' sew lines (inner rings on layer 14), when they differ from the cut.
    pub cutout_sews: Vec<Ring>,
    pub internal: Vec<Line>,
    pub notches: Vec<NotchMark>,
    /// From the first point to the second: the warp's direction.
    pub grain: Option<[DVec2; 2]>,
    pub drill_holes: Vec<DVec2>,
    /// Texts of the piece that are not one of its known keys, kept to show.
    pub annotations: Vec<String>,
    /// Grade rule numbers (`# n` texts) and the points they grade.
    pub grade_points: Vec<GradePoint>,
    /// The grade reference line (layer 5): the X axis of the piece's grade rules, from its
    /// first point to its second. Without one, rules follow the drawing's X axis.
    pub grade_reference: Option<[DVec2; 2]>,
}

/// A grade rule number on a point: `# rule` written at `at` on layer 5, or on layer 4 for a
/// notch. The rule's growths are in the rule table (`.RUL`) beside the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradePoint {
    pub at: DVec2,
    pub rule: i64,
    pub notch: bool,
}

impl GradePoint {
    /// The rule number of a `# n` text (spaces optional, an alternate `n,m` keeps `n`).
    pub fn parse(text: &str) -> Option<i64> {
        let rest = text.trim().strip_prefix('#')?.trim();
        let first = rest.split(',').next()?.trim();
        first.parse().ok()
    }
}

impl Piece {
    /// The material outline: the sew line, or the cut line when there is none.
    pub fn sew_line(&self) -> &Ring {
        self.sew.as_ref().unwrap_or(&self.cut)
    }
}

/// Something in a file that was not imported, with where it is and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// The piece (block) it belongs to; empty for the drawing itself.
    pub piece: String,
    pub layer: String,
    pub entity: String,
    pub reason: String,
    /// How many entities share this piece, layer, entity and reason.
    pub count: usize,
}

/// A read file: its dialect and units as found, the drawing's own texts and the pieces.
#[derive(Clone, Debug)]
pub struct PatternFile {
    pub dialect: Dialect,
    /// What the dialect was told by: a version text, ASTM-only layers, or neither.
    pub dialect_reason: &'static str,
    /// The version text, as written.
    pub version: String,
    pub units: Option<Units>,
    pub metres_per_unit: f64,
    /// Where the scale came from: the `Units:` text, `$INSUNITS` or the caller.
    pub units_source: &'static str,
    pub style: String,
    pub sample_size: String,
    pub author: String,
    pub pieces: Vec<Piece>,
    pub issues: Vec<Issue>,
}

/// The texts' `Key: value` form, key in capitals with single spaces, value trimmed.
pub(crate) fn key_value(text: &str) -> Option<(String, &str)> {
    let (key, value) = text.split_once(':')?;
    let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
    (!key.is_empty() && key.len() <= 40).then(|| (key.to_uppercase(), value.trim()))
}

#[cfg(test)]
mod tests;
