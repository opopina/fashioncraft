//! Transactional authoring in material coordinates, independent of UI and solver.
//! Compile before accepting an edit. Reset physical state only when Edit requests it.
use crate::{
    garment::{
        CompiledGarment, GarmentTemplate, curves,
        markings::{self, Notch},
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        seam::{BoundaryRef, SeamSpec},
        template::{Fabric, FabricProperties, Panel, Placement},
    },
    model::{Pattern, State},
};
use glam::{DQuat, DVec2, DVec3, EulerRot};
pub mod arrange2d;
pub mod clipboard;
pub mod constructions;
mod geometry;
mod linked;
use crate::garment::shapes::{BooleanOp, MirrorAxis, Primitive};

/// A contour corner is the start of a named outgoing boundary, never a mesh index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionRef {
    pub panel: String,
    pub edge: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PointConstruction {
    Project,
    /// Signed distance from the reference boundary's start along its infinite line.
    Distance(f64),
    /// Signed offset from the projected point, toward +90 degrees in board (u,v).
    Perpendicular(f64),
    /// Intersection with a second straight boundary, resolved in board space.
    Intersection(ConstructionRef),
}

pub enum Command {
    ConstructPoint {
        target: ConstructionRef,
        reference: ConstructionRef,
        operation: PointConstruction,
    },
    /// Add a fitted contour without changing existing boundary identities.
    Freehand {
        samples: Vec<DVec2>,
        tolerance_m: f64,
        meshing: MeshingSettings,
        fabric: String,
    },
    Paste {
        clipboard: clipboard::PatternClipboard,
        center: DVec2,
    },
    LayoutGroup {
        panels: Vec<String>,
        delta: DVec2,
    },
    Primitive {
        kind: Primitive,
        size: DVec2,
        sweep_degrees: f64,
        origin: DVec2,
        meshing: MeshingSettings,
        fabric: String,
        replace: bool,
    },
    Mirror {
        panel: String,
        axis: MirrorAxis,
        copy_side: Option<i8>,
    },
    LinkedMirror {
        panel: String,
        axis: MirrorAxis,
    },
    UnlinkMirror {
        panel: String,
    },
    Boolean {
        a: String,
        b: String,
        op: BooleanOp,
        keep_tool: bool,
    },
    InternalLine {
        panel: String,
        points: Vec<DVec2>,
        tangents: Vec<DVec2>,
    },
    RemoveLine {
        panel: String,
        line: String,
    },
    Ruffle {
        panel: String,
        line: String,
        width: f64,
        ratio: f64,
    },
    Polygon {
        points: Vec<DVec2>,
        tangents: Vec<DVec2>,
        meshing: MeshingSettings,
        fabric: String,
        replace: bool,
    },
    Place {
        panel: String,
        placement: Placement,
    },
    /// Body arrangement: rigid placement plus an optional cylindrical wrap (EXO-AV-002).
    Arrange {
        panel: String,
        placement: Placement,
        wrap: Option<crate::garment::template::PanelWrap>,
    },
    /// One world-space rigid transform about a shared pivot; material UVs stay fixed.
    PlaceGroup {
        panels: Vec<String>,
        pivot: DVec3,
        translation: DVec3,
        rotation: DQuat,
    },
    Shape {
        panel: String,
        polygon: PolygonPattern,
    },
    /// Exact numeric size of a rectangular panel, in metres.
    Dimensions {
        panel: String,
        width: f64,
        height: f64,
    },
    /// Place one contour point at exact material coordinates, in metres.
    Anchor {
        panel: String,
        anchor: usize,
        position: DVec2,
    },
    /// Translate a selection of contour points by a material delta, in metres.
    MovePoints {
        panel: String,
        points: Vec<usize>,
        delta: DVec2,
    },
    /// Move contour selections on several panels by one board-space delta.
    /// Panel presentation rotations are inverted independently; compilation is atomic.
    MovePointGroups {
        groups: Vec<(String, Vec<usize>)>,
        board_delta: DVec2,
    },
    /// Move contour points by an exact metric step given in millimetres.
    Nudge {
        panel: String,
        points: Vec<usize>,
        delta_mm: DVec2,
    },
    /// Insert one contour point on a named boundary at a material arclength in
    /// metres from its first endpoint. The first half keeps the boundary name;
    /// the second half receives a deterministic derived name.
    SplitBoundary {
        panel: String,
        edge: String,
        at_m: f64,
    },
    /// Create an actual V-shaped cut-and-sewn dart along an unsewn straight
    /// boundary. The triangular material wedge is cut away and the two new
    /// equal-length dart legs become a persistent same-panel solver seam.
    /// This is NOT the uncut folded-tuck model used in some garments.
    /// Restricted to unsewn straight boundaries without targets, holes, internal
    /// lines or linked mirrors. Stations are fractions; the apex is in metres.
    /// Adapted from Mikyle Couto's PR #66 (e3994ae8), with complete seam-reference
    /// checks and without its unrelated document, export or solver changes.
    DartCutSew {
        panel: String,
        edge: String,
        left_t: f64,
        right_t: f64,
        apex: DVec2,
    },
    /// Remove one contour point and rejoin its two neighbouring boundaries.
    RemovePoint {
        panel: String,
        point: usize,
    },
    /// Curve (true) or straighten (false) both boundaries incident to a point.
    CurvePoint {
        panel: String,
        anchor: usize,
        curved: bool,
    },
    /// Required length of a named boundary, in metres. `0.0` clears the stored
    /// target without moving geometry; a positive value stores and enforces it.
    TargetLength {
        panel: String,
        edge: String,
        target_m: f64,
    },
    /// Per-boundary seam allowance in metres; `0.0` clears it. Draft data: it
    /// never changes the mesh, masses or the physical state.
    SeamAllowance {
        panel: String,
        edge: String,
        width_m: f64,
    },
    /// Insert (`index: None`) or replace one notch on a named closed boundary.
    SetNotch {
        panel: String,
        edge: String,
        index: Option<usize>,
        notch: Notch,
    },
    /// Remove one notch by its index in the boundary's list.
    RemoveNotch {
        panel: String,
        edge: String,
        index: usize,
    },
    Layout {
        panel: String,
        origin: DVec2,
    },
    BoardTransform {
        panel: String,
        origin: DVec2,
        rotation_degrees: f64,
    },
    Duplicate {
        panel: String,
    },
    RemovePanel {
        panel: String,
    },
    RemovePanels {
        panels: Vec<String>,
    },
    /// Take pieces off the board and out of the simulation, keeping them (with the seams
    /// among them) in the document's archive. One undo step.
    ArchivePanels {
        panels: Vec<String>,
    },
    /// Bring archived pieces back where they were.
    RestoreArchived {
        id: String,
    },
    /// Drop archived pieces for good.
    DeleteArchived {
        id: String,
    },
    Sew {
        a: BoundaryRef,
        b: BoundaryRef,
        reverse: bool,
        ease: bool,
    },
    /// Replace an existing seam or create one ordered M:N rest-arclength relation.
    SewChain {
        seam: Option<String>,
        a: Vec<BoundaryRef>,
        b: Vec<BoundaryRef>,
        reverse: bool,
        ease: bool,
    },
    RemoveSeam {
        seam: String,
    },
    FabricAppearance {
        fabric: String,
        appearance: Option<crate::garment::appearance::FabricAppearance>,
    },
    /// Add closed internal rectangles or ellipses to a panel: `count` copies whose centres are
    /// `interval_m` apart along `angle_degrees` (0° up, 90° right), starting at `center`
    /// (material coordinates). One undo step.
    InternalShape {
        panel: String,
        kind: crate::garment::shapes2d::InternalShapeKind,
        center: DVec2,
        size: DVec2,
        count: u32,
        interval_m: f64,
        angle_degrees: f64,
    },
    /// Cut a closed internal shape out of its panel as a hole.
    ConvertToHole {
        panel: String,
        line: String,
    },
    /// Fill a hole back in, leaving a closed internal shape along its rim.
    ConvertToInternalShape {
        panel: String,
        hole: usize,
    },
    /// Create a spiral strip pattern with its bounds' corner at `origin` on the board.
    Spiral {
        spiral: crate::garment::shapes2d::Spiral,
        origin: DVec2,
        meshing: MeshingSettings,
        fabric: String,
    },
    /// Rotate and scale contour points of several panels about a board pivot. Bézier handles
    /// attached to the moved points follow the same transform. One undo step.
    TransformPointGroups {
        groups: Vec<(String, Vec<usize>)>,
        pivot: DVec2,
        rotation_degrees: f64,
        scale: DVec2,
    },
    /// Line contour points of several panels up along one board axis. `reference` is the
    /// point the `ToPoint*` modes align to.
    AlignPointGroups {
        groups: Vec<(String, Vec<usize>)>,
        mode: arrange2d::AlignMode,
        reference: Option<(String, usize)>,
    },
    /// Scale a whole drafted panel (contour, holes, internal lines and pins) about its centre.
    ScalePanel {
        panel: String,
        scale: DVec2,
    },
    /// Line panels up on the board with a reference panel's edges or centre.
    AlignPanels {
        panels: Vec<String>,
        reference: String,
        mode: arrange2d::AlignMode,
    },
    /// Space panels on the board along one axis with a fixed gap between their bounds.
    DistributePanels {
        panels: Vec<String>,
        axis: arrange2d::Axis,
        gap_m: f64,
    },
    /// Change a boundary's length by scaling it about the end that stays: `moving` is the end
    /// that moves (`Center` moves both ends by the same amount). The curve keeps its shape and
    /// the handles of the neighbouring boundaries follow the moved points. A stored target
    /// length on the boundary becomes the new length.
    SetLength {
        panel: String,
        edge: String,
        length_m: f64,
        moving: arrange2d::SegmentEnd,
    },
    /// Turn and move the `moved` boundary's piece on the board so that boundary lies along the
    /// `reference` boundary, running the other way round, as two pieces sewn edge to edge.
    /// Each side is (panel, boundary). Board layout only; the cloth is not touched.
    MatchUp {
        reference: (String, String),
        moved: (String, String),
        anchor: arrange2d::SegmentEnd,
    },
    /// Copy a closed internal shape as a new piece of its panel's fabric. `origin` is where
    /// the shape's material origin lands on the board.
    CloneShapeAsPanel {
        panel: String,
        line: String,
        origin: DVec2,
    },
    /// Copy a piece's outline as a closed internal shape inside `target`, centred on `center`
    /// (material coordinates of `target`).
    ClonePanelAsShape {
        source: String,
        target: String,
        center: DVec2,
    },
    /// Internal lines set off from a boundary or open internal line (`path`): `count` copies
    /// `distance_m` apart into the piece (`reverse`: the other side), cut at the outline and,
    /// with `extend`, prolonged up to it. Without `path`, closed internal shapes inset from
    /// the whole outline.
    OffsetAsLines {
        panel: String,
        path: Option<String>,
        count: u32,
        distance_m: f64,
        reverse: bool,
        extend: bool,
    },
    /// A straight internal line leaving `path` at arclength `at_m` from its start, turned
    /// `angle_degrees` towards the material (90° is perpendicular), `length_m` long or up to
    /// the outline when `None`.
    PerpendicularLine {
        panel: String,
        path: String,
        at_m: f64,
        angle_degrees: f64,
        length_m: Option<f64>,
        reverse: bool,
    },
    /// Remove contour points that start a boundary shorter than `overlap_m`, and internal line
    /// anchors closer than that, on the given panels; with `straight_degrees`, also contour
    /// points between two straight boundaries turning less than that. Boundaries used by
    /// seams, targets, allowances or notches keep their points.
    CleanUpPoints {
        panels: Vec<String>,
        overlap_m: f64,
        straight_degrees: Option<f64>,
    },
    /// Put internal lines of one panel in one group, joining any groups they already belong
    /// to. Grouped lines are selected and moved together on the board.
    GroupLines {
        panel: String,
        lines: Vec<String>,
    },
    /// Dissolve the group of internal line `line`.
    UngroupLines {
        panel: String,
        line: String,
    },
    /// Reduce the curve points of drafted outlines and internal lines, keeping `ratio` (0–1) of
    /// the smooth points that carry curves. Corners, and points on boundaries used by seams,
    /// targets, allowances or notches, stay. Merged spans are refitted to the old curve.
    OptimizeCurvePoints {
        panels: Vec<String>,
        ratio: f64,
    },
    /// Replace the 2D board locks: which panels cannot be picked there and whether internal
    /// lines are locked. A selection aid with no physical effect; one undo step.
    SetBoardLocks {
        panels: std::collections::BTreeSet<String>,
        internal_lines: bool,
    },
    /// Flip the front face of pieces (a look only). One undo step.
    FlipNormals {
        panels: Vec<String>,
    },
    /// Set the angle at which a seam's pieces meet, or clear it (`fold: None`). One undo step.
    SetSeamFold {
        seam: String,
        fold: Option<crate::garment::template::SeamFold>,
    },
    /// Hold two points of the cloth together. One undo step.
    AddTack {
        tack: crate::garment::template::Tack,
    },
    /// Remove tacks by id. One undo step.
    RemoveTacks {
        ids: Vec<String>,
    },
    /// Fold an internal line (angle and strength), replacing the fold there, or unfold it
    /// (`fold: None`). One undo step.
    SetLineFold {
        panel: String,
        line: String,
        fold: Option<crate::garment::template::LineFold>,
    },
    /// Put an elastic or tape on a contour segment or internal line, replacing the one there,
    /// or remove it (`spring: None`). One undo step.
    SetLineSpring {
        panel: String,
        line: String,
        spring: Option<crate::garment::template::LineSpring>,
    },
    /// Set the shrinkage of pieces, percent [warp, weft] (100, 100 clears it). One undo step.
    SetShrink {
        panels: Vec<String>,
        warp: f64,
        weft: f64,
    },
    /// Set the inflating pressure of pieces, Pa (0 clears it). One undo step.
    SetPressure {
        panels: Vec<String>,
        pressure: f64,
    },
    /// Strengthen pieces (their fabric simulated `STRENGTHEN_FACTOR` times stiffer) or return
    /// them to normal. One undo step.
    Strengthen {
        panels: Vec<String>,
        on: bool,
    },
    /// Put pieces on a simulation layer (0 clears it): where pieces of different layers touch,
    /// only the outer one gives way. One undo step.
    SetLayer {
        panels: Vec<String>,
        layer: u32,
    },
    /// Create or replace a nonphysical placement, addressed by its stable ID.
    SetGraphic {
        placement: crate::garment::graphics::GraphicPlacement,
    },
    RemoveGraphic {
        id: String,
    },
    SetTrim {
        trim: crate::garment::trims::Trim,
    },
    RemoveTrim {
        id: String,
    },
    /// Repeat a nonphysical trim using a panel-local offset in metres.
    RepeatTrim {
        id: String,
        copies: usize,
        offset_m: DVec2,
    },
    Fabric {
        panel: String,
        name: String,
        properties: FabricProperties,
        all_panels: bool,
    },
}

pub struct Edit {
    pub template: GarmentTemplate,
    pub garment: CompiledGarment,
    pub selected: Option<String>,
    pub reset_simulation: bool,
    /// Contour point index created by this edit, when the command inserted one.
    pub new_point: Option<usize>,
}

