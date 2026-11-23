use super::polygon::PolygonPattern;
use super::seam::{BoundaryCurve, BoundaryRef, CompiledSeam, SeamSpec, Side};
use crate::{
    energy,
    model::{Material, Mesh, Pattern, Scene, State},
};
use glam::{DQuat, DVec2, DVec3, EulerRot};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::Path,
};

/// Fabric properties are independent of how an individual panel is cut on the roll.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FabricProperties {
    pub warp: f64,
    pub weft: f64,
    pub shear: f64,
    pub compression_ratio: f64,
    pub compression_transition: f64,
    pub bend_warp: f64,
    pub bend_weft: f64,
    pub density: f64,
    pub thickness: f64,
    /// Distance (m) the solver keeps between this fabric and anything else; none uses the
    /// scene's collision distance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_distance: Option<f64>,
    /// Internal damping time, s (see `Material::internal_damping`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub internal_damping: f64,
}
impl Default for FabricProperties {
    fn default() -> Self {
        Self::from(&Material::default())
    }
}
impl From<&Material> for FabricProperties {
    fn from(m: &Material) -> Self {
        Self {
            warp: m.warp,
            weft: m.weft,
            shear: m.shear,
            compression_ratio: m.compression_ratio,
            compression_transition: m.compression_transition,
            bend_warp: m.bend_warp,
            bend_weft: m.bend_weft,
            density: m.density,
            thickness: m.thickness,
            collision_distance: m.collision_distance,
            internal_damping: m.internal_damping,
        }
    }
}
impl FabricProperties {
    pub fn bind(&self, grain_degrees: f64) -> Material {
        Material {
            warp: self.warp,
            weft: self.weft,
            shear: self.shear,
            compression_ratio: self.compression_ratio,
            compression_transition: self.compression_transition,
            bend_warp: self.bend_warp,
            bend_weft: self.bend_weft,
            density: self.density,
            thickness: self.thickness,
            grain_degrees,
            collision_distance: self.collision_distance,
            internal_damping: self.internal_damping,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fabric {
    pub id: String,
    pub name: String,
    pub properties: FabricProperties,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub translation: DVec3,
    /// R = Rx * Ry * Rz (column vectors); rotates the local (u,-v,0) panel.
    pub rotation_degrees: DVec3,
}
impl Default for Placement {
    fn default() -> Self {
        Self {
            translation: DVec3::ZERO,
            rotation_degrees: DVec3::ZERO,
        }
    }
}
impl Placement {
    pub fn transform(&self, p: DVec3) -> DVec3 {
        self.rotation() * p + self.translation
    }
    /// The placement's rotation.
    pub fn rotation(&self) -> DQuat {
        let r = self.rotation_degrees * (std::f64::consts::PI / 180.0);
        DQuat::from_euler(EulerRot::XYZ, r.x, r.y, r.z)
    }
    /// The rigid motion `(q, t)` taking what this placement puts in the world to where `to`
    /// puts it: a world point `x` goes to `q * x + t`.
    pub fn motion_to(&self, to: &Placement) -> (DQuat, DVec3) {
        let q = to.rotation() * self.rotation().inverse();
        (q, to.translation - q * self.translation)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub id: String,
    pub name: String,
    pub pattern: Pattern,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<PolygonPattern>,
    pub fabric: String,
    pub grain_degrees: f64,
    pub placement: Placement,
    /// Position of the local material origin on the unified 2D drafting board.
    /// Presentation only: it never transforms rest geometry or 3D placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_2d: Option<DVec2>,
    /// Board presentation only; material coordinates and grain are unchanged.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub layout_rotation_degrees: f64,
    #[serde(default)]
    pub pins: Vec<Corner>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pin_points: Vec<DVec2>,
}
fn is_zero(value: &f64) -> bool {
    *value == 0.
}
impl Panel {
    pub fn boundary_vertices(&self, side: Side, offset: usize) -> Vec<usize> {
        let (nx, ny) = (self.pattern.nx, self.pattern.ny);
        let vertices: Vec<_> = match side {
            Side::Top => (0..=nx).collect(),
            Side::Right => (0..=ny).map(|j| j * (nx + 1) + nx).collect(),
            Side::Bottom => (0..=nx).rev().map(|i| ny * (nx + 1) + i).collect(),
            Side::Left => (0..=ny).rev().map(|j| j * (nx + 1)).collect(),
        };
        vertices.into_iter().map(|i| i + offset).collect()
    }
    fn corner(&self, corner: Corner) -> usize {
        let (nx, ny) = (self.pattern.nx, self.pattern.ny);
        match corner {
            Corner::TopLeft => 0,
            Corner::TopRight => nx,
            Corner::BottomRight => ny * (nx + 1) + nx,
            Corner::BottomLeft => ny * (nx + 1),
        }
    }
}

/// Optional cylindrical or elliptical wrap of a panel's flat arrangement, applied in the panel's
/// local frame before its `Placement` (EXO-AV-002). Only initial positions change: rest
/// geometry, material coordinates and grain are untouched. The panel bends around an axis
/// parallel to local `y` through local `x = axis_x_m`; a positive radius curves toward local −z,
/// a negative one toward +z. With `half_width_m` the section is an ellipse whose semi-axis
/// across the panel is that and whose semi-axis in depth is the radius; the panel lies on it by
/// arc length. Beyond `max_angle_degrees` (of the ellipse's parameter) from the axis the panel
/// continues along the tangent.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelWrap {
    pub axis_x_m: f64,
    pub radius_m: f64,
    pub max_angle_degrees: f64,
    /// Semi-axis across the panel of an elliptical section; absent for a circle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub half_width_m: Option<f64>,
}

impl PanelWrap {
    pub fn validate(&self) -> Result<(), String> {
        if !self.axis_x_m.is_finite() || !self.radius_m.is_finite() || self.radius_m.abs() < 0.01 {
            return Err("Panel wrap needs a finite radius of at least 10 mm".into());
        }
        if !(self.max_angle_degrees > 0.0 && self.max_angle_degrees <= 180.0) {
            return Err("Panel wrap angle must be in (0°, 180°]".into());
        }
        if self
            .half_width_m
            .is_some_and(|a| !a.is_finite() || a < 0.01)
        {
            return Err("An elliptical panel wrap needs a half width of at least 10 mm".into());
        }
        Ok(())
    }

    /// Bend a local point `(u, −v, 0)` around the wrap axis.
    pub fn bend(&self, p: DVec3) -> DVec3 {
        if let Some(a) = self.half_width_m {
            return self.bend_ellipse(p, a);
        }
        let r = self.radius_m.abs();
        let s = self.radius_m.signum();
        let dx = p.x - self.axis_x_m;
        let max = self.max_angle_degrees.to_radians();
        let theta = dx / r;
        let (t, rest) = if theta.abs() <= max {
            (theta, 0.0)
        } else {
            (max * theta.signum(), (theta.abs() - max) * r)
        };
        let on_arc = DVec3::new(self.axis_x_m + r * t.sin(), p.y, -s * r * (1.0 - t.cos()));
        let tangent = DVec3::new(t.cos(), 0.0, -s * t.sin()) * dx.signum();
        on_arc + tangent * rest + DVec3::Z * p.z
    }

    /// The ellipse `(a sin t, −s b (1 − cos t))`, walked by arc length from the axis.
    fn bend_ellipse(&self, p: DVec3, a: f64) -> DVec3 {
        const STEPS: usize = 256;
        let b = self.radius_m.abs();
        let s = self.radius_m.signum();
        let dx = p.x - self.axis_x_m;
        let target = dx.abs();
        let max = self.max_angle_degrees.to_radians();
        let speed = |t: f64| (a * a * t.cos().powi(2) + b * b * t.sin().powi(2)).sqrt();
        let h = max / STEPS as f64;
        let (mut t, mut walked) = (0.0, 0.0);
        let mut rest = 0.0;
        loop {
            if t >= max - 1e-15 {
                rest = target - walked;
                t = max;
                break;
            }
            // Simpson's rule over one step.
            let segment = h / 6. * (speed(t) + 4. * speed(t + h / 2.) + speed(t + h));
            if walked + segment >= target {
                t += h * (target - walked) / segment;
                break;
            }
            walked += segment;
            t += h;
        }
        let side = if dx < 0. { -1. } else { 1. };
        let on_arc = DVec3::new(
            self.axis_x_m + side * a * t.sin(),
            p.y,
            -s * b * (1.0 - t.cos()),
        );
        let tangent = DVec3::new(side * a * t.cos(), 0.0, -s * b * t.sin()).normalize_or_zero();
        on_arc + tangent * rest.max(0.) + DVec3::Z * p.z
    }
}

/// Archived pieces with the seams among them, as they were when archived.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedPieces {
    pub id: String,
    /// The pieces' ids and names when archived, in the order the archive holds them.
    pub ids: Vec<String>,
    pub names: Vec<String>,
    /// Seams between these pieces and pieces left on the board, sewn again on restore when
    /// those pieces are still there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seams: Vec<super::seam::SeamSpec>,
    /// Middle of the board area they occupied, metres.
    pub center: DVec2,
    pub pieces: crate::authoring::clipboard::PatternClipboard,
}

/// Format tag of a native garment template.
pub const TEMPLATE_FORMAT: &str = "clothcraft-template";
/// The same format as written by earlier builds of this editor; read as [`TEMPLATE_FORMAT`].
const LEGACY_TEMPLATE_FORMAT: &str = "marvelous-cloth-template";

fn template_format<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let tag = String::deserialize(d)?;
    Ok(if tag == LEGACY_TEMPLATE_FORMAT {
        TEMPLATE_FORMAT.into()
    } else {
        tag
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GarmentTemplate {
    #[serde(deserialize_with = "template_format")]
    pub format: String,
    pub version: u32,
    pub name: String,
    pub fabrics: Vec<Fabric>,
    /// Optional visual metadata keyed by stable fabric ID, excluded from physics.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fabric_appearance: BTreeMap<String, super::appearance::FabricAppearance>,
    /// Nonphysical artwork placements; asset IDs are opaque, not file paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<super::graphics::GraphicPlacement>,
    /// Construction annotations only; they do not create physical fasteners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trims: Vec<super::trims::Trim>,
    pub panels: Vec<Panel>,
    pub seams: Vec<SeamSpec>,
    /// Editable two-piece relations, never a solver constraint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirror_links: Vec<MirrorLink>,
    /// Authored world holds, identified by exact material coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pin_targets: Vec<super::pins::MaterialPinTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frozen_panels: Vec<super::pins::FrozenPanel>,
    /// Panels taken out of the simulation: held in place, and no contact with any cloth.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deactivated_panels: Vec<super::pins::DeactivatedPanel>,
    /// Pieces whose front face is the other side (flipped normal): only the look changes, the
    /// back colour shows where the front did.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub flipped_panels: BTreeSet<String>,
    /// Pieces taken off the board and out of the simulation, kept with the document so they
    /// can be brought back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archive: Vec<ArchivedPieces>,
    /// Fold angle and strength of seams, by seam id: the two pieces meet at that angle.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seam_folds: BTreeMap<String, SeamFold>,
    /// Zippers on seams, by seam id: only the share of the seam the slider has closed is
    /// sewn, from the seam's start.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seam_zippers: BTreeMap<String, SeamZipper>,
    /// Where each piece's fabric texture sits, by piece id: the image moved this far across the
    /// piece, in its material coordinates (metres). Looks only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub texture_offsets: BTreeMap<String, glam::DVec2>,
    /// Steam brushed onto the pieces: each stamp shrinks or stretches the cloth under it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steam: Vec<super::steam::SteamStamp>,
    /// Torn stretches of seams, by seam id, as shares of the seam's length: left unsewn, as a
    /// ripped seam is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seam_tears: BTreeMap<String, Vec<[f64; 2]>>,
    /// Seams switched off, by id: kept in the document but not sewn, with their folds and
    /// zippers (Sewing › Activate all seams turns them back on).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub inactive_seams: BTreeSet<String>,
    /// Tacks: two points of the cloth held together (basting, or a button fastened to its
    /// buttonhole).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tacks: Vec<Tack>,
    /// Folds along internal lines: a rest angle and a stiffening of the crease.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_folds: Vec<LineFold>,
    /// Elastic and tape along contour segments and internal lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_springs: Vec<LineSpring>,
    /// Shrinkage by panel id: [warp, weft] in percent of the cut size (100 = none). Below 100
    /// the piece draws in along that grain direction, as a fabric that shrinks; its mass stays.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panel_shrink: BTreeMap<String, [f64; 2]>,
    /// Inflating pressure by panel id, Pa: each triangle is pushed along its normal (the side
    /// its winding faces) with p times its area; negative values pull. Absent = 0.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panel_pressure: BTreeMap<String, f64>,
    /// Panels simulated with their fabric stiffened `STRENGTHEN_FACTOR` times, to hold their
    /// shape while a garment settles; usually switched off afterwards.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub strengthened_panels: BTreeSet<String>,
    /// Panels that keep a captured 3D shape: each vertex is drawn softly to the captured shape
    /// best fitted (rotated and moved) to where the panel is going, so it moves and collides as
    /// a whole but holds its form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solidified_panels: Vec<super::pins::FrozenPanel>,
    /// Simulation layer by panel id (absent = 0): where two pieces of different layers touch,
    /// only the higher (outer) one gives way, as if the lower lay under it unmoved. Used to
    /// settle layered garments, then usually reset to 0.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panel_layers: BTreeMap<String, u32>,
    /// Cylindrical arrangement wraps by panel id; absent in older documents.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panel_wraps: BTreeMap<String, PanelWrap>,
    /// Panels locked on the 2D board: they cannot be picked or box-selected there. A selection
    /// aid only, like the 2D lock of other pattern tools: nothing physical depends on it and
    /// the 3D view, the inspector and scripts can still edit a locked panel.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub locked_panels: BTreeSet<String>,
    /// Internal lines locked on the 2D board, so clicks and boxes reach only the pieces.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub internal_lines_locked: bool,
    /// The authored pins hold on to the avatar: while an animation records, each pinned point
    /// moves with the spot of the skin it was nearest. Still, nothing changes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pins_attached: bool,
    /// Sizes, grade rules and points of measure. The panels are the base size; other sizes
    /// are derived, and the size on show is what compiles. Absent in ungraded documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grading: Option<super::grading::Grading>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorLink {
    pub a: String,
    pub b: String,
    pub axis: super::shapes::MirrorAxis,
}
/// Highest simulation layer a panel can take.
pub const MAX_LAYER: u32 = 9;
/// Elastic or tape along a contour segment or internal line of a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineSpring {
    pub panel: String,
    /// A polygon edge or internal line name, or a rectangle side (top, right, bottom, left).
    pub line: String,
    pub kind: LineKind,
    /// Rest length, percent of the cut length: below 100 an elastic draws the line in.
    pub ratio: f64,
    /// Axial stiffness, N: the pull per unit of stretch beyond the rest length.
    pub strength: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// Gathers the line to `ratio` of its length.
    Elastic,
    /// Keeps the line from stretching (`ratio` 100).
    Tape,
    /// Draws the line in a little, as a tight seam does, so the cloth beside it puckers.
    Puckering,
}

impl LineSpring {
    pub fn elastic(panel: &str, line: &str) -> Self {
        Self {
            panel: panel.into(),
            line: line.into(),
            kind: LineKind::Elastic,
            ratio: 80.,
            strength: 20.,
        }
    }
    pub fn tape(panel: &str, line: &str) -> Self {
        Self {
            panel: panel.into(),
            line: line.into(),
            kind: LineKind::Tape,
            ratio: 100.,
            strength: 2000.,
        }
    }
    /// Puckering: the line drawn in to 92 % with a moderate pull, so the cloth beside it
    /// ripples in small puckers rather than gathering.
    pub fn puckering(panel: &str, line: &str) -> Self {
        Self {
            panel: panel.into(),
            line: line.into(),
            kind: LineKind::Puckering,
            ratio: 92.,
            strength: 40.,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(30.0..=100.0).contains(&self.ratio) || !(0.1..=10_000.0).contains(&self.strength) {
            return Err(
                "Elastic and tape need a ratio of 30 to 100 % and a strength of 0.1 to 10000 N"
                    .into(),
            );
        }
        if self.kind == LineKind::Tape && self.ratio != 100. {
            return Err("Tape keeps the cut length (ratio 100 %)".into());
        }
        Ok(())
    }
}

/// The angle at which a seam's two pieces meet, as a cloth tool's seam fold angle: 180° flat,
/// below 180 folding toward the side the faces' normal points to. Strength multiplies the
/// fabric's bending stiffness across the seam.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeamFold {
    pub angle: f64,
    pub strength: f64,
}

impl Default for SeamFold {
    fn default() -> Self {
        Self {
            angle: 180.,
            strength: 1.,
        }
    }
}

/// A zipper along a seam. The slider has closed its first `closed` share, from 0 (open, the
/// two sides apart) to 1 (closed all along); the tape is `width_m` wide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeamZipper {
    pub closed: f64,
    pub width_m: f64,
}

impl Default for SeamZipper {
    fn default() -> Self {
        Self {
            closed: 1.,
            width_m: 0.012,
        }
    }
}

/// Two material points held together by a stiff zero-length spring.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tack {
    pub id: String,
    pub a: TackPoint,
    pub b: TackPoint,
}

/// A point of a piece, in its material coordinates (metres).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TackPoint {
    pub panel: String,
    pub uv: glam::DVec2,
}

