//! Reviewed DXF contours become ordinary editable panels through one prepared edit.
//! Parsing and preparation never mutate the current document or its history.
use crate::{
    authoring::{self, Edit},
    garment::{
        GarmentTemplate, domain,
        dxf_import::{self, Drawing, Options},
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        template::{Panel, Placement},
    },
    model::Pattern,
};
use glam::{DVec2, DVec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub struct Source {
    pub path: PathBuf,
    text: String,
}
impl Source {
    /// Reads a DXF as UTF-8 or, when it is not, as Windows-1252: the code page R12 files
    /// usually declare (`$DWGCODEPAGE ANSI_1252`), with accented labels such as "Puño".
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("DXF exceeds the 16 MiB input limit".into());
        }
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err("This DXF is saved as UTF-16; save it as ASCII or UTF-8 text".into());
        }
        let text = String::from_utf8(bytes).unwrap_or_else(|e| windows_1252(e.as_bytes()));
        Ok(Self {
            path: path.to_owned(),
            text,
        })
    }
    pub fn inspect(&self, options: Options) -> Result<Drawing, String> {
        dxf_import::read_ascii(&self.text, options)
    }
}

/// Text in the Windows-1252 code page: Latin-1, but for the printable characters it puts at
/// 0x80–0x9F; the five bytes it leaves undefined become U+FFFD.
pub fn windows_1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{FFFD}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{FFFD}', 'Ž',
        '\u{FFFD}', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
        '\u{FFFD}', 'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

/// Cheap eligibility, not full contour validation. Apply validates the whole garment.
pub fn contour_issue(path: &dxf_import::Path) -> Option<&'static str> {
    if !path.closed {
        return Some("Open path — not a panel contour");
    }
    if !(4..=4097).contains(&path.points.len()) {
        return Some("Panel needs 3..4096 contour points");
    }
    if path.points.first() != path.points.last() {
        return Some("Closed path has no exact closing endpoint");
    }
    if path.points.iter().any(|p| !p.is_finite()) {
        return Some("Nonfinite contour coordinate");
    }
    None
}

fn intersects(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let eps = 1e-12;
    if a.min(b).x > c.max(d).x + eps
        || c.min(d).x > a.max(b).x + eps
        || a.min(b).y > c.max(d).y + eps
        || c.min(d).y > a.max(b).y + eps
    {
        return false;
    }
    let cross = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    cross(a, b, c) * cross(a, b, d) <= eps * eps && cross(c, d, a) * cross(c, d, b) <= eps * eps
}

/// Append explicitly selected closed paths. No inferred seams, holes or line stitching.
/// `origin` is the top-left 3D placement of a separated row chosen by the caller;
/// the caller must keep that row outside the avatar and current drape bounds.
/// How far above a piece its title block may be written, metres.
const TITLE_BLOCK_M: f64 = 0.06;