impl Edit {
    /// Keep the drape of panels whose compiled rest geometry and arrangement
    /// are unchanged. Match by panel ID and local indices, never global offsets.
    /// New/remeshed/moved panels start from their new arrangement. Solver/contact
    /// caches must still be rebuilt, even when all positions are preserved.
    /// Like [`Edit::state_after`], but a panel whose mesh changed (a 2D edit, a remesh) keeps
    /// its draped shape instead of going back to the flat arrangement: each new vertex takes
    /// the drape of the same spot on the old mesh, with zero velocity. Panels that were still
    /// flat, and panels that only moved, behave as in `state_after`.
    pub fn state_keeping_drape(&self, previous: &CompiledGarment, state: &State) -> State {
        let mut next = self.state_after(previous, state);
        if state.x.len() != previous.mesh.uv.len() || state.v.len() != state.x.len() {
            return next;
        }
        for new in &self.garment.panels {
            let Some(old) = previous.panels.iter().find(|p| p.id == new.id) else {
                continue;
            };
            let (a, b) = (old.vertices.clone(), new.vertices.clone());
            let local =
                |garment: &CompiledGarment, range: &std::ops::Range<usize>, start: usize| {
                    garment.mesh.triangles[range.clone()]
                        .iter()
                        .map(|t| t.vertices.map(|i| i - start))
                        .collect::<Vec<_>>()
                };
            let old_triangles = local(previous, &old.triangles, a.start);
            if previous.mesh.uv[a.clone()] == self.garment.mesh.uv[b.clone()]
                && old_triangles == local(&self.garment, &new.triangles, b.start)
            {
                // Same mesh: copied, or deliberately re-placed (Place, Arrange).
                continue;
            }
            let draped = state.x[a.clone()]
                .iter()
                .zip(&previous.arrangement_positions[a.clone()])
                .any(|(x, rest)| x.distance(*rest) > 1e-9);
            if !draped {
                continue;
            }
            let moved = crate::garment::transfer::transfer_positions(
                &previous.arrangement_positions[a.clone()],
                &old_triangles,
                &state.x[a],
                &self.garment.arrangement_positions[b.clone()],
            );
            next.x[b.clone()].copy_from_slice(&moved);
            for i in b {
                next.v[i] = DVec3::ZERO;
                if let Some(target) = self.garment.mesh.pins[i] {
                    next.x[i] = target;
                }
            }
        }
        next
    }
    pub fn state_after(&self, previous: &CompiledGarment, state: &State) -> State {
        let mut next = self.garment.initial_state.clone();
        next.time = state.time;
        if state.x.len() != previous.mesh.uv.len() || state.v.len() != state.x.len() {
            return next;
        }
        for new in &self.garment.panels {
            let Some(old) = previous.panels.iter().find(|p| p.id == new.id) else {
                continue;
            };
            let a = old.vertices.clone();
            let b = new.vertices.clone();
            if previous.mesh.uv[a.clone()] != self.garment.mesh.uv[b.clone()]
                || previous.arrangement_positions[a.clone()]
                    != self.garment.arrangement_positions[b.clone()]
                || previous.legacy_pins[a.clone()] != self.garment.legacy_pins[b.clone()]
                || !previous.mesh.triangles[old.triangles.clone()]
                    .iter()
                    .map(|t| t.vertices.map(|i| i - a.start))
                    .eq(self.garment.mesh.triangles[new.triangles.clone()]
                        .iter()
                        .map(|t| t.vertices.map(|i| i - b.start)))
            {
                continue;
            }
            next.x[b.clone()].copy_from_slice(&state.x[a.clone()]);
            next.v[b.clone()].copy_from_slice(&state.v[a.clone()]);
            for (old_i, new_i) in a.zip(b) {
                if self.garment.authored_holds[new_i] {
                    let target = self.garment.mesh.pins[new_i].expect("compiled authored target");
                    next.x[new_i] = target;
                    next.v[new_i] = DVec3::ZERO;
                } else if previous.authored_holds[old_i] {
                    // A released hold never resurrects momentum from before pinning.
                    next.v[new_i] = DVec3::ZERO;
                }
            }
        }
        next
    }
}

/// Prepare a pin/release transaction and continuous candidate state. The caller
/// must run its physical initialization gate before recording or installing it.
pub fn prepare_pin_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    panel: &str,
    selection: &[DVec2],
    hold: bool,
) -> Result<(Edit, State), String> {
    prepare_pin_group_edit(
        source,
        compiled,
        state,
        &std::collections::BTreeMap::from([(panel.to_owned(), selection.to_vec())]),
        hold,
    )
}

/// One atomic pin edit across panels. Source freshness and the final candidate
/// are each compiled once, independently of the number of selected panels.
pub fn prepare_pin_group_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    selections: &std::collections::BTreeMap<String, Vec<DVec2>>,
    hold: bool,
) -> Result<(Edit, State), String> {
    use crate::garment::pins::MaterialPinTarget;
    let count = selections
        .values()
        .try_fold(0usize, |n, points| n.checked_add(points.len()));
    if selections.is_empty()
        || selections.len() > 128
        || selections.values().any(Vec::is_empty)
        || !count.is_some_and(|n| (1..=65_536).contains(&n))
    {
        return Err("Select 1..65536 material points on 1..128 panels".into());
    }
    if source
        .frozen_panels
        .iter()
        .any(|p| selections.contains_key(&p.panel))
    {
        return Err("Release the frozen panel before editing individual pins".into());
    }
    if source
        .deactivated_panels
        .iter()
        .any(|p| selections.contains_key(&p.panel))
    {
        return Err("Activate the piece before editing its pins".into());
    }
    validate_pin_source(source, compiled, state)?;
    if compiled
        .mesh
        .pins
        .iter()
        .zip(&state.x)
        .any(|(pin, x)| pin.is_some_and(|target| target.distance(*x) > 1e-10))
    {
        return Err("Current state disagrees with existing pin targets".into());
    }
    let mut captured = Vec::with_capacity(count.unwrap());
    let mut keys = std::collections::BTreeMap::new();
    for (panel, points) in selections {
        let targets = capture_panel_targets(compiled, state, panel, Some(points))?;
        keys.insert(
            panel.as_str(),
            targets
                .iter()
                .map(|(uv, _)| pin_uv_key(*uv))
                .collect::<std::collections::BTreeSet<_>>(),
        );
        captured.extend(targets.into_iter().map(|(uv, world)| MaterialPinTarget {
            panel: panel.clone(),
            uv,
            world,
        }));
    }
    let mut template = source.clone();
    template.pin_targets.retain(|target| {
        keys.get(target.panel.as_str())
            .is_none_or(|selected| !selected.contains(&pin_uv_key(target.uv)))
    });
    if hold {
        template.pin_targets.extend(captured);
        template.version = 2;
    }
    let garment = template.compile()?;
    let edit = Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected: selections.keys().next().cloned(),
        new_point: None,
    };
    let next = edit.state_after(compiled, state);
    Ok((edit, next))
}

/// Freeze/release a complete sampled panel without consuming history or
/// installing it. Existing individual/legacy pins survive panel release.
pub fn prepare_freeze_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    panel: &str,
    freeze: bool,
) -> Result<(Edit, State), String> {
    use crate::garment::pins::{FrozenPanel, MaterialPinTarget, topology_fingerprint};
    let captured = capture_pin_targets(source, compiled, state, panel, None)?;
    if compiled
        .mesh
        .pins
        .iter()
        .zip(&state.x)
        .any(|(pin, x)| pin.is_some_and(|target| target.distance(*x) > 1e-10))
    {
        return Err("Current state disagrees with existing pin targets".into());
    }
    let mut template = source.clone();
    template.frozen_panels.retain(|p| p.panel != panel);
    if freeze {
        template.frozen_panels.push(FrozenPanel {
            panel: panel.to_owned(),
            topology: topology_fingerprint(compiled, panel)?,
            targets: captured
                .into_iter()
                .map(|(uv, world)| MaterialPinTarget {
                    panel: panel.to_owned(),
                    uv,
                    world,
                })
                .collect(),
        });
        template.version = 2;
    }
    let garment = template.compile()?;
    let edit = Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected: Some(panel.to_owned()),
        new_point: None,
    };
    let next = edit.state_after(compiled, state);
    Ok((edit, next))
}

/// Freeze or release a group atomically. Existing freezes are left untouched
/// when freezing, so their captured world targets are never silently replaced.
/// The caller must validate the resulting state before installing the edit.
pub fn prepare_freeze_group_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    panels: &[String],
    freeze: bool,
) -> Result<(Edit, State), String> {
    use crate::garment::pins::{
        FrozenPanel, MAX_PIN_TARGETS, MaterialPinTarget, topology_fingerprint,
    };
    let ids: std::collections::BTreeSet<_> = panels.iter().collect();
    if ids.is_empty() || ids.len() > 128 || ids.len() != panels.len() {
        return Err("Select 1..128 distinct panels for group freeze".into());
    }
    validate_pin_source(source, compiled, state)?;
    if compiled
        .mesh
        .pins
        .iter()
        .zip(&state.x)
        .any(|(pin, x)| pin.is_some_and(|target| target.distance(*x) > 1e-10))
    {
        return Err("Current state disagrees with existing pin targets".into());
    }
    let mut total_targets = source
        .frozen_panels
        .iter()
        .map(|p| p.targets.len())
        .sum::<usize>();
    let mut total_panels = source.frozen_panels.len();
    for id in &ids {
        let panel = compiled
            .panels
            .iter()
            .find(|panel| &panel.id == *id)
            .ok_or("Select existing panels for group freeze")?;
        if freeze
            && !source
                .frozen_panels
                .iter()
                .any(|frozen| &frozen.panel == *id)
        {
            total_targets = total_targets
                .checked_add(panel.vertices.len())
                .ok_or("Too many frozen targets")?;
            total_panels += 1;
            if total_targets > MAX_PIN_TARGETS || total_panels > 128 {
                return Err("Too many frozen targets or panels".into());
            }
        }
    }
    let mut template = source.clone();
    if freeze {
        for panel in ids {
            if template
                .frozen_panels
                .iter()
                .any(|frozen| &frozen.panel == panel)
            {
                continue;
            }
            let captured = capture_panel_targets(compiled, state, panel, None)?;
            template.frozen_panels.push(FrozenPanel {
                panel: panel.clone(),
                topology: topology_fingerprint(compiled, panel)?,
                targets: captured
                    .into_iter()
                    .map(|(uv, world)| MaterialPinTarget {
                        panel: panel.clone(),
                        uv,
                        world,
                    })
                    .collect(),
            });
            template.version = 2;
        }
    } else {
        template
            .frozen_panels
            .retain(|frozen| !ids.contains(&frozen.panel));
    }
    let garment = template.compile()?;
    let edit = Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected: panels.first().cloned(),
        new_point: None,
    };
    let next = edit.state_after(compiled, state);
    Ok((edit, next))
}

/// Deactivate panels (they stay where they are now and no cloth collides with them; with
/// `sewing` their seams are left out as well) or activate them again, where they are. One
/// edit for the whole group; frozen panels must be released first.
pub fn prepare_deactivate_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    panels: &[String],
    deactivate: bool,
    sewing: bool,
) -> Result<(Edit, State), String> {
    use crate::garment::pins::{DeactivatedPanel, MaterialPinTarget, topology_fingerprint};
    let ids: std::collections::BTreeSet<_> = panels.iter().collect();
    if ids.is_empty() || ids.len() > 128 || ids.len() != panels.len() {
        return Err("Select 1..128 distinct pieces".into());
    }
    validate_pin_source(source, compiled, state)?;
    let mut template = source.clone();
    if deactivate {
        for panel in &ids {
            if source.frozen_panels.iter().any(|f| &f.panel == *panel) {
                return Err("Unfreeze the piece before deactivating it".into());
            }
            template.deactivated_panels.retain(|d| &d.panel != *panel);
            let captured = capture_panel_targets(compiled, state, panel, None)?;
            template.deactivated_panels.push(DeactivatedPanel {
                panel: (*panel).clone(),
                topology: topology_fingerprint(compiled, panel)?,
                targets: captured
                    .into_iter()
                    .map(|(uv, world)| MaterialPinTarget {
                        panel: (*panel).clone(),
                        uv,
                        world,
                    })
                    .collect(),
                sewing,
            });
        }
        template.version = 2;
    } else {
        let before = template.deactivated_panels.len();
        template
            .deactivated_panels
            .retain(|d| !ids.contains(&d.panel));
        if template.deactivated_panels.len() == before {
            return Err("Those pieces are active already".into());
        }
    }
    let garment = template.compile()?;
    let edit = Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected: panels.first().cloned(),
        new_point: None,
    };
    let next = edit.state_after(compiled, state);
    Ok((edit, next))
}

/// Solidify pieces in their current 3D shape (they keep it while moving and colliding as a
/// whole) or release them, as one undo step. Returns the edit and the state to install.
pub fn prepare_solidify_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    panels: &[String],
    solidify: bool,
) -> Result<(Edit, State), String> {
    use crate::garment::pins::{FrozenPanel, MaterialPinTarget, topology_fingerprint};
    let ids: std::collections::BTreeSet<_> = panels.iter().collect();
    if ids.is_empty() || ids.len() > 128 || ids.len() != panels.len() {
        return Err("Select 1..128 distinct pieces".into());
    }
    validate_pin_source(source, compiled, state)?;
    let mut template = source.clone();
    if solidify {
        for panel in &ids {
            if source.frozen_panels.iter().any(|f| &f.panel == *panel)
                || source.deactivated_panels.iter().any(|d| &d.panel == *panel)
            {
                return Err("Unfreeze or activate the piece before solidifying it".into());
            }
            template.solidified_panels.retain(|s| &s.panel != *panel);
            let captured = capture_panel_targets(compiled, state, panel, None)?;
            template.solidified_panels.push(FrozenPanel {
                panel: (*panel).clone(),
                topology: topology_fingerprint(compiled, panel)?,
                targets: captured
                    .into_iter()
                    .map(|(uv, world)| MaterialPinTarget {
                        panel: (*panel).clone(),
                        uv,
                        world,
                    })
                    .collect(),
            });
        }
        template.version = 2;
    } else {
        let before = template.solidified_panels.len();
        template
            .solidified_panels
            .retain(|s| !ids.contains(&s.panel));
        if template.solidified_panels.len() == before {
            return Err("Those pieces are not solidified".into());
        }
    }
    let garment = template.compile()?;
    let edit = Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected: panels.first().cloned(),
        new_point: None,
    };
    let next = edit.state_after(compiled, state);
    Ok((edit, next))
}

/// Fasten every button to the nearest buttonhole on another piece (by where they are now in
/// 3D), as tacks, or unfasten them all (`fasten: false`). One undo step.
pub fn prepare_fasten_edit(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    fasten: bool,
) -> Result<Edit, String> {
    use crate::garment::{
        template::{FASTEN_PREFIX, Tack, TackPoint},
        trims::TrimKind,
    };
    let mut template = source.clone();
    template.tacks.retain(|t| !t.id.starts_with(FASTEN_PREFIX));
    if fasten {
        let world = |panel: &str, uv: glam::DVec2| {
            compiled.material_point(panel, uv).map(|terms| {
                terms
                    .iter()
                    .map(|(v, w)| state.x[*v] * *w)
                    .sum::<glam::DVec3>()
            })
        };
        let holes: Vec<_> = source
            .trims
            .iter()
            .filter_map(|t| match &t.kind {
                TrimKind::Buttonhole { start_m, end_m, .. } => {
                    let uv = (*start_m + *end_m) * 0.5;
                    Some((t, uv, world(&t.panel_id, uv)?))
                }
                _ => None,
            })
            .collect();
        let mut used = std::collections::BTreeSet::new();
        for button in &source.trims {
            let TrimKind::Button { center_m, .. } = &button.kind else {
                continue;
            };
            let Some(at) = world(&button.panel_id, *center_m) else {
                continue;
            };
            let nearest = holes
                .iter()
                .filter(|(hole, ..)| hole.panel_id != button.panel_id && !used.contains(&hole.id))
                .min_by(|a, b| a.2.distance(at).total_cmp(&b.2.distance(at)));
            if let Some((hole, uv, _)) = nearest {
                used.insert(hole.id.clone());
                template.tacks.push(Tack {
                    id: format!("{FASTEN_PREFIX}{}:{}", button.id, hole.id),
                    a: TackPoint {
                        panel: button.panel_id.clone(),
                        uv: *center_m,
                    },
                    b: TackPoint {
                        panel: hole.panel_id.clone(),
                        uv: *uv,
                    },
                });
            }
        }
        if !template
            .tacks
            .iter()
            .any(|t| t.id.starts_with(FASTEN_PREFIX))
        {
            return Err("No button has a buttonhole on another piece to fasten to".into());
        }
    } else if template.tacks.len() == source.tacks.len() {
        return Err("No buttons are fastened".into());
    }
    let garment = template.compile()?;
    Ok(Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected: None,
        new_point: None,
    })
}

/// Capture world targets without editing a document or applying constraints.
/// `None` captures the complete panel; a selection contains exact material UVs
/// obtained from its compiled mesh (not arbitrary coordinates or mesh indices).
/// Call only on an explicit action: validation recompiles the source once.
pub fn capture_pin_targets(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
    panel: &str,
    selected_uvs: Option<&[DVec2]>,
) -> Result<Vec<(DVec2, DVec3)>, String> {
    validate_pin_source(source, compiled, state)?;
    capture_panel_targets(compiled, state, panel, selected_uvs)
}