/// Stiffness of a tack, N/m.
pub const TACK_STIFFNESS: f64 = 500.;
/// Prefix of the tacks that fasten a button to its buttonhole.
pub const FASTEN_PREFIX: &str = "fasten:";

impl CompiledGarment {
    /// The vertices and barycentric weights of a material point: the panel's triangle that
    /// holds `uv` (or the nearest one, its weights clamped to its edges).
    pub fn material_point(&self, panel: &str, uv: glam::DVec2) -> Option<Vec<(usize, f64)>> {
        let range = self.panels.iter().find(|r| r.id == panel)?;
        let mut best: Option<(f64, [usize; 3], [f64; 3])> = None;
        for t in &self.mesh.triangles[range.triangles.clone()] {
            let [a, b, c] = t.vertices.map(|v| self.mesh.uv[v]);
            let (e0, e1, e2) = (b - a, c - a, uv - a);
            let det = e0.perp_dot(e1);
            if det.abs() < 1e-18 {
                continue;
            }
            let wb = e2.perp_dot(e1) / det;
            let wc = e0.perp_dot(e2) / det;
            let w = [1. - wb - wc, wb, wc];
            let score = w.iter().copied().fold(f64::INFINITY, f64::min);
            if best.as_ref().is_none_or(|(s, ..)| score > *s) {
                best = Some((score, t.vertices, w));
            }
        }
        let (_, vertices, w) = best?;
        let w = w.map(|x| x.max(0.));
        let sum: f64 = w.iter().sum();
        Some(
            vertices
                .iter()
                .zip(w)
                .filter(|(_, x)| *x > 0.)
                .map(|(v, x)| (*v, x / sum))
                .collect(),
        )
    }
}