/// The title written on a piece: the tallest label inside its outline, inside the smallest
/// other outline round it (the cutting line round a sewing line), or in a title block just
/// above it that lies in no outline. Ties go to the label written first.
fn piece_name(drawing: &Drawing, path: &crate::garment::dxf_import::Path) -> Option<String> {
    let (lo, hi) = path
        .points
        .iter()
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let around = drawing
        .paths
        .iter()
        .filter(|o| {
            o.closed && !std::ptr::eq(*o, path) && domain::inside(path.points[0], &o.points)
        })
        .min_by(|a, b| {
            domain::area(&a.points)
                .abs()
                .total_cmp(&domain::area(&b.points).abs())
        });
    let top = around.map_or(hi.y, |o| {
        o.points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let in_any_outline = |at: DVec2| {
        drawing
            .paths
            .iter()
            .any(|o| o.closed && domain::inside(at, &o.points))
    };
    drawing
        .texts
        .iter()
        .filter(|t| {
            domain::inside(t.at, &path.points)
                || around.is_some_and(|o| domain::inside(t.at, &o.points))
                || (t.at.y > top
                    && t.at.y <= top + TITLE_BLOCK_M
                    && t.at.x >= lo.x - TITLE_BLOCK_M
                    && t.at.x <= hi.x
                    && !in_any_outline(t.at))
        })
        .fold(
            None,
            |best: Option<&crate::garment::dxf_import::Text>, t| match best {
                Some(b) if b.height_m >= t.height_m => Some(b),
                _ => Some(t),
            },
        )
        .map(|t| t.text.chars().take(80).collect())
}

pub fn prepare(
    source: &GarmentTemplate,
    drawing: &Drawing,
    selected: &[usize],
    fabric: &str,
    meshing: MeshingSettings,
    origin: DVec3,
) -> Result<Edit, String> {
    meshing.validate()?;
    if !origin.is_finite() {
        return Err("Import placement must be finite".into());
    }
    if selected.is_empty() || source.panels.len() + selected.len() > 128 {
        return Err("Choose at least one contour; the garment supports at most 128 panels".into());
    }
    if !source.fabrics.iter().any(|f| f.id == fabric) {
        return Err("Choose an existing fabric".into());
    }
    let mut seen = BTreeSet::new();
    let mut contours = vec![];
    for &index in selected {
        if !seen.insert(index) {
            return Err("A contour was selected twice".into());
        }
        let path = drawing.paths.get(index).ok_or("Stale contour selection")?;
        if let Some(reason) = contour_issue(path) {
            return Err(reason.into());
        }
        contours.push(path);
    }
    // Different source outlines can be sewing/cutting alternatives or holes.
    // Reject their simultaneous import rather than inventing their relationship.
    let mut comparisons = 0usize;
    for (i, a) in contours.iter().enumerate() {
        for b in contours.iter().skip(i + 1) {
            if domain::inside(a.points[0], &b.points) || domain::inside(b.points[0], &a.points) {
                return Err(
                    "Selected contours are nested/overlapping; select an unambiguous outline"
                        .into(),
                );
            }
            for aa in a.points.windows(2) {
                for bb in b.points.windows(2) {
                    comparisons += 1;
                    if comparisons > 2_000_000 {
                        return Err(
                            "Contour comparison budget exceeded; import fewer pieces at once"
                                .into(),
                        );
                    }
                    if intersects(aa[0], aa[1], bb[0], bb[1]) {
                        return Err(
                            "Selected contours touch or overlap; their relationship is ambiguous"
                                .into(),
                        );
                    }
                }
            }
        }
    }
    let mut template = source.clone();
    let mut x = 0.;
    let board_x = source
        .panels
        .iter()
        .enumerate()
        .map(|(i, p)| authoring::layout(source, i).x + p.pattern.width)
        .fold(0., f64::max)
        + 0.15;
    let mut selected_id = None;
    for path in contours {
        let mut points = path.points[..path.points.len() - 1].to_vec();
        let lo = points
            .iter()
            .copied()
            .fold(DVec2::splat(f64::INFINITY), DVec2::min);
        let hi = points
            .iter()
            .copied()
            .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
        let size = hi - lo;
        for p in &mut points {
            // CAD uses +Y up; material V maps downward in the editor.
            *p = glam::DVec2::new(p.x - lo.x, hi.y - p.y);
        }
        if domain::area(&points) < 0. {
            points.reverse();
        }
        let mut serial = 1;
        let id = loop {
            let id = format!("dxf_{serial}");
            if !template.panels.iter().any(|p| p.id == id) {
                break id;
            }
            serial += 1;
        };
        let polygon = PolygonPattern {
            edges: (0..points.len())
                .map(|i| format!("edge_{}", i + 1))
                .collect(),
            points,
            subdivisions: 0,
            meshing: Some(meshing.clone()),
            curves: BTreeMap::new(),
            holes: vec![],
            lines: vec![],
            target_lengths: BTreeMap::new(),
            seam_allowances: BTreeMap::new(),
            notches: BTreeMap::new(),
        };
        template.panels.push(Panel {
            id: id.clone(),
            name: piece_name(drawing, path).unwrap_or_else(|| format!("{} · {}", path.layer, id)),
            pattern: Pattern {
                width: size.x,
                height: size.y,
                nx: 1,
                ny: 1,
            },
            polygon: Some(polygon),
            fabric: fabric.into(),
            grain_degrees: 90.,
            placement: Placement {
                translation: origin + DVec3::new(x, 0., 0.),
                ..Default::default()
            },
            layout_2d: Some(DVec2::new(board_x + x, 0.)),
            layout_rotation_degrees: 0.,
            pins: vec![],
            pin_points: vec![],
        });
        x += size.x + 0.1;
        selected_id = Some(id);
    }
    template.version = 2;
    let garment = template.compile()?;
    Ok(Edit {
        template,
        garment,
        selected: selected_id,
        reset_simulation: true,
        new_point: None,
    })
}