fn validate_pin_source(
    source: &GarmentTemplate,
    compiled: &CompiledGarment,
    state: &State,
) -> Result<(), String> {
    let n = compiled.mesh.uv.len();
    if state.x.len() != n
        || state.v.len() != n
        || !state.time.is_finite()
        || state.x.iter().chain(&state.v).any(|v| !v.is_finite())
    {
        return Err("Pin capture requires a finite state matching the compiled garment".into());
    }
    let fresh = source.compile()?;
    if fresh.mesh.uv != compiled.mesh.uv
        || fresh.initial_state.x != compiled.initial_state.x
        || fresh.mesh.pins != compiled.mesh.pins
        || fresh.arrangement_positions != compiled.arrangement_positions
        || fresh.legacy_pins != compiled.legacy_pins
        || fresh.authored_holds != compiled.authored_holds
        || !fresh.mesh.triangles.iter().map(|t| t.vertices).eq(compiled
            .mesh
            .triangles
            .iter()
            .map(|t| t.vertices))
        || !fresh
            .panels
            .iter()
            .map(|p| (&p.id, &p.vertices))
            .eq(compiled.panels.iter().map(|p| (&p.id, &p.vertices)))
    {
        return Err("Pin capture source is stale; rebuild the garment before capturing".into());
    }
    Ok(())
}

fn pin_uv_key(uv: DVec2) -> [u64; 2] {
    [uv.x, uv.y].map(|v| if v == 0. { 0 } else { v.to_bits() })
}

/// The caller has already checked source freshness and state dimensions.
fn capture_panel_targets(
    compiled: &CompiledGarment,
    state: &State,
    panel: &str,
    selected_uvs: Option<&[DVec2]>,
) -> Result<Vec<(DVec2, DVec3)>, String> {
    const MAX_TARGETS: usize = 65_536;
    let range = compiled
        .panels
        .iter()
        .find(|p| p.id == panel)
        .ok_or("Select an existing panel for pin capture")?
        .vertices
        .clone();
    let count = selected_uvs.map_or(range.len(), <[DVec2]>::len);
    if count == 0 || count > MAX_TARGETS {
        return Err("Capture between 1 and 65536 material targets at a time".into());
    }
    // Canonicalize signed zero. Other values are exact compiled coordinates;
    // no nearest-vertex fallback may silently move a requested attachment.
    let key = pin_uv_key;
    let mut vertices = std::collections::BTreeMap::new();
    for i in range.clone() {
        let uv = compiled.mesh.uv[i];
        if !uv.is_finite() || vertices.insert(key(uv), i).is_some() {
            return Err("Panel contains ambiguous material vertices".into());
        }
    }
    let mut used = std::collections::BTreeSet::new();
    let ids = if let Some(points) = selected_uvs {
        points
            .iter()
            .map(|uv| {
                if !uv.is_finite() || !used.insert(key(*uv)) {
                    return Err(
                        "Pin selection contains non-finite or duplicate coordinates".to_string()
                    );
                }
                vertices.get(&key(*uv)).copied().ok_or_else(|| {
                    "Pin selection must identify exact compiled material vertices".into()
                })
            })
            .collect::<Result<Vec<_>, String>>()?
    } else {
        range.collect()
    };
    Ok(ids
        .into_iter()
        .map(|i| (compiled.mesh.uv[i], state.x[i]))
        .collect())
}

pub fn layout(template: &GarmentTemplate, index: usize) -> DVec2 {
    template.panels[index].layout_2d.unwrap_or_else(|| {
        DVec2::new(
            template.panels[..index]
                .iter()
                .map(|p| p.pattern.width + 0.15)
                .sum(),
            0.,
        )
    })
}
/// Whether `panel` has a contour segment or internal line called `line` (a rectangle side
/// for rectangle pieces).
fn line_exists(t: &GarmentTemplate, panel: &str, line: &str) -> bool {
    t.panels
        .iter()
        .find(|p| p.id == panel)
        .is_some_and(|p| match &p.polygon {
            Some(poly) => {
                poly.edges.iter().any(|e| e == line) || poly.lines.iter().any(|l| l.id == line)
            }
            None => matches!(line, "top" | "right" | "bottom" | "left"),
        })
}

/// Drop the simulation settings of pieces, lines and seams an edit removed (a deleted piece
/// takes its layer, pressure, elastics, folds and tacks with it; a deleted seam its fold), so
/// the edit is not refused for pointing at them.
pub fn prune_simulation_settings(template: &mut GarmentTemplate) {
    let panels: std::collections::BTreeSet<String> =
        template.panels.iter().map(|p| p.id.clone()).collect();
    template
        .deactivated_panels
        .retain(|d| panels.contains(&d.panel));
    template
        .solidified_panels
        .retain(|s| panels.contains(&s.panel));
    template
        .strengthened_panels
        .retain(|id| panels.contains(id));
    template.panel_layers.retain(|id, _| panels.contains(id));
    template.panel_pressure.retain(|id, _| panels.contains(id));
    template.panel_shrink.retain(|id, _| panels.contains(id));
    template.flipped_panels.retain(|id| panels.contains(id));
    template
        .tacks
        .retain(|t| panels.contains(&t.a.panel) && panels.contains(&t.b.panel));
    let springs: Vec<_> = template
        .line_springs
        .iter()
        .filter(|s| line_exists(template, &s.panel, &s.line))
        .cloned()
        .collect();
    template.line_springs = springs;
    let folds: Vec<_> = template
        .line_folds
        .iter()
        .filter(|f| line_exists(template, &f.panel, &f.line))
        .cloned()
        .collect();
    template.line_folds = folds;
    let seams: std::collections::BTreeSet<String> =
        template.seams.iter().map(|s| s.id.clone()).collect();
    template.seam_folds.retain(|id, _| seams.contains(id));
}

pub fn same_physics(a: &GarmentTemplate, b: &GarmentTemplate) -> bool {
    let without_layout = |t: &GarmentTemplate| {
        let mut t = t.clone();
        t.mirror_links.clear();
        t.fabric_appearance.clear();
        t.graphics.clear();
        t.trims.clear();
        // Archived pieces are out of the simulation.
        t.archive.clear();
        // Board locks only filter 2D picking; flipping a piece's front only changes its look.
        t.locked_panels.clear();
        t.flipped_panels.clear();
        t.internal_lines_locked = false;
        for p in &mut t.panels {
            p.layout_2d = None;
            p.layout_rotation_degrees = 0.;
            // A required boundary length is a 2D specification, not rest
            // geometry: storing or clearing it must not invalidate the drape.
            if let Some(polygon) = p.polygon.as_mut() {
                polygon.target_lengths.clear();
                // Markings (seam allowance, notches) are draft data too: adding,
                // moving or deleting one never rebuilds the physical state.
                polygon.seam_allowances.clear();
                polygon.notches.clear();
            }
        }
        serde_json::to_vec(&t).unwrap()
    };
    without_layout(a) == without_layout(b)
}
/// A strict nonphysical draft change, distinct from other board edits: fabric
/// optics, graphics, trims and per-boundary seam allowances/notches. Installing
/// one must not stop playback, bump the worker generation or discard the drape.
pub fn only_appearance_changed(a: &GarmentTemplate, b: &GarmentTemplate) -> bool {
    let markings = |t: &GarmentTemplate| {
        t.panels
            .iter()
            .map(|p| {
                p.polygon
                    .as_ref()
                    .map(|polygon| (polygon.seam_allowances.clone(), polygon.notches.clone()))
            })
            .collect::<Vec<_>>()
    };
    if a.fabric_appearance == b.fabric_appearance
        && a.graphics == b.graphics
        && a.trims == b.trims
        && markings(a) == markings(b)
        && a.locked_panels == b.locked_panels
        && a.internal_lines_locked == b.internal_lines_locked
        && a.flipped_panels == b.flipped_panels
    {
        return false;
    }
    let strip = |source: &GarmentTemplate| {
        let mut t = source.clone();
        t.fabric_appearance.clear();
        t.graphics.clear();
        t.trims.clear();
        t.locked_panels.clear();
        t.internal_lines_locked = false;
        t.flipped_panels.clear();
        for p in &mut t.panels {
            if let Some(polygon) = p.polygon.as_mut() {
                polygon.seam_allowances.clear();
                polygon.notches.clear();
            }
        }
        serde_json::to_vec(&t).unwrap()
    };
    strip(a) == strip(b)
}

/// Ignore only reviewed nonphysical polygon metadata. Unknown fields remain in
/// the comparison so future geometry extensions fail closed by default.
fn strip_graphic_frame_metadata(polygon: &mut serde_json::Value) {
    if let Some(fields) = polygon.as_object_mut() {
        for key in [
            "subdivisions",
            "meshing",
            "target_lengths",
            "seam_allowances",
            "notches",
            // The independently reviewed #1 proposal uses annotations for
            // text-only draft notes; they do not alter the material contour.
            "annotations",
        ] {
            fields.remove(key);
        }
    }
}

fn graphic_material_frame(panel: &Panel) -> Result<Vec<u8>, String> {
    let mut polygon = serde_json::to_value(&panel.polygon).map_err(|e| e.to_string())?;
    strip_graphic_frame_metadata(&mut polygon);
    serde_json::to_vec(&(panel.pattern.width, panel.pattern.height, polygon))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod graphic_frame_tests {
    use super::*;
    #[test]
    fn only_reviewed_nonphysical_fields_are_ignored() {
        let mut polygon = serde_json::json!({"points": [[0, 0]], "meshing": {}, "subdivisions": 1,
            "target_lengths": {}, "seam_allowances": {"edge": 0.01}, "notches": {"edge": []},
            "annotations": [{"text": "front", "position": [0.1, 0.2]}],
            "free_curve_points": {"edge": [[0.3, 0.4]]}, "future_geometry": [1, 2]});
        strip_graphic_frame_metadata(&mut polygon);
        assert_eq!(
            polygon,
            serde_json::json!({"points": [[0, 0]], "free_curve_points": {"edge": [[0.3, 0.4]]}, "future_geometry": [1, 2]})
        );
    }
}
pub fn rotate_2d(p: DVec2, degrees: f64) -> DVec2 {
    let (s, c) = degrees.to_radians().sin_cos();
    DVec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
}
fn normalize(panel: &mut Panel) {
    let poly = panel.polygon.as_mut().unwrap();
    let lo = poly
        .points
        .iter()
        .chain(poly.curves.values().flatten())
        .copied()
        .fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let hi = poly
        .points
        .iter()
        .chain(poly.curves.values().flatten())
        .copied()
        .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
    poly.translate_material(-lo);
    for p in &mut panel.pin_points {
        *p -= lo;
    }
    panel.layout_2d =
        Some(panel.layout_2d.unwrap_or_default() + rotate_2d(lo, panel.layout_rotation_degrees));
    panel.placement.translation = panel.placement.transform(DVec3::new(lo.x, -lo.y, 0.));
    panel.pattern.width = (hi - lo).x;
    panel.pattern.height = (hi - lo).y;
}

/// Exact rectangle resize. The four corners are assigned the requested
/// coordinates directly instead of being scaled, so 400 x 600 mm stays exactly
/// 0.4 x 0.6 m in memory and through a JSON round trip. Only an axis-aligned
/// four-corner contour can carry an exact numeric size; a curved or general
/// polygon keeps its shape and is resized by editing its points.
fn resize_rectangle(panel: &mut Panel, width: f64, height: f64) -> Result<(), String> {
    const TOL: f64 = 1e-9;
    let polygon = panel
        .polygon
        .as_mut()
        .ok_or("This panel has no editable contour")?;
    if polygon.points.len() != 4 || !polygon.curves.is_empty() {
        return Err("Exact dimensions require a four-corner rectangle".into());
    }
    let lo = polygon
        .points
        .iter()
        .copied()
        .fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let hi = polygon
        .points
        .iter()
        .copied()
        .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
    if hi.x - lo.x <= TOL || hi.y - lo.y <= TOL {
        return Err("Exact dimensions require a non-degenerate rectangle".into());
    }
    let on_axis =
        |value: f64, low: f64, high: f64| (value - low).abs() < TOL || (value - high).abs() < TOL;
    if !polygon
        .points
        .iter()
        .all(|p| on_axis(p.x, lo.x, hi.x) && on_axis(p.y, lo.y, hi.y))
    {
        return Err("Exact dimensions require an axis-aligned four-corner rectangle".into());
    }
    let mid = (lo + hi) * 0.5;
    let snapped = |p: DVec2| {
        DVec2::new(
            if p.x <= mid.x { 0. } else { width },
            if p.y <= mid.y { 0. } else { height },
        )
    };
    for p in &mut polygon.points {
        *p = snapped(*p);
    }
    let scale = DVec2::new(width, height) / (hi - lo);
    for hole in &mut polygon.holes {
        for point in &mut hole.points {
            *point = (*point - lo) * scale;
        }
    }
    for line in &mut polygon.lines {
        for point in &mut line.points {
            *point = (*point - lo) * scale;
        }
        for tangent in &mut line.tangents {
            *tangent *= scale;
        }
    }
    for point in &mut panel.pin_points {
        *point = (*point - lo) * scale;
    }
    Ok(())
}

fn unused_id(prefix: &str, ids: impl Iterator<Item = String>) -> String {
    let ids: std::collections::BTreeSet<_> = ids.collect();
    (1..)
        .map(|i| format!("{prefix}_{i}"))
        .find(|s| !ids.contains(s))
        .unwrap()
}

/// Millimetres to metres. Division by 1000 is correctly rounded, so 400 mm is
/// the exact f64 0.4; `mm * 0.001` is not.
pub fn mm_to_m(mm: f64) -> f64 {
    mm / 1000.0
}
/// Metres to millimetres. For readouts only; never feed this back into the model.
pub fn m_to_mm(metres: f64) -> f64 {
    metres * 1000.0
}
/// Sampled rest arclength of one named boundary in metres, using the same
/// sampling as the mesher and the seam compiler.
pub fn edge_length(polygon: &PolygonPattern, edge: &str) -> Result<f64, String> {
    let index = polygon
        .edges
        .iter()
        .position(|name| name == edge)
        .ok_or("Boundary not found")?;
    Ok(polygon
        .edge_points(index)?
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum())
}
/// Signed deviation of a measured length from a target, in millimetres.
pub fn deviation_mm(measured_m: f64, target_m: f64) -> f64 {
    m_to_mm(measured_m - target_m)
}

/// Keep material pin points attached to the contour anchors they coincide with.
fn follow_pins(panel: &mut Panel, moved: &[(usize, DVec2)]) {
    let Some(polygon) = panel.polygon.as_ref() else {
        return;
    };
    let deltas: Vec<(DVec2, DVec2)> = moved
        .iter()
        .map(|(index, before)| (*before, polygon.points[*index] - *before))
        .collect();
    for pin in &mut panel.pin_points {
        for (before, delta) in &deltas {
            if pin.distance(*before) < 1e-9 {
                *pin += *delta;
                break;
            }
        }
    }
}

/// Assign new local coordinates to contour anchors. The selection is deduplicated
/// and validated; absolute coordinates are assigned directly, so no rounding is
/// introduced for a typed position.
/// Write every panel's derived board origin into `layout_2d`, so changing one panel's extent
/// never shifts the implicit layout of the panels after it.
fn freeze_layouts(template: &mut GarmentTemplate) {
    let origins: Vec<_> = (0..template.panels.len())
        .map(|i| layout(template, i))
        .collect();
    for (p, origin) in template.panels.iter_mut().zip(origins) {
        p.layout_2d = Some(origin);
    }
}

fn panel_index(template: &GarmentTemplate, id: &str) -> Result<usize, String> {
    template
        .panels
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| "Panel not found".to_string())
}

fn remap_anchors(
    panel: &mut Panel,
    indices: &[usize],
    map: impl Fn(DVec2) -> DVec2,
) -> Result<(), String> {
    let count = panel
        .polygon
        .as_ref()
        .ok_or("This panel has no editable contour")?
        .points
        .len();
    let mut unique = indices.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.is_empty() {
        return Err("Select at least one point".into());
    }
    if unique.iter().any(|index| *index >= count) {
        return Err("Point index out of range".into());
    }
    let before: Vec<DVec2> = {
        let polygon = panel.polygon.as_ref().unwrap();
        unique.iter().map(|index| polygon.points[*index]).collect()
    };
    let after: Vec<DVec2> = before.iter().map(|point| map(*point)).collect();
    {
        let polygon = panel.polygon.as_mut().unwrap();
        for ((index, position), old) in unique.iter().zip(&after).zip(&before) {
            let delta = *position - *old;
            polygon.points[*index] = *position;
            if let Some(controls) = polygon.curves.get_mut(&polygon.edges[*index]) {
                controls[0] += delta;
            }
            let previous = (*index + count - 1) % count;
            if let Some(controls) = polygon.curves.get_mut(&polygon.edges[previous]) {
                controls[1] += delta;
            }
        }
    }
    let moved: Vec<(usize, DVec2)> = unique.into_iter().zip(before).collect();
    follow_pins(panel, &moved);
    Ok(())
}

/// Deterministic successor name for the second half of a split boundary. The
/// first half keeps the existing name, so no unrelated identifier changes.
fn split_edge_name(edges: &[String], base: &str) -> String {
    let prefix: String = base.chars().take(60).collect();
    (2..)
        .map(|suffix| format!("{prefix}_{suffix}"))
        .find(|candidate| !edges.iter().any(|name| name == candidate))
        .expect("a free split name exists")
}