/// A fold along an internal line, as a cloth tool's fold angle and strength.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineFold {
    pub panel: String,
    pub line: String,
    /// Fold angle, degrees, 0 to 360: 180 is flat; below 180 the piece folds toward the side
    /// its normal faces, above 180 away from it.
    pub angle: f64,
    /// Bending stiffness of the crease, times the fabric's.
    pub strength: f64,
}

impl LineFold {
    pub fn new(panel: &str, line: &str) -> Self {
        Self {
            panel: panel.into(),
            line: line.into(),
            angle: 90.,
            strength: 10.,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=360.0).contains(&self.angle) || !(1.0..=1000.0).contains(&self.strength) {
            return Err("A fold needs an angle of 0 to 360° and a strength of 1 to 1000".into());
        }
        Ok(())
    }
}

/// One tension-only spring of an elastic or tape: E = ½·k·max(0, |x_a − x_b| − rest)².
#[derive(Clone, Debug)]
pub struct CompiledLine {
    pub vertices: [usize; 2],
    pub rest: f64,
    pub stiffness: f64,
}

/// The triangle holding the boundary edge (a, b), and its third vertex.
fn edge_triangle(mesh: &crate::model::Mesh, a: usize, b: usize) -> Option<(usize, usize)> {
    mesh.triangles.iter().enumerate().find_map(|(i, t)| {
        let v = t.vertices;
        (v.contains(&a) && v.contains(&b))
            .then(|| (i, *v.iter().find(|x| **x != a && **x != b).unwrap()))
    })
}

/// Whether `vertices`, in winding order, run from `a` to `b`.
fn winds(vertices: &[usize; 3], a: usize, b: usize) -> bool {
    (0..3).any(|k| vertices[k] == a && vertices[(k + 1) % 3] == b)
}

/// `chain` carried on at both ends along mesh edges that continue it in a straight line
/// (within about 2.5°), as far as such edges go.
fn extend_straight(mesh: &crate::model::Mesh, chain: &[usize]) -> Vec<usize> {
    if chain.len() < 2 {
        return chain.to_vec();
    }
    let mut neighbours: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for t in &mesh.triangles {
        let [a, b, c] = t.vertices;
        for (p, q) in [(a, b), (b, c), (c, a)] {
            neighbours.entry(p).or_default().insert(q);
            neighbours.entry(q).or_default().insert(p);
        }
    }
    let carry = |mut prev: usize, mut end: usize, seen: &BTreeSet<usize>| {
        let mut more: Vec<usize> = vec![];
        for _ in 0..64 {
            let dir = (mesh.uv[end] - mesh.uv[prev]).normalize();
            let next = neighbours.get(&end).and_then(|ns| {
                ns.iter().copied().find(|&w| {
                    !seen.contains(&w)
                        && !more.contains(&w)
                        && (mesh.uv[w] - mesh.uv[end]).normalize().dot(dir) > 0.999
                })
            });
            let Some(w) = next else { break };
            more.push(w);
            prev = end;
            end = w;
        }
        more
    };
    let seen: BTreeSet<usize> = chain.iter().copied().collect();
    let n = chain.len();
    let tail = carry(chain[n - 2], chain[n - 1], &seen);
    let mut head = carry(chain[1], chain[0], &seen);
    head.reverse();
    head.into_iter()
        .chain(chain.iter().copied())
        .chain(tail)
        .collect()
}

/// Allowed shrinkage, percent of the cut size.
pub const SHRINK_RANGE: std::ops::RangeInclusive<f64> = 50.0..=150.0;

/// Scale a panel's rest shape by `warp` and `weft` along its grain: rest coordinates X become
/// S·X with S = R·diag(warp, weft)·Rᵀ, so a warp fibre's rest length is `warp` times its cut
/// length. Masses are left as cut.
pub(crate) fn shrink(out: &mut CompiledGarment, range: &PanelRange, warp: f64, weft: f64) {
    let (s, c) = out.materials[range.material]
        .grain_degrees
        .to_radians()
        .sin_cos();
    let r = glam::DMat2::from_cols(glam::DVec2::new(c, s), glam::DVec2::new(-s, c));
    let scale = r * glam::DMat2::from_diagonal(glam::DVec2::new(warp, weft)) * r.transpose();
    let inverse = scale.inverse();
    for t in &mut out.mesh.triangles[range.triangles.clone()] {
        t.rest = t.rest.map(|p| scale * p);
        t.inv_dm *= inverse;
        t.area *= warp * weft;
    }
    for a in &mut out.mesh.rest_area[range.vertices.clone()] {
        *a *= warp * weft;
    }
}

/// Largest pressure magnitude on a panel, Pa.
pub const MAX_PRESSURE: f64 = 2000.;
/// Stiffness factor of a strengthened panel.
pub const STRENGTHEN_FACTOR: f64 = 10.;

/// A solidified panel's vertices and the captured shape they are drawn to.
#[derive(Clone, Debug)]
pub struct SolidGroup {
    pub vertices: Vec<usize>,
    pub reference: Vec<DVec3>,
}