/// de Casteljau split: the two cubics that together reproduce the input exactly.
fn split_cubic(cubic: [DVec2; 4], t: f64) -> ([DVec2; 4], [DVec2; 4]) {
    let p01 = cubic[0].lerp(cubic[1], t);
    let p12 = cubic[1].lerp(cubic[2], t);
    let p23 = cubic[2].lerp(cubic[3], t);
    let p012 = p01.lerp(p12, t);
    let p123 = p12.lerp(p23, t);
    let middle = p012.lerp(p123, t);
    ([cubic[0], p01, p012, middle], [middle, p123, p23, cubic[3]])
}

/// Cubic parameter whose sampled arclength from the curve start is `at_m`. Any
/// parameter splits a cubic exactly; bisection only chooses where the point lands.
fn parameter_at_arclength(cubic: [DVec2; 4], at_m: f64) -> Result<f64, String> {
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..48 {
        let middle = (low + high) * 0.5;
        let (left, _) = split_cubic(cubic, middle);
        let length: f64 = curves::flatten(left, curves::FLATNESS_M)?
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .sum();
        if length < at_m {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok((low + high) * 0.5)
}

/// Whether a seam references one named boundary of a panel.
/// Boundaries of panel `index` whose points must stay: used by seams, or carrying a target
/// length, a seam allowance or notches.
fn protected_boundaries(template: &GarmentTemplate, index: usize) -> Vec<String> {
    let panel = &template.panels[index];
    panel
        .polygon
        .as_ref()
        .map(|p| {
            p.edges
                .iter()
                .filter(|e| {
                    seam_uses(template, &panel.id, e)
                        || p.target_lengths.contains_key(*e)
                        || p.seam_allowances.contains_key(*e)
                        || p.notches.contains_key(*e)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// The smallest share of a panel's curve points that keeps its outline within `tolerance_m`
/// of the current one: the starting value for [`Command::OptimizeCurvePoints`].
pub fn suggested_curve_ratio(
    template: &GarmentTemplate,
    panel: &str,
    tolerance_m: f64,
) -> Result<f64, String> {
    let index = panel_index(template, panel)?;
    let kept = protected_boundaries(template, index);
    let polygon = template.panels[index]
        .polygon
        .as_ref()
        .ok_or("This panel has no editable contour")?;
    crate::garment::curvefit::suggest_ratio(polygon, |e| kept.iter().any(|k| k == e), tolerance_m)
}

fn seam_uses(template: &GarmentTemplate, panel: &str, edge: &str) -> bool {
    template.seams.iter().any(|seam| {
        seam.references()
            .any(|r| r.panel == panel && r.edge.as_deref() == Some(edge))
    })
}

/// An error leaves the input byte-for-byte unchanged; no partial edit is visible.
/// Fit a stroke and close it explicitly with a straight edge when necessary.
/// Coordinates remain in board/material metres. Validation uses the same
/// sampled contour and mesher as committed panels, not the draped mesh.
pub fn freehand_contour(
    samples: &[DVec2],
    tolerance_m: f64,
    meshing: MeshingSettings,
) -> Result<PolygonPattern, String> {
    use crate::garment::freehand::{FitOptions, fit_freehand};
    let fitted = fit_freehand(
        samples,
        FitOptions {
            max_error_m: tolerance_m,
            max_segments: 63,
        },
    )
    .map_err(|error| format!("Cannot fit stroke: {error:?}"))?;
    let mut spans: Vec<_> = fitted.segments.iter().map(|s| s.controls).collect();
    let first = spans[0][0];
    let last = spans.last().unwrap()[3];
    if first != last {
        let delta = first - last;
        spans.push([last, last + delta / 3., last + delta * (2. / 3.), first]);
    }
    // A closed curve can have fewer than three spans; exact subdivision
    // supplies the polygon's required anchors without changing the curve.
    while spans.len() < 3 {
        let [a, b, c, d] = spans.remove(0);
        let (ab, bc, cd) = (a.lerp(b, 0.5), b.lerp(c, 0.5), c.lerp(d, 0.5));
        let (abc, bcd) = (ab.lerp(bc, 0.5), bc.lerp(cd, 0.5));
        let mid = abc.lerp(bcd, 0.5);
        spans.splice(0..0, [[a, ab, abc, mid], [mid, bcd, cd, d]]);
    }
    let mut area = 0.;
    for span in &spans {
        for pair in curves::flatten(*span, curves::FLATNESS_M)?.windows(2) {
            area += (pair[0] - first).perp_dot(pair[1] - first);
        }
    }
    if area < 0. {
        spans.reverse();
        for span in &mut spans {
            span.reverse();
        }
    }
    let polygon = PolygonPattern {
        points: spans.iter().map(|s| s[0]).collect(),
        edges: (1..=spans.len()).map(|i| format!("edge_{i}")).collect(),
        curves: spans
            .iter()
            .enumerate()
            .map(|(i, s)| (format!("edge_{}", i + 1), [s[1], s[2]]))
            .collect(),
        subdivisions: 0,
        meshing: Some(meshing),
        holes: vec![],
        lines: vec![],
        target_lengths: Default::default(),
        seam_allowances: Default::default(),
        notches: Default::default(),
    };
    polygon.build(&crate::model::Material::default())?;
    Ok(polygon)
}

fn construction_corner<'a>(
    template: &'a GarmentTemplate,
    reference: &ConstructionRef,
) -> Result<(usize, usize, &'a PolygonPattern), String> {
    let panel = template
        .panels
        .iter()
        .position(|p| p.id == reference.panel)
        .ok_or("Construction panel no longer exists")?;
    let polygon = template.panels[panel]
        .polygon
        .as_ref()
        .ok_or("Construction requires an editable contour")?;
    let point = polygon
        .edges
        .iter()
        .position(|e| e == &reference.edge)
        .ok_or("Construction boundary no longer exists")?;
    polygon
        .points
        .get(point)
        .ok_or("Invalid construction corner")?;
    Ok((panel, point, polygon))
}

/// Preview in the target's local material frame. Board layout is used only to
/// relate two flat pieces; no 3D position or simulation state enters this operation.
pub fn constructed_point(
    template: &GarmentTemplate,
    target: &ConstructionRef,
    reference: &ConstructionRef,
    operation: PointConstruction,
) -> Result<DVec2, String> {
    use constructions::Line;
    let (ti, tk, target_poly) = construction_corner(template, target)?;
    let (ri, rk, reference_poly) = construction_corner(template, reference)?;
    if reference_poly.curves.contains_key(&reference.edge) {
        return Err(
            "Choose a straight reference edge; curved boundaries are not treated as lines".into(),
        );
    }
    let on_board = |i: usize, p| {
        layout(template, i) + rotate_2d(p, template.panels[i].layout_rotation_degrees)
    };
    let a = on_board(ri, reference_poly.points[rk]);
    let b = on_board(
        ri,
        reference_poly.points[(rk + 1) % reference_poly.points.len()],
    );
    let point = on_board(ti, target_poly.points[tk]);
    let line = Line::through(a, b).map_err(|e| e.to_string())?;
    let result = match operation {
        PointConstruction::Intersection(second) => {
            let (si, sk, poly) = construction_corner(template, &second)?;
            if poly.curves.contains_key(&second.edge) {
                return Err("Choose a straight second reference edge".into());
            }
            let c = on_board(si, poly.points[sk]);
            let d = on_board(si, poly.points[(sk + 1) % poly.points.len()]);
            Line::through(c, d).and_then(|other| line.intersection(other))
        }
        PointConstruction::Project => line.project(point),
        PointConstruction::Distance(distance) => line.point_at_distance_m(distance),
        PointConstruction::Perpendicular(offset) => {
            let direction = Line::through(DVec2::ZERO, b - a)
                .map_err(|e| e.to_string())?
                .point_at_distance_m(1.)
                .map_err(|e| e.to_string())?;
            let foot = line.project(point).map_err(|e| e.to_string())?;
            Line::through(DVec2::ZERO, DVec2::new(-direction.y, direction.x))
                .and_then(|normal| normal.point_at_distance_m(offset))
                .map(|delta| foot + delta)
        }
    }
    .map_err(|e| e.to_string())?;
    let local = rotate_2d(
        result - layout(template, ti),
        -template.panels[ti].layout_rotation_degrees,
    );
    if !local.is_finite() {
        return Err("Construction result must be finite".into());
    }
    Ok(local)
}

pub fn apply(source: &GarmentTemplate, command: Command) -> Result<Edit, String> {
    let held = |id: &str| {
        source.pin_targets.iter().any(|p| p.panel == id)
            || source.frozen_panels.iter().any(|p| p.panel == id)
    };
    let unsupported_hold_copy = match &command {
        Command::Duplicate { panel }
        | Command::LinkedMirror { panel, .. }
        | Command::Mirror { panel, .. }
        | Command::Place { panel, .. }
        | Command::Arrange { panel, .. }
        | Command::ScalePanel { panel, .. } => held(panel),
        Command::PlaceGroup { panels, .. } => panels.iter().any(|p| held(p)),
        Command::Primitive { replace: true, .. } | Command::Polygon { replace: true, .. } => {
            !source.pin_targets.is_empty() || !source.frozen_panels.is_empty()
        }
        Command::Paste { clipboard, .. } => {
            // Clipboard target remapping is not implemented. Reject explicitly
            // instead of allowing the existing panel-only paste to drop holds.
            let value = serde_json::to_value(clipboard).map_err(|e| e.to_string())?;
            value["template"]["pin_targets"]
                .as_array()
                .is_some_and(|p| !p.is_empty())
                || value["template"]["frozen_panels"]
                    .as_array()
                    .is_some_and(|p| !p.is_empty())
        }
        _ => false,
    };
    if unsupported_hold_copy {
        return Err(
            "Release authored pins before copying, mirroring, replacing or arranging held panels"
                .into(),
        );
    }
    linked::check_command(source, &command)?;
    let deletes_panels = matches!(
        &command,
        Command::RemovePanel { .. } | Command::RemovePanels { .. }
    );
    let unsupported_graphic_mirror = match &command {
        Command::Mirror { panel, .. } | Command::LinkedMirror { panel, .. } => {
            source.graphics.iter().any(|g| &g.panel_id == panel)
        }
        _ => false,
    };
    if unsupported_graphic_mirror {
        return Err("Mirroring panels with graphics is not supported yet".into());
    }
    if matches!(&command, Command::LinkedMirror { panel, .. } if source.trims.iter().any(|t| &t.panel_id == panel))
    {
        return Err("Linked mirroring panels with trims is not supported yet".into());
    }
    // Only this command explicitly reflects the existing trim coordinates;
    // all other contour changes still go through the material-frame guard.
    let mirrored_trim_panel = match &command {
        Command::Mirror {
            panel,
            copy_side: None,
            ..
        } => Some(panel.clone()),
        _ => None,
    };
    if !source.trims.is_empty()
        && matches!(
            &command,
            Command::Primitive { replace: true, .. } | Command::Polygon { replace: true, .. }
        )
    {
        return Err("Remove trims before replacing the garment".into());
    }
    if !source.graphics.is_empty()
        && matches!(
            &command,
            Command::Primitive { replace: true, .. } | Command::Polygon { replace: true, .. }
        )
    {
        return Err("Remove graphics before replacing the garment".into());
    }
    let mut template = source.clone();
    let mut selected = None;
    let mut new_point = None;
    match command {
        Command::ConstructPoint {
            target,
            reference,
            operation,
        } => {
            let position = constructed_point(source, &target, &reference, operation)?;
            let (index, anchor, _) = construction_corner(source, &target)?;
            // Freeze fallback layouts before changing any width, so unrelated
            // pieces do not shift and the reference remains where it was chosen.
            for (i, panel) in template.panels.iter_mut().enumerate() {
                panel.layout_2d = Some(layout(source, i));
            }
            remap_anchors(&mut template.panels[index], &[anchor], |_| position)?;
            normalize(&mut template.panels[index]);
            selected = Some(target.panel);
        }
        Command::Freehand {
            samples,
            tolerance_m,
            meshing,
            fabric,
        } => {
            let mut polygon = freehand_contour(&samples, tolerance_m, meshing)?;
            let lo = polygon
                .points
                .iter()
                .chain(polygon.curves.values().flatten())
                .copied()
                .fold(DVec2::splat(f64::INFINITY), DVec2::min);
            polygon.translate_material(-lo);
            let id = unused_id("panel", template.panels.iter().map(|p| p.id.clone()));
            let translation = if let Some(first) = template.panels.first() {
                let delta = lo - layout(&template, 0);
                first.placement.translation + DVec3::new(delta.x, -delta.y, 0.12)
            } else {
                DVec3::new(0., 1.5, 0.45)
            };
            let mut panel = Panel {
                id: id.clone(),
                name: id.clone(),
                pattern: Pattern {
                    width: 1.,
                    height: 1.,
                    nx: 1,
                    ny: 1,
                },
                polygon: Some(polygon),
                fabric,
                grain_degrees: 90.,
                placement: Placement {
                    translation,
                    ..Default::default()
                },
                pins: vec![],
                pin_points: vec![],
                layout_2d: Some(lo),
                layout_rotation_degrees: 0.,
            };
            normalize(&mut panel);
            template.panels.push(panel);
            template.version = 2;
            selected = Some(id);
        }
        Command::LinkedMirror { panel, axis } => {
            let copy = geometry::mirror(&mut template, &panel, axis, Some(1))?;
            template
                .mirror_links
                .push(crate::garment::template::MirrorLink {
                    a: panel,
                    b: copy.clone(),
                    axis,
                });
            selected = Some(copy);
        }
        Command::UnlinkMirror { panel } => {
            template
                .mirror_links
                .retain(|l| l.a != panel && l.b != panel);
            selected = Some(panel);
        }
        Command::Paste { clipboard, center } => {
            selected = clipboard.paste(&mut template, center)?.first().cloned();
        }
        Command::LayoutGroup { panels, delta } => {
            if !delta.is_finite() || panels.is_empty() {
                return Err("Select panels and a finite layout offset".into());
            }
            let ids: std::collections::BTreeSet<_> = panels.iter().collect();
            if ids
                .iter()
                .any(|id| !template.panels.iter().any(|p| &p.id == *id))
            {
                return Err("Panel not found".into());
            }
            // Resolve implicit layouts before mutating any panel.
            let origins: Vec<_> = (0..template.panels.len())
                .map(|i| layout(&template, i))
                .collect();
            for (p, origin) in template.panels.iter_mut().zip(origins) {
                if ids.contains(&p.id) {
                    p.layout_2d = Some(origin + delta);
                }
            }
        }
        Command::Primitive {
            kind,
            size,
            sweep_degrees,
            origin,
            meshing,
            fabric,
            replace,
        } => {
            let polygon = crate::garment::shapes::primitive(kind, size, sweep_degrees, meshing)?;
            selected = Some(geometry::add_polygon(
                &mut template,
                polygon,
                origin,
                fabric,
                replace,
            )?);
        }
        Command::Mirror {
            panel,
            axis,
            copy_side,
        } => selected = Some(geometry::mirror(&mut template, &panel, axis, copy_side)?),
        Command::Boolean {
            a,
            b,
            op,
            keep_tool,
        } => selected = Some(geometry::boolean(&mut template, &a, &b, op, keep_tool)?),
        Command::InternalLine {
            panel,
            points,
            tangents,
        } => {
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let poly = p
                .polygon
                .as_mut()
                .ok_or("Internal lines require a v2 polygonal panel")?;
            let id = unused_id("line", poly.paths()?.into_iter().map(|(name, _)| name));
            poly.lines.push(crate::garment::domain::InternalLine {
                id,
                points,
                tangents,
                closed: false,
                group: None,
            });
            selected = Some(panel);
        }
        Command::RemoveLine { panel, line } => {
            if template.seams.iter().any(|s| {
                s.references()
                    .any(|r| r.panel == panel && r.edge.as_ref() == Some(&line))
            }) {
                return Err("Remove the seam before deleting its internal line".into());
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let poly = p
                .polygon
                .as_mut()
                .ok_or("A v2 polygonal panel is required")?;
            if !poly.lines.iter().any(|l| l.id == line) {
                return Err("Internal line not found".into());
            }
            poly.lines.retain(|l| l.id != line);
            selected = Some(panel);
        }
        Command::Ruffle {
            panel,
            line,
            width,
            ratio,
        } => {
            selected = Some(geometry::ruffle(
                &mut template,
                &panel,
                &line,
                width,
                ratio,
            )?)
        }
        Command::InternalShape {
            panel,
            kind,
            center,
            size,
            count,
            interval_m,
            angle_degrees,
        } => {
            if !(1..=50).contains(&count) {
                return Err("Create between 1 and 50 shapes at a time".into());
            }
            if !interval_m.is_finite() || !angle_degrees.is_finite() {
                return Err("The interval and angle must be finite".into());
            }
            let index = panel_index(&template, &panel)?;
            let poly = template.panels[index]
                .polygon
                .as_mut()
                .ok_or("Internal shapes require a drafted panel")?;
            for at in crate::garment::shapes2d::replicate(center, count, interval_m, angle_degrees)
            {
                let id = unused_id("shape", poly.paths()?.into_iter().map(|(name, _)| name));
                poly.lines.push(crate::garment::shapes2d::internal_shape(
                    id, kind, at, size,
                )?);
            }
            selected = Some(panel);
        }
        Command::ConvertToHole { panel, line } => {
            if template.seams.iter().any(|s| {
                s.references()
                    .any(|r| r.panel == panel && r.edge.as_ref() == Some(&line))
            }) {
                return Err("Remove the seam on this shape before cutting it out".into());
            }
            let index = panel_index(&template, &panel)?;
            let poly = template.panels[index]
                .polygon
                .as_mut()
                .ok_or("Holes require a drafted panel")?;
            let at = poly
                .lines
                .iter()
                .position(|l| l.id == line)
                .ok_or("Internal shape not found")?;
            let hole = crate::garment::shapes2d::shape_to_hole(&poly.lines[at])?;
            poly.lines.remove(at);
            poly.holes.push(hole);
            selected = Some(panel);
        }
        Command::ConvertToInternalShape { panel, hole } => {
            let index = panel_index(&template, &panel)?;
            let edges = template.panels[index]
                .polygon
                .as_ref()
                .and_then(|poly| poly.holes.get(hole))
                .ok_or("Hole not found")?
                .edges
                .clone();
            if template.seams.iter().any(|s| {
                s.references()
                    .any(|r| r.panel == panel && r.edge.as_ref().is_some_and(|e| edges.contains(e)))
            }) {
                return Err("Remove the seams on this hole before filling it".into());
            }
            let poly = template.panels[index].polygon.as_mut().unwrap();
            let rim = poly.holes.remove(hole);
            let id = unused_id("shape", poly.paths()?.into_iter().map(|(name, _)| name));
            poly.lines
                .push(crate::garment::shapes2d::hole_to_shape(id, &rim)?);
            selected = Some(panel);
        }
        Command::Spiral {
            spiral,
            origin,
            meshing,
            fabric,
        } => {
            let polygon = spiral.polygon(meshing)?;
            selected = Some(geometry::add_polygon(
                &mut template,
                polygon,
                origin,
                fabric,
                false,
            )?);
        }
        Command::TransformPointGroups {
            groups,
            pivot,
            rotation_degrees,
            scale,
        } => {
            if !pivot.is_finite() || !rotation_degrees.is_finite() || groups.is_empty() {
                return Err("Select points and a finite rotation".into());
            }
            arrange2d::validate_scale(scale)?;
            freeze_layouts(&mut template);
            let mut seen = std::collections::BTreeSet::new();
            for (id, points) in groups {
                if !seen.insert(id.clone()) {
                    return Err("A panel may occur only once in a point transform".into());
                }
                let index = panel_index(&template, &id)?;
                let (origin, degrees) = (
                    layout(&template, index),
                    template.panels[index].layout_rotation_degrees,
                );
                let map = |uv: DVec2| {
                    let board = origin + rotate_2d(uv, degrees);
                    rotate_2d(
                        arrange2d::affine(board, pivot, rotation_degrees, scale) - origin,
                        -degrees,
                    )
                };
                let panel = &mut template.panels[index];
                arrange2d::transform_anchors(panel, &points, map)?;
                normalize(panel);
            }
        }
        Command::AlignPointGroups {
            groups,
            mode,
            reference,
        } => {
            if groups.is_empty() {
                return Err("Select points to align".into());
            }
            freeze_layouts(&mut template);
            let reference = match reference {
                Some((id, k)) => {
                    let index = panel_index(&template, &id)?;
                    let uv = *template.panels[index]
                        .polygon
                        .as_ref()
                        .and_then(|poly| poly.points.get(k))
                        .ok_or("Reference point not found")?;
                    Some(arrange2d::board_point(&template, index, uv))
                }
                None => None,
            };
            let mut flat = Vec::new();
            for (id, points) in &groups {
                let index = panel_index(&template, id)?;
                let poly = template.panels[index]
                    .polygon
                    .as_ref()
                    .ok_or("This panel has no editable contour")?;
                for &k in points {
                    let uv = *poly.points.get(k).ok_or("Point index out of range")?;
                    flat.push((index, k, arrange2d::board_point(&template, index, uv)));
                }
            }
            if flat.len() < 2 && reference.is_none() {
                return Err("Select at least two points to align".into());
            }
            let positions: Vec<DVec2> = flat.iter().map(|(_, _, b)| *b).collect();
            let targets = arrange2d::align_points(&positions, mode, reference)?;
            // Material deltas first: every board position is read before any panel moves.
            let moves: Vec<(usize, usize, DVec2)> = flat
                .iter()
                .zip(&targets)
                .map(|((index, k, board), target)| {
                    (
                        *index,
                        *k,
                        arrange2d::material_point(&template, *index, *target)
                            - arrange2d::material_point(&template, *index, *board),
                    )
                })
                .collect();
            let mut touched = std::collections::BTreeSet::new();
            for (index, k, delta) in moves {
                remap_anchors(&mut template.panels[index], &[k], |uv| uv + delta)?;
                touched.insert(index);
            }
            for index in touched {
                normalize(&mut template.panels[index]);
            }
        }
        Command::ScalePanel { panel, scale } => {
            arrange2d::validate_scale(scale)?;
            freeze_layouts(&mut template);
            let index = panel_index(&template, &panel)?;
            let p = &mut template.panels[index];
            let pivot = DVec2::new(p.pattern.width, p.pattern.height) * 0.5;
            arrange2d::scale_panel_material(p, pivot, scale)?;
            // `normalize` moves the board origin by the new bounds' corner, so the piece stays
            // centred where it was.
            normalize(p);
            selected = Some(panel);
        }
        Command::AlignPanels {
            panels,
            reference,
            mode,
        } => {
            let reference_index = panel_index(&template, &reference)?;
            let indices: Vec<usize> = panels
                .iter()
                .map(|id| panel_index(&template, id))
                .collect::<Result<_, _>>()?;
            if indices.is_empty() {
                return Err("Select the pieces to align".into());
            }
            freeze_layouts(&mut template);
            let boxes: Vec<_> = indices
                .iter()
                .map(|i| arrange2d::board_bounds(&template, *i))
                .collect::<Result<_, _>>()?;
            let reference_box = arrange2d::board_bounds(&template, reference_index)?;
            let offsets = arrange2d::align_box_offsets(&boxes, reference_box, mode)?;
            for (index, offset) in indices.into_iter().zip(offsets) {
                let origin = layout(&template, index);
                template.panels[index].layout_2d = Some(origin + offset);
            }
            selected = Some(reference);
        }
        Command::DistributePanels {
            panels,
            axis,
            gap_m,
        } => {
            if !gap_m.is_finite() || !(-10.0..=10.0).contains(&gap_m) {
                return Err("The spacing must be finite and within ±10 m".into());
            }
            let indices: Vec<usize> = panels
                .iter()
                .map(|id| panel_index(&template, id))
                .collect::<Result<_, _>>()?;
            if indices.len() < 2 {
                return Err("Select at least two pieces to distribute".into());
            }
            freeze_layouts(&mut template);
            let boxes: Vec<_> = indices
                .iter()
                .map(|i| arrange2d::board_bounds(&template, *i))
                .collect::<Result<_, _>>()?;
            for (index, offset) in indices
                .into_iter()
                .zip(arrange2d::distribute_offsets(&boxes, axis, gap_m))
            {
                let origin = layout(&template, index);
                template.panels[index].layout_2d = Some(origin + offset);
            }
        }
        Command::SetLength {
            panel,
            edge,
            length_m,
            moving,
        } => {
            if !length_m.is_finite() || !(0.001..=50.).contains(&length_m) {
                return Err("The length must be between 1 mm and 50 m".into());
            }
            let index = panel_index(&template, &panel)?;
            let p = &mut template.panels[index];
            let polygon = p
                .polygon
                .as_ref()
                .ok_or("This panel has no editable contour")?;
            let k = polygon
                .edges
                .iter()
                .position(|name| name == &edge)
                .ok_or("Boundary not found")?;
            let next = (k + 1) % polygon.points.len();
            for _ in 0..4 {
                let polygon = p.polygon.as_ref().unwrap();
                let current = edge_length(polygon, &edge)?;
                if current <= 0. {
                    return Err("Boundary has zero length".into());
                }
                if (current - length_m).abs() <= length_m * 1e-12 {
                    break;
                }
                let factor = length_m / current;
                let pivot = arrange2d::scale_pivot(polygon.points[k], polygon.points[next], moving);
                let own = polygon.curves.get(&edge).copied();
                let scale = move |q: DVec2| pivot + (q - pivot) * factor;
                remap_anchors(p, &[k, next], scale)?;
                // The boundary's own handles scale with it, so its curve keeps its shape.
                if let Some([first, second]) = own {
                    p.polygon
                        .as_mut()
                        .unwrap()
                        .curves
                        .insert(edge.clone(), [scale(first), scale(second)]);
                }
            }
            let polygon = p.polygon.as_mut().unwrap();
            if polygon.target_lengths.contains_key(&edge) {
                polygon.target_lengths.insert(edge.clone(), length_m);
            }
            normalize(p);
            selected = Some(panel);
        }
        Command::MatchUp {
            reference,
            moved,
            anchor,
        } => {
            let r = panel_index(&template, &reference.0)?;
            let m = panel_index(&template, &moved.0)?;
            if r == m {
                return Err("Pick a boundary on another piece".into());
            }
            freeze_layouts(&mut template);
            let ends = |t: &GarmentTemplate, i: usize, edge: &str| -> Result<[DVec2; 2], String> {
                let polygon = t.panels[i]
                    .polygon
                    .as_ref()
                    .ok_or("Matching up needs drafted pieces")?;
                let k = polygon
                    .edges
                    .iter()
                    .position(|name| name == edge)
                    .ok_or("Boundary not found")?;
                let next = (k + 1) % polygon.points.len();
                Ok([
                    arrange2d::board_point(t, i, polygon.points[k]),
                    arrange2d::board_point(t, i, polygon.points[next]),
                ])
            };
            let a = ends(&template, r, &reference.1)?;
            let b = ends(&template, m, &moved.1)?;
            let origin = layout(&template, m);
            let (degrees, shift) = arrange2d::match_up(a, b, origin, anchor)?;
            let panel = &mut template.panels[m];
            panel.layout_rotation_degrees =
                (panel.layout_rotation_degrees + degrees + 180.).rem_euclid(360.) - 180.;
            panel.layout_2d = Some(origin + shift);
            selected = Some(moved.0);
        }
        Command::CloneShapeAsPanel {
            panel,
            line,
            origin,
        } => {
            let index = panel_index(&template, &panel)?;
            let source = &template.panels[index];
            let polygon = source
                .polygon
                .as_ref()
                .ok_or("Internal shapes require a drafted panel")?;
            let shape = polygon
                .lines
                .iter()
                .find(|l| l.id == line)
                .ok_or("Internal shape not found")?;
            let outline =
                crate::garment::shapes2d::shape_outline_polygon(shape, polygon.meshing_settings())?;
            let (fabric, rotation) = (source.fabric.clone(), source.layout_rotation_degrees);
            let id = geometry::add_polygon(&mut template, outline, origin, fabric, false)?;
            // Keep the source piece's board rotation, turning about `origin`.
            let clone = template.panels.last_mut().unwrap();
            let lo = clone.layout_2d.unwrap_or(origin) - origin;
            clone.layout_rotation_degrees = rotation;
            clone.layout_2d = Some(origin + rotate_2d(lo, rotation));
            selected = Some(id);
        }
        Command::ClonePanelAsShape {
            source,
            target,
            center,
        } => {
            if !center.is_finite() {
                return Err("Invalid position".into());
            }
            let s = panel_index(&template, &source)?;
            let t = panel_index(&template, &target)?;
            let outline = template.panels[s]
                .polygon
                .as_ref()
                .ok_or("Only drafted pieces can become internal shapes")?;
            let polygon = template.panels[t]
                .polygon
                .as_ref()
                .ok_or("Internal shapes require a drafted panel")?;
            let id = unused_id("shape", polygon.paths()?.into_iter().map(|(name, _)| name));
            let mut line = crate::garment::shapes2d::outline_to_shape(id, outline)?;
            let samples = line.samples()?;
            let (lo, hi) = arrange2d::bounds(&samples).ok_or("Empty outline")?;
            let shift = center - (lo + hi) * 0.5;
            for point in &mut line.points {
                *point += shift;
            }
            template.panels[t]
                .polygon
                .as_mut()
                .unwrap()
                .lines
                .push(line);
            selected = Some(target);
        }
        Command::OffsetAsLines {
            panel,
            path,
            count,
            distance_m,
            reverse,
            extend,
        } => {
            use crate::garment::lines2d;
            let index = panel_index(&template, &panel)?;
            let polygon = template.panels[index]
                .polygon
                .as_mut()
                .ok_or("Internal lines require a drafted panel")?;
            let mut lines = vec![];
            let mut names: Vec<String> = polygon.paths()?.into_iter().map(|(n, _)| n).collect();
            let mut fresh = |prefix: &str| {
                let id = unused_id(prefix, names.clone().into_iter());
                names.push(id.clone());
                id
            };
            match &path {
                Some(name) => {
                    for points in
                        lines2d::offset_lines(polygon, name, count, distance_m, reverse, extend)?
                    {
                        lines.push(lines2d::line_from_polyline(fresh("offset"), &points)?);
                    }
                }
                None => {
                    for ring in lines2d::offset_outline(polygon, count, distance_m)? {
                        lines.push(lines2d::shape_from_ring(fresh("offset"), &ring)?);
                    }
                }
            }
            polygon.lines.extend(lines);
            selected = Some(panel);
        }
        Command::PerpendicularLine {
            panel,
            path,
            at_m,
            angle_degrees,
            length_m,
            reverse,
        } => {
            let index = panel_index(&template, &panel)?;
            let polygon = template.panels[index]
                .polygon
                .as_mut()
                .ok_or("Internal lines require a drafted panel")?;
            let [a, b] = crate::garment::lines2d::perpendicular_line(
                polygon,
                &path,
                at_m,
                angle_degrees,
                length_m,
                reverse,
            )?;
            let id = unused_id("line", polygon.paths()?.into_iter().map(|(n, _)| n));
            polygon.lines.push(crate::garment::domain::InternalLine {
                id,
                points: vec![a, b],
                tangents: vec![DVec2::ZERO; 2],
                closed: false,
                group: None,
            });
            selected = Some(panel);
        }
        Command::CleanUpPoints {
            panels,
            overlap_m,
            straight_degrees,
        } => {
            use crate::garment::lines2d;
            if !overlap_m.is_finite() || !(0.0..=0.01).contains(&overlap_m) {
                return Err("The overlap distance must be between 0 and 10 mm".into());
            }
            if straight_degrees.is_some_and(|d| !(0.0..=10.0).contains(&d)) {
                return Err("The straightness limit must be between 0° and 10°".into());
            }
            let mut changed = 0;
            for id in &panels {
                let index = panel_index(&template, id)?;
                let sewn: Vec<String> = template.panels[index]
                    .polygon
                    .as_ref()
                    .map(|p| {
                        p.edges
                            .iter()
                            .filter(|e| seam_uses(&template, id, e))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let p = &mut template.panels[index];
                let Some(polygon) = p.polygon.as_mut() else {
                    continue;
                };
                let keep = |e: &str| {
                    sewn.iter().any(|s| s == e)
                        || polygon.target_lengths.contains_key(e)
                        || polygon.seam_allowances.contains_key(e)
                        || polygon.notches.contains_key(e)
                };
                let points = lines2d::removable_points(polygon, overlap_m, straight_degrees, keep);
                for k in &points {
                    polygon.points.remove(*k);
                    let edge = polygon.edges.remove(*k);
                    polygon.curves.remove(&edge);
                }
                changed += points.len();
                for line in &mut polygon.lines {
                    for k in lines2d::removable_line_points(line, overlap_m) {
                        line.points.remove(k);
                        line.tangents.remove(k);
                        changed += 1;
                    }
                }
                normalize(p);
            }
            if changed == 0 {
                return Err("No points to remove".into());
            }
            selected = panels.first().cloned();
        }
        Command::GroupLines { panel, lines } => {
            let index = panel_index(&template, &panel)?;
            let polygon = template.panels[index]
                .polygon
                .as_mut()
                .ok_or("Internal lines require a drafted panel")?;
            if lines.len() < 2 {
                return Err("Pick at least two internal lines to group".into());
            }
            if let Some(missing) = lines
                .iter()
                .find(|id| !polygon.lines.iter().any(|l| &l.id == *id))
            {
                return Err(format!("Internal line {missing} not found"));
            }
            let joined: Vec<String> = polygon
                .lines
                .iter()
                .filter(|l| lines.contains(&l.id))
                .filter_map(|l| l.group.clone())
                .collect();
            let name = joined.first().cloned().unwrap_or_else(|| {
                unused_id(
                    "group",
                    polygon.lines.iter().filter_map(|l| l.group.clone()),
                )
            });
            for line in &mut polygon.lines {
                if lines.contains(&line.id)
                    || line.group.as_ref().is_some_and(|g| joined.contains(g))
                {
                    line.group = Some(name.clone());
                }
            }
            selected = Some(panel);
        }
        Command::UngroupLines { panel, line } => {
            let index = panel_index(&template, &panel)?;
            let polygon = template.panels[index]
                .polygon
                .as_mut()
                .ok_or("Internal lines require a drafted panel")?;
            let group = polygon
                .lines
                .iter()
                .find(|l| l.id == line)
                .ok_or("Internal line not found")?
                .group
                .clone()
                .ok_or("This line is not in a group")?;
            for l in &mut polygon.lines {
                if l.group.as_ref() == Some(&group) {
                    l.group = None;
                }
            }
            selected = Some(panel);
        }
        Command::OptimizeCurvePoints { panels, ratio } => {
            if !(0.0..=1.0).contains(&ratio) {
                return Err("The ratio must be between 0 and 100 %".into());
            }
            let mut changed = 0;
            for id in &panels {
                let index = panel_index(&template, id)?;
                let kept = protected_boundaries(&template, index);
                let sewn_lines: Vec<String> = template.panels[index]
                    .polygon
                    .as_ref()
                    .map(|p| {
                        p.lines
                            .iter()
                            .filter(|l| seam_uses(&template, id, &l.id))
                            .map(|l| l.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let p = &mut template.panels[index];
                let Some(polygon) = p.polygon.as_mut() else {
                    continue;
                };
                let before = polygon.points.len()
                    + polygon.lines.iter().map(|l| l.points.len()).sum::<usize>();
                crate::garment::curvefit::optimize_outline(polygon, ratio, |e| {
                    kept.iter().any(|k| k == e)
                })?;
                for line in &mut polygon.lines {
                    if !sewn_lines.contains(&line.id) {
                        crate::garment::curvefit::optimize_line(line, ratio)?;
                    }
                }
                let after = polygon.points.len()
                    + polygon.lines.iter().map(|l| l.points.len()).sum::<usize>();
                changed += before - after;
                normalize(p);
            }
            if changed == 0 {
                return Err("No curve points to optimize".into());
            }
            selected = panels.first().cloned();
        }
        Command::SetBoardLocks {
            panels,
            internal_lines,
        } => {
            if let Some(id) = panels
                .iter()
                .find(|id| !template.panels.iter().any(|p| &p.id == *id))
            {
                return Err(format!("Cannot lock {id}: panel not found"));
            }
            if panels == template.locked_panels && internal_lines == template.internal_lines_locked
            {
                return Err("The 2D locks are already set that way".into());
            }
            template.locked_panels = panels;
            template.internal_lines_locked = internal_lines;
            // A locked piece cannot stay selected on the board.
            selected = None;
        }
        Command::FlipNormals { panels } => {
            if panels.is_empty() {
                return Err("Select pieces to flip".into());
            }
            for id in &panels {
                if !template.panels.iter().any(|p| &p.id == id) {
                    return Err(format!("Cannot flip {id}: panel not found"));
                }
                if !template.flipped_panels.remove(id) {
                    template.flipped_panels.insert(id.clone());
                }
            }
            selected = panels.first().cloned();
        }
        Command::SetSeamFold { seam, fold } => {
            if !template.seams.iter().any(|s| s.id == seam) {
                return Err(format!("There is no seam {seam}"));
            }
            let before = template.seam_folds.get(&seam).cloned();
            if before == fold {
                return Err("The seam is set that way already".into());
            }
            match fold {
                Some(f) => {
                    template.seam_folds.insert(seam, f);
                }
                None => {
                    template.seam_folds.remove(&seam);
                }
            }
        }
        Command::AddTack { tack } => {
            for p in [&tack.a, &tack.b] {
                if !template.panels.iter().any(|q| q.id == p.panel) || !p.uv.is_finite() {
                    return Err(format!("Tack {} refers to an unknown panel", tack.id));
                }
            }
            if template.tacks.iter().any(|t| t.id == tack.id) {
                return Err(format!("A tack called {} exists already", tack.id));
            }
            selected = Some(tack.a.panel.clone());
            template.tacks.push(tack);
        }
        Command::RemoveTacks { ids } => {
            if ids.is_empty() {
                return Err("No tacks to remove".into());
            }
            for id in &ids {
                if !template.tacks.iter().any(|t| &t.id == id) {
                    return Err(format!("There is no tack {id}"));
                }
            }
            template.tacks.retain(|t| !ids.contains(&t.id));
        }
        Command::SetLineFold { panel, line, fold } => {
            if !line_exists(&template, &panel, &line) {
                return Err(format!("Panel {panel} has no internal line {line}"));
            }
            if let Some(f) = &fold
                && (f.panel != panel || f.line != line)
            {
                return Err("The fold belongs to another line".into());
            }
            let before = template
                .line_folds
                .iter()
                .position(|f| f.panel == panel && f.line == line);
            match (before, fold) {
                (None, None) => return Err("That line has no fold".into()),
                (Some(i), None) => {
                    template.line_folds.remove(i);
                }
                (Some(i), Some(f)) => {
                    if template.line_folds[i] == f {
                        return Err("The fold is set that way already".into());
                    }
                    template.line_folds[i] = f;
                }
                (None, Some(f)) => template.line_folds.push(f),
            }
            selected = Some(panel);
        }
        Command::SetLineSpring {
            panel,
            line,
            spring,
        } => {
            if !line_exists(&template, &panel, &line) {
                return Err(format!(
                    "Panel {panel} has no segment or internal line {line}"
                ));
            }
            if let Some(s) = &spring
                && (s.panel != panel || s.line != line)
            {
                return Err("The elastic belongs to another line".into());
            }
            let before = template
                .line_springs
                .iter()
                .position(|s| s.panel == panel && s.line == line);
            match (before, spring) {
                (None, None) => return Err("That line has no elastic or tape".into()),
                (Some(i), None) => {
                    template.line_springs.remove(i);
                }
                (Some(i), Some(s)) => {
                    if template.line_springs[i] == s {
                        return Err("The elastic is set that way already".into());
                    }
                    template.line_springs[i] = s;
                }
                (None, Some(s)) => template.line_springs.push(s),
            }
            selected = Some(panel);
        }
        Command::SetShrink { panels, warp, weft } => {
            if panels.is_empty() {
                return Err("Select pieces to set their shrinkage".into());
            }
            let range = crate::garment::template::SHRINK_RANGE;
            if !range.contains(&warp) || !range.contains(&weft) {
                return Err(format!(
                    "Shrinkage goes from {} to {} %",
                    range.start(),
                    range.end()
                ));
            }
            let mut changed = false;
            for id in &panels {
                if !template.panels.iter().any(|p| &p.id == id) {
                    return Err(format!("Cannot shrink {id}: panel not found"));
                }
                let before = template
                    .panel_shrink
                    .get(id)
                    .copied()
                    .unwrap_or([100., 100.]);
                changed |= before != [warp, weft];
                if warp == 100. && weft == 100. {
                    template.panel_shrink.remove(id);
                } else {
                    template.panel_shrink.insert(id.clone(), [warp, weft]);
                }
            }
            if !changed {
                return Err("The shrinkage is set that way already".into());
            }
            selected = panels.first().cloned();
        }
        Command::SetPressure { panels, pressure } => {
            if panels.is_empty() {
                return Err("Select pieces to set their pressure".into());
            }
            let limit = crate::garment::template::MAX_PRESSURE;
            if !pressure.is_finite() || pressure.abs() > limit {
                return Err(format!("Pressure must be within ±{limit} Pa"));
            }
            let mut changed = false;
            for id in &panels {
                if !template.panels.iter().any(|p| &p.id == id) {
                    return Err(format!("Cannot set the pressure of {id}: panel not found"));
                }
                let before = template.panel_pressure.get(id).copied().unwrap_or(0.);
                changed |= before != pressure;
                if pressure == 0. {
                    template.panel_pressure.remove(id);
                } else {
                    template.panel_pressure.insert(id.clone(), pressure);
                }
            }
            if !changed {
                return Err(format!("The pressure is {pressure} Pa already"));
            }
            selected = panels.first().cloned();
        }
        Command::Strengthen { panels, on } => {
            if panels.is_empty() {
                return Err("Select pieces to strengthen".into());
            }
            let mut changed = false;
            for id in &panels {
                if !template.panels.iter().any(|p| &p.id == id) {
                    return Err(format!("Cannot strengthen {id}: panel not found"));
                }
                changed |= if on {
                    template.strengthened_panels.insert(id.clone())
                } else {
                    template.strengthened_panels.remove(id)
                };
            }
            if !changed {
                return Err(if on {
                    "Those pieces are strengthened already".into()
                } else {
                    "Those pieces are not strengthened".into()
                });
            }
            selected = panels.first().cloned();
        }
        Command::SetLayer { panels, layer } => {
            if panels.is_empty() {
                return Err("Select pieces to put on a layer".into());
            }
            if layer > crate::garment::template::MAX_LAYER {
                return Err(format!(
                    "Layers go from 0 to {}",
                    crate::garment::template::MAX_LAYER
                ));
            }
            let mut changed = false;
            for id in &panels {
                if !template.panels.iter().any(|p| &p.id == id) {
                    return Err(format!("Cannot set the layer of {id}: panel not found"));
                }
                let before = template.panel_layers.get(id).copied().unwrap_or(0);
                changed |= before != layer;
                if layer == 0 {
                    template.panel_layers.remove(id);
                } else {
                    template.panel_layers.insert(id.clone(), layer);
                }
            }
            if !changed {
                return Err(format!("Already on layer {layer}"));
            }
            selected = panels.first().cloned();
        }
        Command::FabricAppearance { fabric, appearance } => {
            if !template.fabrics.iter().any(|f| f.id == fabric) {
                return Err("Fabric not found".into());
            }
            if let Some(appearance) = appearance {
                appearance.validate()?;
                template
                    .fabric_appearance
                    .insert(fabric.clone(), appearance);
            } else {
                template.fabric_appearance.remove(&fabric);
            }
            selected = template
                .panels
                .iter()
                .find(|p| p.fabric == fabric)
                .map(|p| p.id.clone());
        }
        Command::Fabric {
            panel,
            name,
            properties,
            all_panels,
        } => {
            properties.bind(0.).validate()?;
            if name.trim().is_empty() {
                return Err("Fabric name is empty".into());
            }
            let index = template
                .panels
                .iter()
                .position(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let old_id = template.panels[index].fabric.clone();
            let shared = template
                .panels
                .iter()
                .filter(|p| p.fabric == old_id)
                .count()
                > 1;
            let id = if shared && !all_panels {
                let id = unused_id("fabric", template.fabrics.iter().map(|f| f.id.clone()));
                if let Some(optics) = template.fabric_appearance.get(&old_id).cloned() {
                    template.fabric_appearance.insert(id.clone(), optics);
                }
                template.fabrics.push(Fabric {
                    id: id.clone(),
                    name,
                    properties,
                });
                id
            } else {
                let f = template
                    .fabrics
                    .iter_mut()
                    .find(|f| f.id == old_id)
                    .ok_or("Fabric not found")?;
                f.name = name;
                f.properties = properties;
                old_id
            };
            for (i, p) in template.panels.iter_mut().enumerate() {
                if all_panels || i == index {
                    p.fabric = id.clone();
                }
            }
            selected = Some(panel);
        }
        Command::Polygon {
            mut points,
            mut tangents,
            meshing,
            fabric,
            replace,
        } => {
            if !(3..=64).contains(&points.len()) || points.iter().any(|p| !p.is_finite()) {
                return Err("Draw 3 to 64 finite points".into());
            }
            if tangents.is_empty() {
                tangents.resize(points.len(), DVec2::ZERO);
            }
            if tangents.len() != points.len() || tangents.iter().any(|p| !p.is_finite()) {
                return Err("Invalid drawing tangents".into());
            }
            let lo = points
                .iter()
                .copied()
                .fold(DVec2::splat(f64::INFINITY), DVec2::min);
            for p in &mut points {
                *p -= lo;
            }
            let size = points.iter().copied().fold(DVec2::ZERO, DVec2::max);
            let area: f64 = (0..points.len())
                .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
                .sum();
            if area < 0. {
                points.reverse();
                tangents.reverse();
                for t in &mut tangents {
                    *t = -*t;
                }
            }
            if replace {
                template.name = "Custom garment".into();
                template.panels.clear();
                template.seams.clear();
            }
            let id = unused_id("panel", template.panels.iter().map(|p| p.id.clone()));
            let curves = (0..points.len())
                .filter_map(|i| {
                    let j = (i + 1) % points.len();
                    (tangents[i].length_squared() + tangents[j].length_squared() > 1e-20).then(
                        || {
                            (
                                format!("edge_{}", i + 1),
                                [points[i] + tangents[i], points[j] - tangents[j]],
                            )
                        },
                    )
                })
                .collect();
            template.panels.push(Panel {
                id: id.clone(),
                name: id.clone(),
                pattern: Pattern {
                    width: size.x,
                    height: size.y,
                    nx: 1,
                    ny: 1,
                },
                polygon: Some(PolygonPattern {
                    edges: (0..points.len())
                        .map(|i| format!("edge_{}", i + 1))
                        .collect(),
                    points,
                    subdivisions: 0,
                    meshing: Some(meshing),
                    curves,
                    holes: vec![],
                    lines: vec![],
                    target_lengths: Default::default(),
                    seam_allowances: Default::default(),
                    notches: Default::default(),
                }),
                fabric,
                grain_degrees: 90.,
                placement: Placement {
                    translation: if let Some(first) = template.panels.first() {
                        let delta = lo - layout(&template, 0);
                        first.placement.translation + DVec3::new(delta.x, -delta.y, 0.12)
                    } else {
                        DVec3::new(-size.x * 0.5, 1.5, 0.45)
                    },
                    ..Default::default()
                },
                pins: vec![],
                pin_points: vec![],
                layout_2d: Some(lo),
                layout_rotation_degrees: 0.,
            });
            normalize(template.panels.last_mut().unwrap());
            template.version = 2;
            selected = Some(id);
        }
        Command::Place { panel, placement } => {
            template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?
                .placement = placement;
            selected = Some(panel);
        }
        Command::Arrange {
            panel,
            placement,
            wrap,
        } => {
            if let Some(w) = &wrap {
                w.validate()?;
            }
            template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?
                .placement = placement;
            match wrap {
                Some(w) => {
                    template.panel_wraps.insert(panel.clone(), w);
                }
                None => {
                    template.panel_wraps.remove(&panel);
                }
            }
            selected = Some(panel);
        }
        Command::PlaceGroup {
            panels,
            pivot,
            translation,
            rotation,
        } => {
            if panels.is_empty() || panels.len() > template.panels.len() {
                return Err("Select existing panels for group placement".into());
            }
            let mut seen = std::collections::BTreeSet::new();
            for id in &panels {
                if !seen.insert(id) {
                    return Err("Group placement contains a duplicate panel".into());
                }
                let panel = template
                    .panels
                    .iter_mut()
                    .find(|p| &p.id == id)
                    .ok_or("Group placement panel not found")?;
                panel.placement =
                    transform_placement(&panel.placement, pivot, translation, rotation)?;
            }
            selected = panels.first().cloned();
        }
        Command::Shape { panel, polygon } => {
            let index = template
                .panels
                .iter()
                .position(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let origin = layout(&template, index);
            let p = &mut template.panels[index];
            let old = p
                .polygon
                .as_ref()
                .ok_or("Convert the rectangular pattern before editing")?;
            if polygon.edges != old.edges || polygon.points.len() != old.points.len() {
                return Err("Named boundaries must stay stable".into());
            }
            p.layout_2d = Some(origin);
            p.polygon = Some(polygon);
            normalize(p);
            selected = Some(panel);
        }
        Command::Dimensions {
            panel,
            width,
            height,
        } => {
            if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
                return Err("Dimensions must be positive finite values".into());
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            if p.polygon.is_some() {
                resize_rectangle(p, width, height)?;
                normalize(p);
            }
            // Re-assert the requested size after normalization: the stored value
            // must be the exact input, never a rounded copy of the contour.
            p.pattern.width = width;
            p.pattern.height = height;
            selected = Some(panel);
        }
        Command::Anchor {
            panel,
            anchor,
            position,
        } => {
            if !position.is_finite() {
                return Err("Point coordinates must be finite".into());
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            // Absolute coordinates are assigned directly, never reconstructed
            // from a delta, so the typed value is stored bit-for-bit.
            remap_anchors(p, &[anchor], |_| position)?;
            // Re-basing the local origin keeps every untouched point at the same
            // board and world position, exactly like a dragged contour edit.
            normalize(p);
            selected = Some(panel);
        }
        Command::MovePoints {
            panel,
            points,
            delta,
        } => {
            if !delta.is_finite() {
                return Err("Move delta must be finite".into());
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            remap_anchors(p, &points, |point| point + delta)?;
            normalize(p);
            selected = Some(panel);
        }
        Command::MovePointGroups {
            groups,
            board_delta,
        } => {
            if !board_delta.is_finite() || groups.is_empty() {
                return Err("Select points and use a finite move delta".into());
            }
            // Freeze derived board origins before any panel changes its extent.
            let origins: Vec<_> = (0..source.panels.len())
                .map(|i| layout(source, i))
                .collect();
            for (p, origin) in template.panels.iter_mut().zip(origins) {
                p.layout_2d = Some(origin);
            }
            let mut seen = std::collections::BTreeSet::new();
            for (id, points) in groups {
                if !seen.insert(id.clone()) {
                    return Err("A panel may occur only once in a point move".into());
                }
                let index = template
                    .panels
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or("Panel not found")?;
                let origin = layout(&template, index);
                let p = &mut template.panels[index];
                p.layout_2d = Some(origin);
                let delta = rotate_2d(board_delta, -p.layout_rotation_degrees);
                remap_anchors(p, &points, |point| point + delta)?;
                normalize(p);
            }
        }
        Command::Nudge {
            panel,
            points,
            delta_mm,
        } => {
            if !delta_mm.is_finite() {
                return Err("Nudge step must be finite".into());
            }
            // Metric steps convert once, by division, so 1 mm and 10 mm are the
            // exact f64 0.001 and 0.01.
            let delta = delta_mm / 1000.0;
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            remap_anchors(p, &points, |point| point + delta)?;
            normalize(p);
            selected = Some(panel);
        }
        Command::SplitBoundary { panel, edge, at_m } => {
            if !at_m.is_finite() || at_m <= 0. {
                return Err("Insert position must be a positive finite length".into());
            }
            if seam_uses(&template, &panel, &edge) {
                return Err(
                    "This boundary belongs to a seam; delete the seam before splitting it".into(),
                );
            }
            if template
                .panels
                .iter()
                .find(|p| p.id == panel)
                .and_then(|p| p.polygon.as_ref())
                .is_some_and(|polygon| polygon.target_lengths.contains_key(&edge))
            {
                return Err(
                    "This boundary has a target length; clear the target before splitting it"
                        .into(),
                );
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let index = {
                let polygon = p
                    .polygon
                    .as_mut()
                    .ok_or("This panel has no editable contour")?;
                let index = polygon
                    .edges
                    .iter()
                    .position(|name| name == &edge)
                    .ok_or("Boundary not found")?;
                let current = edge_length(polygon, &edge)?;
                if at_m >= current {
                    return Err("Insert position is beyond the boundary".into());
                }
                if polygon.notches.get(&edge).is_some_and(|notches| {
                    notches.iter().any(|notch| {
                        (notch.at * current - at_m).abs() < markings::end_clearance(notch)
                    })
                }) {
                    return Err(
                        "Split would place a notch within 1 mm of a boundary end or inside its V span; move or remove it first"
                            .into(),
                    );
                }
                let count = polygon.points.len();
                let a = polygon.points[index];
                let b = polygon.points[(index + 1) % count];
                let controls = polygon.curves.get(&edge).copied();
                let (point, first, second) = match controls {
                    // Any cubic parameter splits the outline exactly; the point
                    // is placed at the requested material arclength.
                    Some([c1, c2]) => {
                        let cubic = [a, c1, c2, b];
                        let parameter = parameter_at_arclength(cubic, at_m)?;
                        let (left, right) = split_cubic(cubic, parameter);
                        (left[3], [left[1], left[2]], [right[1], right[2]])
                    }
                    None => {
                        let t = at_m / current;
                        (a.lerp(b, t), [DVec2::ZERO; 2], [DVec2::ZERO; 2])
                    }
                };
                polygon.points.insert(index + 1, point);
                let name = split_edge_name(&polygon.edges, &edge);
                polygon.edges.insert(index + 1, name.clone());
                if controls.is_some() {
                    polygon.curves.insert(edge.clone(), first);
                    polygon.curves.insert(name.clone(), second);
                }
                // Carry draft markings across the split. The two halves are the
                // same physical boundary, so the allowance applies to both, and
                // each notch keeps its material arclength position.
                if let Some(width) = polygon.seam_allowances.get(&edge).copied() {
                    polygon.seam_allowances.insert(name.clone(), width);
                }
                if let Some(notches) = polygon.notches.remove(&edge) {
                    // The split point is exact on a cubic, but the two halves are
                    // sampled as separate boundaries. Use their actual sampled
                    // lengths, not the requested split length, so a curved-edge
                    // notch keeps its material arclength position.
                    let left_len = edge_length(polygon, &edge)?;
                    let right_len = edge_length(polygon, &name)?;
                    let (mut left, mut right) = (Vec::new(), Vec::new());
                    for mut notch in notches {
                        let along = notch.at * current;
                        if along < at_m {
                            notch.at = along / left_len;
                            left.push(notch);
                        } else {
                            notch.at = (along - at_m) / right_len;
                            right.push(notch);
                        }
                    }
                    for (target, list) in [(edge.clone(), left), (name.clone(), right)] {
                        if !list.is_empty() {
                            polygon.notches.insert(target, list);
                        }
                    }
                }
                index + 1
            };
            normalize(p);
            selected = Some(panel);
            new_point = Some(index);
        }
        Command::DartCutSew {
            panel,
            edge,
            left_t,
            right_t,
            apex,
        } => {
            if !left_t.is_finite()
                || !right_t.is_finite()
                || left_t <= 0.01
                || right_t >= 0.99
                || right_t - left_t < 0.005
                || !apex.is_finite()
            {
                return Err(
                    "Dart requires two distinct interior edge stations and a finite apex".into(),
                );
            }
            // All chain segments count, including tails and trimmed/reversed sides.
            if seam_uses(&template, &panel, &edge) {
                return Err("Dart placement on an already sewn edge is not supported; select an unsewn boundary".into());
            }
            if template
                .mirror_links
                .iter()
                .any(|l| l.a == panel || l.b == panel)
            {
                return Err("Unlink the mirrored pair before cutting a dart".into());
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Dart panel not found")?;
            let polygon = p
                .polygon
                .as_mut()
                .ok_or("Dart requires an editable physical polygon")?;
            let i = polygon
                .edges
                .iter()
                .position(|name| name == &edge)
                .ok_or("Dart source edge not found")?;
            if polygon.curves.contains_key(&edge) || polygon.target_lengths.contains_key(&edge) {
                return Err(
                    "Dart on a curved or annotated boundary needs explicit reference remapping"
                        .into(),
                );
            }
            let a = polygon.points[i];
            let b = polygon.points[(i + 1) % polygon.points.len()];
            let delta = b - a;
            let length = delta.length();
            if length < 0.02 {
                return Err("Dart source edge must be at least 20 mm".into());
            }
            let left = a.lerp(b, left_t);
            let right = a.lerp(b, right_t);
            let intake = left.distance(right);
            if !(0.003..=0.15).contains(&intake) {
                return Err("Dart intake must be between 3 and 150 mm".into());
            }
            let inward = DVec2::new(-delta.y, delta.x) / length;
            let mid = (left + right) * 0.5;
            let height = (apex - mid).dot(inward);
            if !(0.008..=0.5).contains(&height) || (apex - mid).dot(delta / length).abs() > 1e-6 {
                return Err(
                    "Dart apex must lie 8–500 mm inward on the intake perpendicular bisector"
                        .into(),
                );
            }
            if !polygon.holes.is_empty() || !polygon.lines.is_empty() {
                return Err(
                    "Darts on existing holes/internal lines need topology-aware migration".into(),
                );
            }
            let suffix = unused_id("dart", template.seams.iter().map(|s| s.id.clone()));
            let first = format!("{suffix}_leg_a");
            let second = format!("{suffix}_leg_b");
            let trailing = format!("{suffix}_after");
            if [&first, &second, &trailing]
                .iter()
                .any(|candidate| polygon.edges.contains(candidate))
            {
                return Err("Dart edge identifier collision".into());
            }
            polygon.points.splice((i + 1)..(i + 1), [left, apex, right]);
            polygon.edges.splice(
                (i + 1)..(i + 1),
                [first.clone(), second.clone(), trailing.clone()],
            );
            // The original sew side before the first leg retains its name.
            template.seams.push(SeamSpec {
                id: suffix,
                a: BoundaryRef {
                    panel: panel.clone(),
                    side: crate::garment::seam::Side::Top,
                    edge: Some(first),
                    range: [0., 1.],
                    reversed: false,
                },
                b: BoundaryRef {
                    panel: panel.clone(),
                    side: crate::garment::seam::Side::Top,
                    edge: Some(second),
                    range: [0., 1.],
                    reversed: false,
                },
                a_tail: vec![],
                b_tail: vec![],
                reverse_b: true,
                notches: vec![],
                stiffness: 50_000.,
                allow_ease: false,
            });
            normalize(p);
            selected = Some(panel);
        }
        Command::RemovePoint { panel, point } => {
            let (removed, incoming, outgoing) = {
                let p = template
                    .panels
                    .iter()
                    .find(|p| p.id == panel)
                    .ok_or("Panel not found")?;
                let polygon = p
                    .polygon
                    .as_ref()
                    .ok_or("This panel has no editable contour")?;
                let count = polygon.points.len();
                if count <= 3 {
                    return Err("A contour needs at least three points".into());
                }
                if point >= count {
                    return Err("Point index out of range".into());
                }
                let previous = (point + count - 1) % count;
                (
                    polygon.points[point],
                    polygon.edges[previous].clone(),
                    polygon.edges[point].clone(),
                )
            };
            for name in [&incoming, &outgoing] {
                if seam_uses(&template, &panel, name) {
                    return Err("A seam uses this point's boundaries; delete the seam first".into());
                }
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            if p.polygon.as_ref().is_some_and(|polygon| {
                polygon.target_lengths.contains_key(&incoming)
                    || polygon.target_lengths.contains_key(&outgoing)
            }) {
                return Err(
                    "A target length uses this point's boundaries; clear the target first".into(),
                );
            }
            if p.polygon.as_ref().is_some_and(|polygon| {
                [&incoming, &outgoing].iter().any(|name| {
                    polygon.seam_allowances.contains_key(*name)
                        || polygon.notches.contains_key(*name)
                })
            }) {
                return Err(
                    "Seam allowances or notches use this point's boundaries; clear them first"
                        .into(),
                );
            }
            {
                let polygon = p.polygon.as_mut().unwrap();
                if polygon.curves.contains_key(&incoming) || polygon.curves.contains_key(&outgoing)
                {
                    return Err("Remove the Bézier handles before deleting this point".into());
                }
                polygon.points.remove(point);
                // The outgoing name disappears with its point; the incoming name
                // becomes the rejoined boundary.
                polygon.edges.remove(point);
            }
            // A pin on the removed vertex cannot survive that vertex.
            p.pin_points.retain(|pin| pin.distance(removed) >= 1e-9);
            normalize(p);
            selected = Some(panel);
        }
        Command::CurvePoint {
            panel,
            anchor,
            curved,
        } => {
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let polygon = p
                .polygon
                .as_mut()
                .ok_or("This panel has no editable contour")?;
            let count = polygon.points.len();
            if anchor >= count {
                return Err("Point index out of range".into());
            }
            let previous = (anchor + count - 1) % count;
            let outgoing = polygon.edges[anchor].clone();
            let incoming = polygon.edges[previous].clone();
            for (name, start) in [(&outgoing, anchor), (&incoming, previous)] {
                if curved {
                    if !polygon.curves.contains_key(name) {
                        let a = polygon.points[start];
                        let b = polygon.points[(start + 1) % count];
                        // Collinear controls at 1/3 and 2/3 trace the straight
                        // segment exactly, so becoming a curve changes nothing.
                        polygon
                            .curves
                            .insert(name.clone(), [a.lerp(b, 1. / 3.), a.lerp(b, 2. / 3.)]);
                    }
                } else {
                    polygon.curves.remove(name);
                }
            }
            normalize(p);
            selected = Some(panel);
        }
        Command::TargetLength {
            panel,
            edge,
            target_m,
        } => {
            if !target_m.is_finite() || target_m < 0. {
                return Err("Target length must be a positive finite value".into());
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let index = p
                .polygon
                .as_ref()
                .ok_or("This panel has no editable contour")?
                .edges
                .iter()
                .position(|name| name == &edge)
                .ok_or("Boundary not found")?;
            if target_m == 0. {
                // Clearing a specification never moves geometry.
                p.polygon.as_mut().unwrap().target_lengths.remove(&edge);
                selected = Some(panel);
            } else {
                let count = p.polygon.as_ref().unwrap().points.len();
                let end = (index + 1) % count;
                let fixed = p.polygon.as_ref().unwrap().points[index];
                let before_end = p.polygon.as_ref().unwrap().points[end];
                for _ in 0..4 {
                    let current = edge_length(p.polygon.as_ref().unwrap(), &edge)?;
                    if current <= 0. {
                        return Err("Boundary has zero length".into());
                    }
                    if (current - target_m).abs() <= target_m * 1e-12 {
                        break;
                    }
                    // The first anchor stays put; the rest of the boundary scales
                    // about it, so the direction is preserved.
                    let factor = target_m / current;
                    let polygon = p.polygon.as_mut().unwrap();
                    let scale = |point: DVec2| fixed + (point - fixed) * factor;
                    polygon.points[end] = scale(polygon.points[end]);
                    if let Some([first, second]) = polygon.curves.get(&edge).copied() {
                        polygon
                            .curves
                            .insert(edge.clone(), [scale(first), scale(second)]);
                    }
                }
                let polygon = p.polygon.as_mut().unwrap();
                let delta = polygon.points[end] - before_end;
                if let Some(controls) = polygon.curves.get_mut(&polygon.edges[end]) {
                    controls[0] += delta;
                }
                follow_pins(p, &[(end, before_end)]);
                normalize(p);
                p.polygon
                    .as_mut()
                    .unwrap()
                    .target_lengths
                    .insert(edge, target_m);
                selected = Some(panel);
            }
        }
        Command::SeamAllowance {
            panel,
            edge,
            width_m,
        } => {
            if !width_m.is_finite() || width_m < 0. || width_m > markings::MAX_ALLOWANCE_M {
                return Err(format!(
                    "Seam allowance must be in [0, {} mm]",
                    markings::MAX_ALLOWANCE_M * 1000.
                ));
            }
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let polygon = p
                .polygon
                .as_mut()
                .ok_or("This panel has no editable contour")?;
            if polygon.closed_boundary(&edge).is_none() {
                return Err("Seam allowance needs a named outer or hole boundary".into());
            }
            if width_m == 0. {
                polygon.seam_allowances.remove(&edge);
            } else {
                polygon.seam_allowances.insert(edge, width_m);
            }
            selected = Some(panel);
        }
        Command::SetNotch {
            panel,
            edge,
            index,
            notch,
        } => {
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let polygon = p
                .polygon
                .as_mut()
                .ok_or("This panel has no editable contour")?;
            let points = polygon
                .closed_boundary(&edge)
                .ok_or("A notch needs a named outer or hole boundary")?;
            markings::validate_notch(&notch, markings::polyline_length(&points))?;
            let list = polygon.notches.entry(edge).or_default();
            match index {
                Some(i) if i < list.len() => list[i] = notch,
                Some(_) => return Err("Notch index out of range".into()),
                None => list.push(notch),
            }
            selected = Some(panel);
        }
        Command::RemoveNotch { panel, edge, index } => {
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            let polygon = p
                .polygon
                .as_mut()
                .ok_or("This panel has no editable contour")?;
            let list = polygon
                .notches
                .get_mut(&edge)
                .ok_or("This boundary has no notches")?;
            if index >= list.len() {
                return Err("Notch index out of range".into());
            }
            list.remove(index);
            if list.is_empty() {
                polygon.notches.remove(&edge);
            }
            selected = Some(panel);
        }
        Command::Layout { panel, origin } => {
            template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?
                .layout_2d = Some(origin);
            selected = Some(panel);
        }
        Command::BoardTransform {
            panel,
            origin,
            rotation_degrees,
        } => {
            let p = template
                .panels
                .iter_mut()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?;
            if !rotation_degrees.is_finite() {
                return Err("Invalid 2D rotation".into());
            }
            p.layout_2d = Some(origin);
            p.layout_rotation_degrees = rotation_degrees.rem_euclid(360.);
            selected = Some(panel);
        }
        Command::Duplicate { panel } => {
            let mut copy = template
                .panels
                .iter()
                .find(|p| p.id == panel)
                .ok_or("Panel not found")?
                .clone();
            copy.id = unused_id("panel", template.panels.iter().map(|p| p.id.clone()));
            copy.name = format!("{} copy", copy.name);
            copy.placement.translation.x += copy.pattern.width + 0.05;
            let i = template.panels.iter().position(|p| p.id == panel).unwrap();
            copy.layout_2d = Some(layout(&template, i) + DVec2::new(copy.pattern.width + 0.15, 0.));
            for graphic in source.graphics.iter().filter(|g| g.panel_id == panel) {
                let mut graphic = graphic.clone();
                graphic.id = unused_id("graphic", template.graphics.iter().map(|g| g.id.clone()));
                graphic.panel_id = copy.id.clone();
                template.graphics.push(graphic);
            }
            for trim in source.trims.iter().filter(|t| t.panel_id == panel) {
                let mut trim = trim.clone();
                trim.id = unused_id("trim", template.trims.iter().map(|t| t.id.clone()));
                trim.panel_id = copy.id.clone();
                template.trims.push(trim);
            }
            selected = Some(copy.id.clone());
            template.panels.push(copy);
        }
        Command::RemovePanel { panel } => {
            if template.panels.len() <= 1 {
                return Err("Keep at least one panel".into());
            }
            if !template.panels.iter().any(|p| p.id == panel) {
                return Err("Panel not found".into());
            }
            template.panels.retain(|p| p.id != panel);
            template
                .seams
                .retain(|s| s.references().all(|r| r.panel != panel));
        }
        Command::RemovePanels { panels } => {
            let ids: std::collections::BTreeSet<_> = panels.into_iter().collect();
            if ids.is_empty()
                || ids
                    .iter()
                    .any(|id| !template.panels.iter().any(|p| &p.id == id))
            {
                return Err("Select existing panels to delete".into());
            }
            if ids.len() >= template.panels.len() {
                return Err("Keep at least one panel".into());
            }
            template.panels.retain(|p| !ids.contains(&p.id));
            template
                .seams
                .retain(|s| s.references().all(|r| !ids.contains(&r.panel)));
        }
        Command::ArchivePanels { panels } => {
            let ids: std::collections::BTreeSet<_> = panels.iter().cloned().collect();
            if ids.is_empty()
                || ids
                    .iter()
                    .any(|id| !template.panels.iter().any(|p| &p.id == id))
            {
                return Err("Select existing panels to archive".into());
            }
            if ids.len() >= template.panels.len() {
                return Err("Keep at least one panel on the board".into());
            }
            let held = |id: &String| {
                template.pin_targets.iter().any(|p| &p.panel == id)
                    || template.frozen_panels.iter().any(|p| &p.panel == id)
            };
            if ids.iter().any(held) {
                return Err("Release pins and unfreeze the pieces before archiving them".into());
            }
            let ordered: Vec<String> = template
                .panels
                .iter()
                .filter(|p| ids.contains(&p.id))
                .map(|p| p.id.clone())
                .collect();
            let pieces = clipboard::PatternClipboard::capture(&template, &ordered)?;
            let center = clipboard::layout_center(&template, &ordered);
            let names = template
                .panels
                .iter()
                .filter(|p| ids.contains(&p.id))
                .map(|p| p.name.clone())
                .collect();
            let crossing = template
                .seams
                .iter()
                .filter(|s| {
                    s.references().any(|r| ids.contains(&r.panel))
                        && s.references().any(|r| !ids.contains(&r.panel))
                })
                .cloned()
                .collect();
            let id = unused_id("archive", template.archive.iter().map(|a| a.id.clone()));
            template.panels.retain(|p| !ids.contains(&p.id));
            template
                .seams
                .retain(|s| s.references().all(|r| !ids.contains(&r.panel)));
            template
                .archive
                .push(crate::garment::template::ArchivedPieces {
                    id,
                    ids: ordered,
                    names,
                    seams: crossing,
                    center,
                    pieces,
                });
        }
        Command::RestoreArchived { id } => {
            let k = template
                .archive
                .iter()
                .position(|a| a.id == id)
                .ok_or("Archived pieces not found")?;
            let entry = template.archive.remove(k);
            let created = entry.pieces.restore(&mut template, entry.center)?;
            let renamed: std::collections::BTreeMap<&String, &String> =
                entry.ids.iter().zip(&created).collect();
            // Sew the pieces back to the ones still on the board.
            for seam in entry.seams {
                let mut seam = seam;
                for r in seam.references_mut() {
                    if let Some(new) = renamed.get(&r.panel) {
                        r.panel = (*new).clone();
                    }
                }
                let present = seam
                    .references()
                    .all(|r| template.panels.iter().any(|p| p.id == r.panel));
                if present {
                    if template.seams.iter().any(|s| s.id == seam.id) {
                        seam.id = unused_id("seam", template.seams.iter().map(|s| s.id.clone()));
                    }
                    template.seams.push(seam);
                }
            }
            selected = created.first().cloned();
        }
        Command::DeleteArchived { id } => {
            if !template.archive.iter().any(|a| a.id == id) {
                return Err("Archived pieces not found".into());
            }
            template.archive.retain(|a| a.id != id);
        }
        Command::Sew {
            a,
            b,
            reverse,
            ease,
        } => {
            let id = unused_id("seam", template.seams.iter().map(|s| s.id.clone()));
            template.seams.push(SeamSpec {
                a_tail: vec![],
                b_tail: vec![],
                id,
                a,
                b,
                reverse_b: reverse,
                allow_ease: ease,
                notches: vec![],
                stiffness: 50_000.,
            });
        }
        Command::SewChain {
            seam,
            mut a,
            mut b,
            reverse,
            ease,
        } => {
            if a.is_empty() || b.is_empty() || a.len() > 128 || b.len() > 128 {
                return Err("Choose 1..128 segments on each sewing side".into());
            }
            let old = if let Some(id) = &seam {
                Some(
                    template
                        .seams
                        .iter()
                        .find(|s| &s.id == id)
                        .ok_or("Seam not found")?
                        .clone(),
                )
            } else {
                None
            };
            let id = seam
                .unwrap_or_else(|| unused_id("seam", template.seams.iter().map(|s| s.id.clone())));
            template.seams.retain(|s| s.id != id);
            template.seams.push(SeamSpec {
                id,
                a: a.remove(0),
                b: b.remove(0),
                a_tail: a,
                b_tail: b,
                reverse_b: reverse,
                allow_ease: ease,
                stiffness: old.as_ref().map_or(50_000., |s| s.stiffness),
                notches: old.map_or_else(Vec::new, |s| s.notches),
            });
        }
        Command::RemoveSeam { seam } => {
            if !template.seams.iter().any(|s| s.id == seam) {
                return Err("Seam not found".into());
            }
            template.seams.retain(|s| s.id != seam);
        }
        Command::SetGraphic { placement } => {
            placement.validate_panel(&template)?;
            selected = Some(placement.panel_id.clone());
            if let Some(existing) = template.graphics.iter_mut().find(|g| g.id == placement.id) {
                if *existing == placement {
                    return Err("Graphic placement is unchanged".into());
                }
                *existing = placement;
            } else {
                template.graphics.push(placement);
            }
        }
        Command::RemoveGraphic { id } => {
            if !template.graphics.iter().any(|g| g.id == id) {
                return Err("Graphic not found".into());
            }
            template.graphics.retain(|g| g.id != id);
        }
        Command::SetTrim { trim } => {
            trim.validate()?;
            if !template.panels.iter().any(|p| p.id == trim.panel_id) {
                return Err("Trim references a missing panel".into());
            }
            selected = Some(trim.panel_id.clone());
            if let Some(existing) = template.trims.iter_mut().find(|t| t.id == trim.id) {
                if *existing == trim {
                    return Err("Trim is unchanged".into());
                }
                *existing = trim;
            } else {
                template.trims.push(trim);
            }
        }
        Command::RemoveTrim { id } => {
            if !template.trims.iter().any(|t| t.id == id) {
                return Err("Trim not found".into());
            }
            template.trims.retain(|t| t.id != id);
        }
        Command::RepeatTrim {
            id,
            copies,
            offset_m,
        } => {
            use crate::garment::trims::TrimKind;
            if !(1..=128).contains(&copies)
                || !offset_m.is_finite()
                || offset_m.length_squared() < 1e-24
                || template.trims.len().saturating_add(copies) > 4096
            {
                return Err(
                    "Repeat requires 1–128 copies, a finite nonzero offset and at most 4096 trims"
                        .into(),
                );
            }
            let original = template
                .trims
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or("Trim not found")?;
            if template
                .mirror_links
                .iter()
                .any(|link| link.a == original.panel_id || link.b == original.panel_id)
            {
                return Err("Unlink the mirrored panel before repeating trims".into());
            }
            let mut ids: std::collections::HashSet<String> =
                template.trims.iter().map(|t| t.id.clone()).collect();
            let mut next = 1usize;
            let mut additions = Vec::with_capacity(copies);
            for i in 1..=copies {
                let mut copy = original.clone();
                loop {
                    let candidate = format!("trim_{next}");
                    next += 1;
                    if ids.insert(candidate.clone()) {
                        copy.id = candidate;
                        break;
                    }
                }
                let delta = offset_m * i as f64;
                match &mut copy.kind {
                    TrimKind::Button { center_m, .. } => *center_m += delta,
                    TrimKind::Buttonhole { start_m, end_m, .. } => {
                        *start_m += delta;
                        *end_m += delta;
                    }
                    TrimKind::Zipper { path_m, .. } | TrimKind::Topstitch { path_m, .. } => {
                        for point in path_m {
                            *point += delta;
                        }
                    }
                }
                copy.validate()?;
                additions.push(copy);
            }
            selected = Some(original.panel_id);
            template.trims.extend(additions);
        }
    }
    linked::synchronize(source, &mut template)?;
    // Placement coordinates refer to the existing material frame. Until topology
    // remapping exists, reject edits that change that frame rather than shifting
    // artwork silently. Meshing density and length specifications do not change it.
    let mut checked_graphic_panels = std::collections::BTreeSet::new();
    for graphic in &source.graphics {
        if !checked_graphic_panels.insert(&graphic.panel_id) {
            continue;
        }
        let old = source
            .panels
            .iter()
            .find(|p| p.id == graphic.panel_id)
            .ok_or("Graphic references a missing panel")?;
        let Some(new) = template.panels.iter().find(|p| p.id == graphic.panel_id) else {
            if deletes_panels {
                continue;
            }
            return Err("Remove graphics before replacing their panel".into());
        };
        if graphic_material_frame(old)? != graphic_material_frame(new)? {
            return Err("Remove graphics before changing their panel's material contour".into());
        }
    }
    let mut checked_trim_panels = std::collections::BTreeSet::new();
    for trim in &source.trims {
        if !checked_trim_panels.insert(&trim.panel_id) {
            continue;
        }
        let old = source
            .panels
            .iter()
            .find(|p| p.id == trim.panel_id)
            .ok_or("Trim references a missing panel")?;
        let Some(new) = template.panels.iter().find(|p| p.id == trim.panel_id) else {
            if deletes_panels {
                continue;
            }
            return Err("Remove trims before replacing their panel".into());
        };
        if mirrored_trim_panel.as_ref() != Some(&trim.panel_id)
            && graphic_material_frame(old)? != graphic_material_frame(new)?
        {
            return Err("Remove trims before changing their panel's material contour".into());
        }
    }
    template
        .graphics
        .retain(|g| template.panels.iter().any(|p| p.id == g.panel_id));
    template
        .trims
        .retain(|t| template.panels.iter().any(|p| p.id == t.panel_id));
    template
        .pin_targets
        .retain(|target| template.panels.iter().any(|p| p.id == target.panel));
    template
        .frozen_panels
        .retain(|target| template.panels.iter().any(|p| p.id == target.panel));
    template
        .panel_wraps
        .retain(|id, _| template.panels.iter().any(|p| &p.id == id));
    template
        .locked_panels
        .retain(|id| template.panels.iter().any(|p| &p.id == id));
    prune_simulation_settings(&mut template);
    let garment = template.compile()?;
    Ok(Edit {
        reset_simulation: !same_physics(source, &template),
        template,
        garment,
        selected,
        new_point,
    })
}

/// Rigid rotation around a material-space pivot. Rest UV and area never change.
pub fn rotate_about(placement: &Placement, pivot: DVec3, axis: DVec3, angle: f64) -> Placement {
    let r = placement.rotation_degrees * std::f64::consts::PI / 180.;
    let old = DQuat::from_euler(EulerRot::XYZ, r.x, r.y, r.z);
    let new = DQuat::from_axis_angle(axis, angle) * old;
    let (x, y, z) = new.to_euler(EulerRot::XYZ);
    Placement {
        translation: placement.transform(pivot) - new * pivot,
        rotation_degrees: DVec3::new(x, y, z) * 180. / std::f64::consts::PI,
    }
}

/// Compose a world-space rigid motion with a panel arrangement. The quaternion
/// must be unit length; invalid transforms fail rather than scaling cloth.
pub fn transform_placement(
    placement: &Placement,
    pivot: DVec3,
    translation: DVec3,
    rotation: DQuat,
) -> Result<Placement, String> {
    if !pivot.is_finite()
        || !translation.is_finite()
        || !rotation.is_finite()
        || (rotation.length_squared() - 1.).abs() > 1e-10
        || !placement.translation.is_finite()
        || !placement.rotation_degrees.is_finite()
    {
        return Err("Group placement requires a finite rigid transform".into());
    }
    if rotation == DQuat::IDENTITY || rotation == -DQuat::IDENTITY {
        let next = placement.translation + translation;
        if !next.is_finite() {
            return Err("Group placement exceeds finite coordinates".into());
        }
        return Ok(Placement {
            translation: next,
            ..placement.clone()
        });
    }
    let rotation = rotation.normalize();
    let r = placement.rotation_degrees * (std::f64::consts::PI / 180.);
    let orientation = rotation * DQuat::from_euler(EulerRot::XYZ, r.x, r.y, r.z);
    let (x, y, z) = orientation.to_euler(EulerRot::XYZ);
    let result = Placement {
        translation: pivot + rotation * (placement.translation - pivot) + translation,
        rotation_degrees: DVec3::new(x, y, z) * (180. / std::f64::consts::PI),
    };
    if !result.translation.is_finite() || !result.rotation_degrees.is_finite() {
        return Err("Group placement exceeds finite coordinates".into());
    }
    Ok(result)
}

/// History stores editable templates, never GPU resources or transient indices.
#[derive(Default)]
pub struct History {
    undo: Vec<GarmentTemplate>,
    redo: Vec<GarmentTemplate>,
}
impl History {
    pub fn record(&mut self, old: GarmentTemplate) {
        if self.undo.len() == 32 {
            self.undo.remove(0);
        }
        self.undo.push(old);
        self.redo.clear();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn restore(&mut self, current: &GarmentTemplate, redo: bool) -> Result<Edit, String> {
        self.restore_checked(current, redo, |_| Ok(()))
    }
    /// Validate the complete candidate before consuming either history stack.
    /// Native callers can reject physically invalid restored targets atomically.
    pub fn restore_checked(
        &mut self,
        current: &GarmentTemplate,
        redo: bool,
        validate: impl FnOnce(&Edit) -> Result<(), String>,
    ) -> Result<Edit, String> {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let template = from.last().ok_or("History is empty")?;
        let garment = template.compile()?;
        let edit = Edit {
            reset_simulation: !same_physics(current, template),
            template: template.clone(),
            garment,
            selected: None,
            new_point: None,
        };
        validate(&edit)?;
        from.pop();
        to.push(current.clone());
        Ok(edit)
    }
}