#[derive(Clone, Debug)]
pub struct PanelRange {
    pub id: String,
    pub vertices: Range<usize>,
    pub triangles: Range<usize>,
    pub material: usize,
    pub boundaries: BTreeMap<String, Vec<usize>>,
}
#[derive(Clone, Debug)]
pub struct CompiledGarment {
    pub mesh: Mesh,
    pub initial_state: State,
    /// Flat arrangement before authored world targets; not material rest shape.
    pub arrangement_positions: Vec<DVec3>,
    /// Legacy constraints before adding authored world targets.
    pub legacy_pins: Vec<Option<DVec3>>,
    /// Membership remains explicit when an authored target coincides with a legacy pin.
    pub authored_holds: Vec<bool>,
    /// Vertices of deactivated panels: the solver leaves them out of every contact.
    pub inert: Vec<bool>,
    /// Simulation layer of each vertex's panel; empty when every panel is on layer 0.
    pub layers: Vec<u32>,
    /// Solidified panels: their vertices and captured reference positions.
    pub solid: Vec<SolidGroup>,
    /// Pressure on each triangle, Pa; empty when no panel has any.
    pub pressure: Vec<f64>,
    /// Elastic and tape as tension-only springs between consecutive vertices of their lines.
    pub lines: Vec<CompiledLine>,
    /// One orientation-bound material per panel, derived from shared fabric properties.
    pub materials: Vec<Material>,
    pub vertex_material: Vec<usize>,
    pub seams: Vec<CompiledSeam>,
    pub panels: Vec<PanelRange>,
    pub warnings: Vec<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>, kind: &str) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !valid_id(id) || !seen.insert(id) {
            return Err(format!("Invalid or duplicate {kind} id: {id}"));
        }
    }
    Ok(())
}
impl GarmentTemplate {
    pub fn load(path: &Path) -> Result<Self, String> {
        let template: Self =
            serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        template.compile()?;
        Ok(template)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.compile()?;
        fs::write(
            path,
            serde_json::to_string_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
    pub fn compile(&self) -> Result<CompiledGarment, String> {
        // A graded garment compiles in the sizes on show; the derived template carries no
        // grading, so this does not recurse.
        if let Some(grading) = &self.grading {
            grading.validate()?;
            if let Some(sized) = super::grading::apply::view(self)? {
                return sized.compile();
            }
        }
        if let Some(id) = self
            .locked_panels
            .iter()
            .find(|id| !self.panels.iter().any(|p| &p.id == *id))
        {
            return Err(format!("Locked panel {id} is not in the template"));
        }
        for (id, wrap) in &self.panel_wraps {
            if !self.panels.iter().any(|p| &p.id == id) {
                return Err(format!("Panel wrap refers to unknown panel {id}"));
            }
            wrap.validate()?;
        }
        for id in &self.flipped_panels {
            if !self.panels.iter().any(|p| &p.id == id) {
                return Err(format!("Flipped panel {id} is not in the template"));
            }
        }
        for (id, offset) in &self.texture_offsets {
            if !self.panels.iter().any(|p| &p.id == id) || !offset.is_finite() {
                return Err(format!("Texture position on unknown piece {id}"));
            }
        }
        for (id, tears) in &self.seam_tears {
            if !self.seams.iter().any(|s| &s.id == id) {
                return Err(format!("Tear on unknown seam {id}"));
            }
            if tears.len() > 64
                || tears.iter().any(|[lo, hi]| {
                    !(0.0..=1.0).contains(lo) || !(0.0..=1.0).contains(hi) || hi - lo < 1e-6
                })
            {
                return Err(format!(
                    "Seam {id} needs tears within its length, each of some length"
                ));
            }
        }
        if self.steam.len() > super::steam::MAX_STAMPS {
            return Err("Too much steam: remove some before adding more".into());
        }
        for stamp in &self.steam {
            if !self.panels.iter().any(|p| p.id == stamp.panel) {
                return Err(format!("Steam on unknown panel {}", stamp.panel));
            }
            stamp.validate()?;
        }
        for id in &self.inactive_seams {
            if !self.seams.iter().any(|s| &s.id == id) {
                return Err(format!("Inactive seam {id} is not in the template"));
            }
        }
        for (id, fold) in &self.seam_folds {
            if !self.seams.iter().any(|s| &s.id == id) {
                return Err(format!("Fold angle refers to unknown seam {id}"));
            }
            if !(0.0..=360.0).contains(&fold.angle) || !(1.0..=1000.0).contains(&fold.strength) {
                return Err(
                    "A seam fold needs an angle of 0 to 360° and a strength of 1 to 1000".into(),
                );
            }
        }
        for (id, zipper) in &self.seam_zippers {
            if !self.seams.iter().any(|s| &s.id == id) {
                return Err(format!("Zipper refers to unknown seam {id}"));
            }
            if !(0.0..=1.0).contains(&zipper.closed) || !(0.002..=0.05).contains(&zipper.width_m) {
                return Err(
                    "A zipper needs a closed share of 0 to 100 % and a width of 2 to 50 mm".into(),
                );
            }
        }
        let mut tack_ids = BTreeSet::new();
        for tack in &self.tacks {
            if !tack_ids.insert(&tack.id) {
                return Err(format!("Two tacks are called {}", tack.id));
            }
            for p in [&tack.a, &tack.b] {
                if !self.panels.iter().any(|q| q.id == p.panel) || !p.uv.is_finite() {
                    return Err(format!("Tack {} refers to an unknown panel", tack.id));
                }
            }
        }
        let mut folds = BTreeSet::new();
        for fold in &self.line_folds {
            if !self.panels.iter().any(|p| p.id == fold.panel) {
                return Err(format!("Fold refers to unknown panel {}", fold.panel));
            }
            if !folds.insert((&fold.panel, &fold.line)) {
                return Err(format!(
                    "Line {} of {} has two folds",
                    fold.line, fold.panel
                ));
            }
            fold.validate()?;
        }
        let mut lines = BTreeSet::new();
        for spring in &self.line_springs {
            if !self.panels.iter().any(|p| p.id == spring.panel) {
                return Err(format!("Elastic refers to unknown panel {}", spring.panel));
            }
            if !lines.insert((&spring.panel, &spring.line)) {
                return Err(format!(
                    "Line {} of {} has two elastics or tapes",
                    spring.line, spring.panel
                ));
            }
            spring.validate()?;
        }
        for (id, factors) in &self.panel_shrink {
            if !self.panels.iter().any(|p| &p.id == id) {
                return Err(format!("Shrinkage refers to unknown panel {id}"));
            }
            if factors.iter().any(|f| !SHRINK_RANGE.contains(f)) {
                return Err(format!(
                    "Shrinkage goes from {} to {} %",
                    SHRINK_RANGE.start(),
                    SHRINK_RANGE.end()
                ));
            }
        }
        for (id, pressure) in &self.panel_pressure {
            if !self.panels.iter().any(|p| &p.id == id) {
                return Err(format!("Pressure refers to unknown panel {id}"));
            }
            if !pressure.is_finite() || pressure.abs() > MAX_PRESSURE {
                return Err(format!("Pressure must be within ±{MAX_PRESSURE} Pa"));
            }
        }
        for id in &self.strengthened_panels {
            if !self.panels.iter().any(|p| &p.id == id) {
                return Err(format!("Strengthened panel {id} is not in the template"));
            }
        }
        let mut solidified = BTreeSet::new();
        for s in &self.solidified_panels {
            if !self.panels.iter().any(|p| p.id == s.panel) {
                return Err(format!(
                    "Solidified panel {} is not in the template",
                    s.panel
                ));
            }
            if !solidified.insert(s.panel.as_str()) {
                return Err(format!("Panel {} is solidified twice", s.panel));
            }
            if self.frozen_panels.iter().any(|f| f.panel == s.panel)
                || self.deactivated_panels.iter().any(|d| d.panel == s.panel)
            {
                return Err(format!(
                    "Panel {} cannot be solidified while frozen or deactivated",
                    s.panel
                ));
            }
        }
        for (id, layer) in &self.panel_layers {
            if !self.panels.iter().any(|p| &p.id == id) {
                return Err(format!("Layer refers to unknown panel {id}"));
            }
            if *layer > MAX_LAYER {
                return Err(format!("Layers go from 0 to {MAX_LAYER}"));
            }
        }
        let mut deactivated = BTreeSet::new();
        for d in &self.deactivated_panels {
            if !self.panels.iter().any(|p| p.id == d.panel) {
                return Err(format!(
                    "Deactivated panel {} is not in the template",
                    d.panel
                ));
            }
            if !deactivated.insert(d.panel.as_str()) {
                return Err(format!("Panel {} is deactivated twice", d.panel));
            }
            if self.frozen_panels.iter().any(|f| f.panel == d.panel) {
                return Err(format!(
                    "Panel {} cannot be both frozen and deactivated",
                    d.panel
                ));
            }
        }
        if self.format != TEMPLATE_FORMAT || !(1..=2).contains(&self.version) {
            return Err("Unsupported garment template format/version".into());
        }
        if self.version == 1
            && (!self.pin_targets.is_empty()
                || !self.frozen_panels.is_empty()
                || !self.deactivated_panels.is_empty()
                || self
                    .panels
                    .iter()
                    .any(|p| p.polygon.is_some() || !p.pin_points.is_empty())
                || self
                    .seams
                    .iter()
                    .any(|s| s.references().any(|r| r.edge.is_some())))
        {
            return Err("Polygon geometry and material pins require template version 2".into());
        }
        if self.panels.is_empty()
            || self.panels.len() > 128
            || self.fabrics.is_empty()
            || self.seams.len() > 512
        {
            return Err("Template needs 1..128 panels, fabrics and at most 512 seams".into());
        }
        unique_ids(self.panels.iter().map(|p| p.id.as_str()), "panel")?;
        unique_ids(self.fabrics.iter().map(|f| f.id.as_str()), "fabric")?;
        unique_ids(self.seams.iter().map(|s| s.id.as_str()), "seam")?;
        if self.graphics.len() > 4096 {
            return Err("At most 4096 graphic placements are supported".into());
        }
        unique_ids(self.graphics.iter().map(|g| g.id.as_str()), "graphic")?;
        for graphic in &self.graphics {
            graphic.validate_panel(self)?;
        }
        if self.trims.len() > 4096 {
            return Err("At most 4096 trims are supported".into());
        }
        unique_ids(self.trims.iter().map(|t| t.id.as_str()), "trim")?;
        for trim in &self.trims {
            trim.validate()?;
            if !self.panels.iter().any(|p| p.id == trim.panel_id) {
                return Err("Trim references a missing panel".into());
            }
        }
        let mut linked = std::collections::BTreeSet::new();
        for link in &self.mirror_links {
            for id in [&link.a, &link.b] {
                if !linked.insert(id)
                    || !self
                        .panels
                        .iter()
                        .any(|p| &p.id == id && p.polygon.is_some())
                {
                    return Err("Mirror links require distinct polygon panels, with at most one partner per panel".into());
                }
            }
        }
        for (id, appearance) in &self.fabric_appearance {
            if !self.fabrics.iter().any(|f| &f.id == id) {
                return Err(format!("Appearance references missing fabric {id}"));
            }
            appearance.validate()?;
        }
        for f in &self.fabrics {
            f.properties.bind(0.0).validate()?;
        }
        let mut out = CompiledGarment {
            arrangement_positions: vec![],
            legacy_pins: vec![],
            authored_holds: vec![],
            inert: vec![],
            layers: vec![],
            solid: vec![],
            pressure: vec![],
            lines: vec![],
            mesh: Mesh {
                uv: vec![],
                triangles: vec![],
                hinges: vec![],
                rest_area: vec![],
                masses: vec![],
                pins: vec![],
            },
            initial_state: State {
                x: vec![],
                v: vec![],
                time: 0.0,
            },
            materials: vec![],
            vertex_material: vec![],
            seams: vec![],
            panels: vec![],
            warnings: vec![],
        };
        for panel in &self.panels {
            if panel.layout_2d.is_some_and(|p| !p.is_finite())
                || !panel.layout_rotation_degrees.is_finite()
            {
                return Err("Non-finite 2D layout".into());
            }
            let f = self
                .fabrics
                .iter()
                .find(|f| f.id == panel.fabric)
                .ok_or_else(|| {
                    format!(
                        "Panel {} references missing fabric {}",
                        panel.id, panel.fabric
                    )
                })?;
            if !panel.placement.translation.is_finite()
                || !panel.placement.rotation_degrees.is_finite()
            {
                return Err(format!("Non-finite placement for {}", panel.id));
            }
            if panel
                .pins
                .iter()
                .enumerate()
                .any(|(i, p)| panel.pins[..i].contains(p))
            {
                return Err(format!("Duplicate pin in {}", panel.id));
            }
            let material = f.properties.bind(panel.grain_degrees);
            let (mut mesh, mut boundaries) = if let Some(polygon) = &panel.polygon {
                if !panel.pins.is_empty() {
                    return Err("Polygon panels use pin_points, not rectangle corners".into());
                }
                if polygon.points.iter().any(|p| {
                    p.x < 0.0
                        || p.y < 0.0
                        || p.x > panel.pattern.width + 1e-9
                        || p.y > panel.pattern.height + 1e-9
                }) {
                    return Err("Polygon outside declared pattern bounds".into());
                }
                let built = polygon.build(&material)?;
                (built.mesh, built.boundaries)
            } else {
                (
                    panel.pattern.build(&material, Scene::Sphere)?.0,
                    BTreeMap::new(),
                )
            };
            let vertex_start = out.mesh.uv.len();
            let tri_start = out.mesh.triangles.len();
            let material_id = out.materials.len();
            let x: Vec<_> = mesh
                .uv
                .iter()
                .map(|p| {
                    let local = DVec3::new(p.x, -p.y, 0.0);
                    let local = self
                        .panel_wraps
                        .get(&panel.id)
                        .map_or(local, |w| w.bend(local));
                    panel.placement.transform(local)
                })
                .collect();
            if x.iter().any(|p| !p.is_finite()) {
                return Err("Placement overflow".into());
            }
            mesh.pins.fill(None);
            for corner in &panel.pins {
                let i = panel.corner(*corner);
                mesh.pins[i] = Some(x[i]);
            }
            for point in &panel.pin_points {
                let i = mesh
                    .uv
                    .iter()
                    .position(|p| p.distance(*point) < 1e-9)
                    .ok_or("Pin point must coincide with a rest mesh vertex")?;
                mesh.pins[i] = Some(x[i]);
            }
            for ids in boundaries.values_mut() {
                for i in ids {
                    *i += vertex_start;
                }
            }
            for t in &mut mesh.triangles {
                t.vertices = t.vertices.map(|i| i + vertex_start);
                t.material = material_id;
            }
            for h in &mut mesh.hinges {
                h.vertices = h.vertices.map(|i| i + vertex_start);
                h.material = material_id;
            }
            out.vertex_material
                .extend(std::iter::repeat_n(material_id, x.len()));
            out.initial_state
                .v
                .extend(std::iter::repeat_n(DVec3::ZERO, x.len()));
            out.initial_state.x.extend(x);
            out.mesh.uv.extend(mesh.uv);
            out.mesh.triangles.extend(mesh.triangles);
            out.mesh.hinges.extend(mesh.hinges);
            out.mesh.rest_area.extend(mesh.rest_area);
            out.mesh.masses.extend(mesh.masses);
            out.mesh.pins.extend(mesh.pins);
            out.materials.push(material);
            out.panels.push(PanelRange {
                id: panel.id.clone(),
                vertices: vertex_start..out.mesh.uv.len(),
                triangles: tri_start..out.mesh.triangles.len(),
                material: material_id,
                boundaries,
            });
        }
        let curve = |reference: &BoundaryRef, reverse: bool| -> Result<BoundaryCurve, String> {
            let i = self
                .panels
                .iter()
                .position(|p| p.id == reference.panel)
                .ok_or_else(|| format!("Missing seam panel {}", reference.panel))?;
            let vertices = if let Some(edge) = &reference.edge {
                out.panels[i]
                    .boundaries
                    .get(edge)
                    .cloned()
                    .ok_or_else(|| format!("Unknown boundary {edge} on {}", reference.panel))?
            } else {
                if self.panels[i].polygon.is_some() {
                    return Err("Polygon seam requires a named edge".into());
                }
                self.panels[i].boundary_vertices(reference.side, out.panels[i].vertices.start)
            };
            BoundaryCurve::new(
                &out.mesh,
                vertices,
                reference.range,
                reverse ^ reference.reversed,
            )
        };
        let unsewn: BTreeSet<&str> = self
            .deactivated_panels
            .iter()
            .filter(|d| d.sewing)
            .map(|d| d.panel.as_str())
            .collect();
        for (i, spec) in self.seams.iter().enumerate() {
            if spec.a_tail.len() >= 128 || spec.b_tail.len() >= 128 {
                return Err("Sewing side needs 1..128 segments".into());
            }
            // Seams of a panel deactivated with its sewing, and seams switched off, stay in the
            // document only.
            if spec.references().any(|r| unsewn.contains(r.panel.as_str()))
                || self.inactive_seams.contains(&spec.id)
            {
                continue;
            }
            // Reject overlapping seam ownership, including sewing an interval to itself.
            let refs: Vec<_> = spec.references().collect();
            let previous: Vec<_> = self.seams[..i]
                .iter()
                .flat_map(|s| s.references())
                .collect();
            for (j, r) in refs.iter().enumerate() {
                for other in previous.iter().copied().chain(refs[..j].iter().copied()) {
                    if r.panel == other.panel
                        && (if r.edge.is_some() || other.edge.is_some() {
                            r.edge == other.edge
                        } else {
                            r.side == other.side
                        })
                        && r.range[0].max(other.range[0]) < r.range[1].min(other.range[1]) - 1e-12
                    {
                        return Err(format!(
                            "Overlapping seam ownership on {} {:?}",
                            r.panel, r.side
                        ));
                    }
                }
            }
            // A zipper sews only the share its slider has closed; fully open, nothing.
            let closed = self.seam_zippers.get(&spec.id).map_or(1., |z| z.closed);
            if closed <= 1e-9 {
                continue;
            }
            let seam = CompiledSeam::compile_until(
                spec,
                BoundaryCurve::concatenate(
                    spec.side(false)
                        .map(|r| curve(r, false))
                        .collect::<Result<_, _>>()?,
                    false,
                )?,
                BoundaryCurve::concatenate(
                    spec.side(true)
                        .map(|r| curve(r, false))
                        .collect::<Result<_, _>>()?,
                    spec.reverse_b,
                )?,
                closed,
                self.seam_tears.get(&spec.id).map_or(&[][..], |t| t),
            )?;
            if (seam.lengths[0] - seam.lengths[1]).abs()
                > 1e-8 * seam.lengths[0].max(seam.lengths[1])
                || seam.notches.iter().any(|k| (k[0] - k[1]).abs() > 1e-8)
            {
                out.warnings.push(format!(
                    "{}: unequal total or notch-interval lengths ({:.3} / {:.3} m total); no physical gathering model",
                    seam.id, seam.lengths[0], seam.lengths[1]
                ));
            }
            out.seams.push(seam);
        }
        // Seam folds: a hinge across the seam for each edge of its first side, from that edge's
        // triangle to the triangle at the matching place on the other side.
        let mut seam_hinges = vec![];
        for spec in &self.seams {
            let Some(fold) = self.seam_folds.get(&spec.id) else {
                continue;
            };
            if !out.seams.iter().any(|s| s.id == spec.id) {
                continue;
            }
            let a = BoundaryCurve::concatenate(
                spec.side(false)
                    .map(|r| curve(r, false))
                    .collect::<Result<_, _>>()?,
                false,
            )?;
            let b = BoundaryCurve::concatenate(
                spec.side(true)
                    .map(|r| curve(r, false))
                    .collect::<Result<_, _>>()?,
                spec.reverse_b,
            )?;
            let rest = (fold.angle - 180.).to_radians();
            let breaks = a.breakpoints();
            for w in breaks.windows(2) {
                let t = 0.5 * (w[0] + w[1]);
                let [(a0, _), (a1, _)] = a.point(t);
                let [(b0, _), (b1, _)] = b.point(t);
                let (Some((ta, c)), Some((tb, d))) = (
                    edge_triangle(&out.mesh, a0, a1),
                    edge_triangle(&out.mesh, b0, b1),
                ) else {
                    continue;
                };
                // The edge in the winding order of its own triangle, so the first normal is
                // that face's own.
                let (p, q) = if winds(&out.mesh.triangles[ta].vertices, a0, a1) {
                    (a0, a1)
                } else {
                    (a1, a0)
                };
                let e = out.mesh.uv[q] - out.mesh.uv[p];
                let areas = out.mesh.triangles[ta].area + out.mesh.triangles[tb].area;
                seam_hinges.push(crate::model::Hinge {
                    vertices: [p, q, c, d],
                    weight: 3. * e.length_squared() / areas * fold.strength,
                    across: glam::DVec2::new(-e.y, e.x).normalize(),
                    material: out.mesh.triangles[ta].material,
                    rest_angle: rest,
                });
            }
        }
        out.mesh.hinges.extend(seam_hinges);
        for tack in &self.tacks {
            let locate = |p: &TackPoint| {
                out.material_point(&p.panel, p.uv)
                    .ok_or_else(|| format!("Tack {} is on a missing panel", tack.id))
            };
            let mut terms: BTreeMap<usize, f64> = BTreeMap::new();
            for (v, w) in locate(&tack.a)? {
                *terms.entry(v).or_default() += w;
            }
            for (v, w) in locate(&tack.b)? {
                *terms.entry(v).or_default() -= w;
            }
            terms.retain(|_, w| w.abs() > 1e-12);
            if terms.is_empty() {
                continue;
            }
            // Exactly zero sum, as a seam's correspondence requires.
            let excess: f64 = terms.values().sum();
            if let Some(w) = terms.values_mut().next() {
                *w -= excess;
            }
            let point = super::seam::StitchPoint {
                terms: terms.into_iter().collect(),
            };
            // A tack is a one-sample seam: the solvers, the GPU and the contact exclusions
            // treat it as one, with a stiffness in N/m.
            out.seams.push(CompiledSeam {
                id: format!("tack:{}", tack.id),
                samples: vec![super::seam::StitchSample {
                    point: point.clone(),
                    weight_m: 1.,
                }],
                probes: vec![point],
                guides: vec![],
                pairs: vec![],
                stiffness: TACK_STIFFNESS,
                lengths: [0.01, 0.01],
                notches: vec![],
            });
        }
        out.arrangement_positions = out.initial_state.x.clone();
        out.legacy_pins = out.mesh.pins.clone();
        out.authored_holds = vec![false; out.mesh.uv.len()];
        for (i, target) in super::pins::resolve_targets(&out, &self.pin_targets)? {
            out.authored_holds[i] = true;
            out.mesh.pins[i] = Some(target);
            out.initial_state.x[i] = target;
        }
        for (i, target) in super::pins::resolve_frozen(&out, &self.frozen_panels)? {
            out.authored_holds[i] = true;
            out.mesh.pins[i] = Some(target);
            out.initial_state.x[i] = target;
        }
        for fold in &self.line_folds {
            let range = out
                .panels
                .iter()
                .find(|r| r.id == fold.panel)
                .ok_or("Fold on a missing panel")?;
            let line = range.boundaries.get(&fold.line).ok_or_else(|| {
                format!("Panel {} has no internal line {}", fold.panel, fold.line)
            })?;
            // Internal lines stop just short of the outline; a fold that stopped there would
            // have to stretch the cloth at its ends. Carry it on straight to the outline.
            let chain = &extend_straight(&out.mesh, line);
            let crease: BTreeSet<[usize; 2]> = chain
                .windows(2)
                .map(|w| [w[0].min(w[1]), w[0].max(w[1])])
                .collect();
            // Below 180° the halves turn toward the side the faces' winding normal points to.
            let rest = (fold.angle - 180.).to_radians();
            for h in &mut out.mesh.hinges {
                let [a, b, ..] = h.vertices;
                if crease.contains(&[a.min(b), a.max(b)]) {
                    // With consistently wound faces the signed dihedral angle does not depend
                    // on which way the hinge's edge runs, so one rest angle fits the crease.
                    h.rest_angle = rest;
                    h.weight *= fold.strength;
                }
            }
        }
        for spring in &self.line_springs {
            let range = out
                .panels
                .iter()
                .find(|r| r.id == spring.panel)
                .ok_or("Elastic on a missing panel")?;
            let chain = match range.boundaries.get(&spring.line) {
                Some(chain) => chain.clone(),
                None => {
                    let panel = self
                        .panels
                        .iter()
                        .find(|p| p.id == spring.panel)
                        .ok_or("Elastic on a missing panel")?;
                    let side = match spring.line.as_str() {
                        "top" => Side::Top,
                        "right" => Side::Right,
                        "bottom" => Side::Bottom,
                        "left" => Side::Left,
                        other => {
                            return Err(format!(
                                "Panel {} has no segment or internal line {other}",
                                spring.panel
                            ));
                        }
                    };
                    if panel.polygon.is_some() {
                        return Err(format!(
                            "Panel {} has no segment or internal line {}",
                            spring.panel, spring.line
                        ));
                    }
                    panel.boundary_vertices(side, range.vertices.start)
                }
            };
            for pair in chain.windows(2) {
                let cut = out.mesh.uv[pair[0]].distance(out.mesh.uv[pair[1]]);
                if cut <= 1e-12 {
                    continue;
                }
                out.lines.push(CompiledLine {
                    vertices: [pair[0], pair[1]],
                    rest: cut * spring.ratio / 100.,
                    stiffness: spring.strength / cut,
                });
            }
        }
        for range in out.panels.clone() {
            if let Some([warp, weft]) = self.panel_shrink.get(&range.id) {
                shrink(&mut out, &range, warp / 100., weft / 100.);
            }
        }
        if !self.steam.is_empty() {
            super::steam::apply(&mut out, &self.steam);
        }
        if self.panel_pressure.values().any(|p| *p != 0.) {
            out.pressure = vec![0.; out.mesh.triangles.len()];
            for range in &out.panels {
                if let Some(p) = self.panel_pressure.get(&range.id) {
                    out.pressure[range.triangles.clone()].fill(*p);
                }
            }
        }
        for range in &out.panels {
            if self.strengthened_panels.contains(&range.id) {
                let m = &mut out.materials[range.material];
                m.warp *= STRENGTHEN_FACTOR;
                m.weft *= STRENGTHEN_FACTOR;
                m.shear *= STRENGTHEN_FACTOR;
                m.bend_warp *= STRENGTHEN_FACTOR;
                m.bend_weft *= STRENGTHEN_FACTOR;
            }
        }
        for hold in &self.solidified_panels {
            let (vertices, reference) =
                super::pins::resolve_frozen(&out, std::slice::from_ref(hold))?
                    .into_iter()
                    .unzip();
            out.solid.push(SolidGroup {
                vertices,
                reference,
            });
        }
        if self.panel_layers.values().any(|l| *l > 0) {
            out.layers = vec![0; out.mesh.uv.len()];
            for range in &out.panels {
                let layer = self.panel_layers.get(&range.id).copied().unwrap_or(0);
                out.layers[range.vertices.clone()].fill(layer);
            }
        }
        out.inert = vec![false; out.mesh.uv.len()];
        let holds: Vec<_> = self.deactivated_panels.iter().map(|d| d.hold()).collect();
        for (i, target) in super::pins::resolve_frozen(&out, &holds)? {
            out.authored_holds[i] = true;
            out.inert[i] = true;
            out.mesh.pins[i] = Some(target);
            out.initial_state.x[i] = target;
        }
        Ok(out)
    }

    /// Deterministic small fixture: 3x4 and 4x7 cells, independent 3D placements.
    pub fn two_panel_demo() -> Self {
        let properties = FabricProperties::default();
        Self {
            format: TEMPLATE_FORMAT.into(),
            version: 1,
            name: "Two panels / nonmatching seam".into(),
            mirror_links: vec![],
            pin_targets: vec![],
            frozen_panels: vec![],
            deactivated_panels: vec![],
            flipped_panels: Default::default(),
            archive: Vec::new(),
            seam_folds: Default::default(),
            seam_zippers: Default::default(),
            inactive_seams: Default::default(),
            seam_tears: Default::default(),
            texture_offsets: Default::default(),
            steam: vec![],
            tacks: vec![],
            line_folds: vec![],
            line_springs: vec![],
            panel_shrink: Default::default(),
            panel_pressure: Default::default(),
            strengthened_panels: Default::default(),
            solidified_panels: vec![],
            panel_layers: Default::default(),
            panel_wraps: Default::default(),
            locked_panels: Default::default(),
            internal_lines_locked: false,
            pins_attached: false,
            grading: None,
            fabric_appearance: Default::default(),
            graphics: vec![],
            trims: vec![],
            fabrics: vec![Fabric {
                id: "woven-demo".into(),
                name: "Uncalibrated woven reference".into(),
                properties,
            }],
            panels: vec![
                Panel {
                    polygon: None,
                    pin_points: vec![],
                    layout_2d: None,
                    layout_rotation_degrees: 0.,
                    id: "left".into(),
                    name: "Left panel".into(),
                    pattern: Pattern {
                        width: 0.4,
                        height: 0.6,
                        nx: 3,
                        ny: 4,
                    },
                    fabric: "woven-demo".into(),
                    grain_degrees: 0.0,
                    placement: Placement {
                        translation: DVec3::new(-0.4, 1.0, 0.0),
                        ..Default::default()
                    },
                    pins: vec![Corner::TopLeft, Corner::TopRight],
                },
                Panel {
                    polygon: None,
                    pin_points: vec![],
                    layout_2d: None,
                    layout_rotation_degrees: 0.,
                    id: "right".into(),
                    name: "Right panel".into(),
                    pattern: Pattern {
                        width: 0.4,
                        height: 0.6,
                        nx: 4,
                        ny: 7,
                    },
                    fabric: "woven-demo".into(),
                    grain_degrees: 45.0,
                    placement: Placement {
                        translation: DVec3::new(0.08, 1.0, 0.03),
                        rotation_degrees: DVec3::new(8.0, 12.0, 0.0),
                    },
                    pins: vec![],
                },
            ],
            seams: vec![SeamSpec {
                a_tail: vec![],
                b_tail: vec![],
                id: "centre-seam".into(),
                a: BoundaryRef {
                    reversed: false,
                    edge: None,
                    panel: "left".into(),
                    side: Side::Right,
                    range: [0.0, 1.0],
                },
                b: BoundaryRef {
                    reversed: false,
                    edge: None,
                    panel: "right".into(),
                    side: Side::Left,
                    range: [0.0, 1.0],
                },
                reverse_b: true,
                notches: vec![[0.5, 0.5]],
                stiffness: 5000.0,
                allow_ease: false,
            }],
        }
    }
}

/// A piece's fabric as a key sets it: pressure (Pa) and shrinkage along the warp and the
/// weft (percent of the cut size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FabricValues {
    pub pressure: f64,
    pub shrink: [f64; 2],
}

impl CompiledGarment {
    /// This garment, compiled from `template`, with the pieces in `fabric` given those
    /// pressures and shrinkages instead of the template's: the same mesh, its rest shapes and
    /// pressure changed, its masses kept.
    pub fn with_fabric(
        &self,
        template: &GarmentTemplate,
        fabric: &BTreeMap<String, FabricValues>,
    ) -> Result<Self, String> {
        let mut out = self.clone();
        for (id, values) in fabric {
            let range = out
                .panels
                .iter()
                .find(|p| &p.id == id)
                .cloned()
                .ok_or_else(|| format!("Fabric keys name a missing piece: {id}"))?;
            if !values.pressure.is_finite() || values.pressure.abs() > MAX_PRESSURE {
                return Err(format!("Pressure must be within ±{MAX_PRESSURE} Pa"));
            }
            if values.shrink.iter().any(|s| !SHRINK_RANGE.contains(s)) {
                return Err(format!(
                    "Shrinkage must be {}–{} % of the cut size",
                    SHRINK_RANGE.start(),
                    SHRINK_RANGE.end()
                ));
            }
            if values.pressure != 0. && out.pressure.is_empty() {
                out.pressure = vec![0.; out.mesh.triangles.len()];
            }
            if !out.pressure.is_empty() {
                out.pressure[range.triangles.clone()].fill(values.pressure);
            }
            // The compiled shape already holds the template's shrinkage: scale by the change.
            let [warp, weft] = template
                .panel_shrink
                .get(id)
                .copied()
                .unwrap_or([100., 100.]);
            let (a, b) = (values.shrink[0] / warp, values.shrink[1] / weft);
            if a != 1. || b != 1. {
                shrink(&mut out, &range, a, b);
            }
        }
        Ok(out)
    }

    pub fn max_seam_gap(&self, x: &[DVec3]) -> f64 {
        self.seams.iter().map(|s| s.max_gap(x)).fold(0.0, f64::max)
    }
    pub fn fiber_frames(&self, x: &[DVec3]) -> Vec<energy::FiberFrame> {
        self.mesh
            .triangles
            .iter()
            .map(|t| energy::fiber_frame(x, t, &self.materials[t.material]))
            .collect()
    }
}
