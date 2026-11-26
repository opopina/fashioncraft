//! Original vector glyph set for the atelier tool palettes.
//!
//! Every glyph here is drawn from scratch as plain stroke geometry for this
//! project. Nothing is downloaded, embedded, traced or adapted from another
//! product, icon set or image asset, so the set carries no third-party licence.
//!
//! Geometry is expressed in a normalised `0..1` square and mapped onto whatever
//! rectangle the caller passes, which keeps one glyph legible in a dense 16 px
//! palette row and crisp at 2x. Generation ([`geometry`]) is deliberately
//! separate from painting ([`paint`]) so tests can assert that every glyph is
//! non-empty and stays inside the unit square without a GPU.

// The palette is declared in full ahead of the toolbar wiring, so a few entries
// are still waiting for a call site. They are covered by tests as they land.
#![allow(dead_code)]

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Response, Shape, Stroke, StrokeKind, Vec2,
};

/// One drawing primitive in the normalised unit square.
///
/// Coordinates are `[x, y]` with `0..1` spanning the square; `y` grows downward,
/// matching egui's screen coordinates.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Prim {
    /// Straight segment from `a` to `b`.
    Line { a: [f32; 2], b: [f32; 2] },
    /// Open polyline through `points`.
    Path { points: Vec<[f32; 2]> },
    /// Stroked circle.
    Circle { center: [f32; 2], radius: f32 },
    /// Stroked rectangle.
    Rect { min: [f32; 2], max: [f32; 2] },
    /// Small filled marker: anchors, notch points, snap points.
    Dot { center: [f32; 2], radius: f32 },
}

/// Every glyph the atelier can draw.
///
/// The order is stable and each entry owns a fixed primitive count recorded in
/// the tests, so a glyph that silently loses its geometry fails a test instead
/// of reaching the user as a blank button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(super) enum Icon {
    Select,
    Edit,
    Pen,
    Rectangle,
    Sew,
    Move3d,
    Rotate3d,
    Grab,
    Undo,
    Redo,
    Duplicate,
    Mirror,
    Grid,
    Snap,
    Fit,
    ViewIso,
    ViewFront,
    ViewBack,
    ViewLeft,
    ViewRight,
    ViewTop,
    Settings,
    Theme,
    Fabric,
    Simulate,
    Step,
    Reset,
    Export,
    Search,
    Info,
    Warning,
    Seam,
    Notch,
    Gradient,
    Ellipse,
    Arc,
    Union,
    Subtract,
    Intersect,
    Measure,
    /// A framed picture: the tool that moves, scales and turns graphics.
    Graphic,
    /// A standing figure: show or hide the avatar.
    Avatar,
    /// A shirt: show or hide the garment.
    Garment,
    /// A bone: the X-ray skeleton.
    Skeleton,
    /// A shirt drawn in outline: the technical view.
    Technical,
    /// A push pin: pins that hold cloth in place.
    Pin,
    /// A four-hole button: buttons, buttonholes and other trims.
    Button,
    /// A free curve: freehand outlines.
    Curve,
    /// A pentagon: pieces drawn corner by corner.
    Polygon,
    /// A bin: delete.
    Trash,
    /// Two arrows pointing apart: give a view the whole window.
    Expand,
    /// An iron: steam that shrinks or stretches cloth.
    Iron,
    /// A snowflake: freeze pieces where they are.
    Snowflake,
    /// A zigzag: pleats.
    Pleat,
    /// Points around a centre: arrangement points.
    Points,
    /// A wave: puckering.
    Wave,
    /// A dashed line over a line: topstitches and tapes.
    Stitch,
    /// Two overlapping sheets: superimpose pieces.
    Layers,
    /// Scissors: cut a piece.
    Scissors,
    /// The corners of a frame: select part of the mesh.
    SelectArea,
    /// A window split down the middle: the 3D and 2D views side by side.
    SplitView,
    /// A pattern sheet: the 2D view alone.
    Sheet,
    /// A padlock: lock pieces or lines in 2D.
    Lock,
    /// A paintbrush: paint a selection onto the mesh.
    Brush,
    /// A lasso loop: select by drawing around.
    Lasso,
    /// A pin inside a selection frame: pin a boxed area.
    PinBox,
    /// A pin inside a lasso loop: pin a drawn area.
    PinLasso,
    /// A style line under the pointer: edit it.
    StyleEdit,
    /// A style line with a two-way arrow: slide it.
    StyleMove,
    /// A style line with a diagonal arrow: scale it.
    StyleScale,
    /// A style line ending in a pencil: draw a new one.
    StyleDraw,
    /// A pin driven into the body: pins follow the avatar.
    PinAttach,
    /// A pin lifted off the body.
    PinDetach,
    /// A sheet torn in two along a ragged edge.
    Tear,
    /// A ragged tear line with its points: edit a tear.
    TearEdit,
    /// The reset arrow around a sheet: reset the 2D arrangement.
    ResetSheet,
    /// The reset arrow around a dot: reset only the selection.
    ResetSelected,
    /// A sheet with a folded-back flap.
    Fold,
    /// Cloth dropping over a rounded body.
    Drape,
    /// Arrows closing in on a torso: fit to the body.
    FitBody,
    /// Two free curves joined by stitches.
    SewFree,
    /// Two segments on one side sewn to one on the other (M:N).
    SewChain,
    /// Two free curves sewn to one (M:N free).
    SewChainFree,
    /// A square split into four: a quad mesh.
    Quads,
    /// A square cut into triangles: a triangle mesh.
    Triangles,
    /// A fine grid with an up arrow: a denser mesh.
    MeshFiner,
    /// A coarse grid with a down arrow: a lighter mesh.
    MeshCoarser,
    /// An arrow coming down onto a sheet: lay it over.
    LayerOver,
    /// An arrow coming up under a sheet: tuck it under.
    LayerUnder,
    /// Two sheets side by side with an arrow between.
    LayerSide,
    /// A water drop: thaw (unfreeze).
    Drop,
    /// A shield: strengthen.
    Shield,
    /// A solid block seen from the front: solidify.
    Block,
    /// A pattern piece with its neck notch and grainline.
    Piece,
    /// A pattern piece struck through: deactivate.
    PieceOff,
    /// A pattern piece with a tick: activate.
    PieceOn,
    /// A pattern piece with its seam, struck through.
    PieceSeamOff,
    /// A tape measure following the body.
    Tape,
    /// A height dimension: caps and a vertical arrow.
    Height,
    /// A loop around a torso: body circumference.
    Girth,
    /// A loop with an arrow: garment circumference.
    Loop,
    /// A dimension line with end caps: a straight measure.
    Dimension,
    /// A rectangle with its width marked above: shape by size.
    ShapeSize,
    /// A figure with a raised arm: pose.
    Pose,
    /// A bust beside a slider: edit the avatar.
    AvatarEdit,
    /// Two busts: the avatar library.
    AvatarLibrary,
    /// A frame with its four handles: select all.
    SelectAll,
    /// A clipboard: paste.
    Paste,
    /// A line with a new point on it.
    AddPoint,
    /// A curve with a tangent handle: smooth a curve.
    Smooth,
    /// A line pushed out to a boundary.
    Extend,
    /// One line trimmed at a boundary, another extended to it.
    Trim,
    /// A square turning into a circle: replace the outline.
    Replace,
    /// A spiral.
    Spiral,
    /// A line inside a piece outline.
    InternalLine,
    /// A rectangle inside a piece outline.
    InternalRect,
    /// A circle inside a piece outline.
    InternalCircle,
    /// A diamond dart inside a piece outline.
    InternalDart,
    /// An outline being traced through its points.
    Trace,
    /// Two crossing guidelines.
    Guide,
    /// Two boxes lined up on one edge: smart guides.
    Align,
    /// An open padlock.
    Unlock,
    /// A piece slashed into a fan and spread.
    Spread,
    /// A pattern piece with a plus: duplicate it.
    PiecePlus,
    /// Stitches with a spark: sew automatically.
    AutoSew,
    /// A measured length with a tick: check lengths.
    LengthCheck,
    /// A band of tape laid along a seam.
    TapeBand,
    /// A roll of tape.
    TapeRoll,
    /// Topstitching on both sides of a seam.
    StitchSeam,
    /// Topstitching along a free curve.
    StitchFree,
    /// A cord laid along an edge.
    Piping,
    /// A buttonhole slot.
    Buttonhole,
    /// A button sewn on with a cross: fasten.
    Fasten,
    /// A zip with its teeth and pull.
    Zipper,
    /// A picture with corner handles: transform a graphic.
    GraphicMove,
    /// A swatch with woven waves: texture.
    Texture,
    /// One folded pleat.
    PleatSingle,
    /// Pleats held by a row of stitches.
    PleatSew,
    /// An eye: show only the selection.
    Eye,
    /// A name tag: piece names.
    Tag,
    /// Three overlapping circles: random colours.
    Colours,
    /// A block pulled apart by two arrows: stress.
    Force,
    /// Arrows pressing on a surface.
    Pressure,
    /// A ball over its soft shadow.
    Shadow,
    /// A floor plane in perspective.
    Floor,
    /// A sheet of paper with a turned corner.
    Page,
    /// A new sheet of paper.
    PagePlus,
    /// An L-shaped ruler: rulers.
    RulerCorner,
    /// A lightning bolt: quick actions.
    Bolt,
    /// A cut gem: high-quality view.
    Gem,
    /// A solid disc: plain colour.
    Swatch,
    /// A speech bubble: the Assistant.
    Chat,
    /// Two sliders: the Assistant's settings.
    Sliders,
    /// An arrow turning back: restore to an earlier moment.
    Restore,
    /// A paperclip: attach files.
    Paperclip,
    /// A filled square: stop.
    Stop,
}

#[cfg(test)]
const ALL: [Icon; 156] = [
    Icon::Select,
    Icon::Edit,
    Icon::Pen,
    Icon::Rectangle,
    Icon::Sew,
    Icon::Move3d,
    Icon::Rotate3d,
    Icon::Grab,
    Icon::Undo,
    Icon::Redo,
    Icon::Duplicate,
    Icon::Mirror,
    Icon::Grid,
    Icon::Snap,
    Icon::Fit,
    Icon::ViewIso,
    Icon::ViewFront,
    Icon::ViewBack,
    Icon::ViewLeft,
    Icon::ViewRight,
    Icon::ViewTop,
    Icon::Settings,
    Icon::Theme,
    Icon::Fabric,
    Icon::Simulate,
    Icon::Step,
    Icon::Reset,
    Icon::Export,
    Icon::Search,
    Icon::Info,
    Icon::Warning,
    Icon::Seam,
    Icon::Notch,
    Icon::Gradient,
    Icon::Ellipse,
    Icon::Arc,
    Icon::Union,
    Icon::Subtract,
    Icon::Intersect,
    Icon::Measure,
    Icon::Graphic,
    Icon::Avatar,
    Icon::Garment,
    Icon::Skeleton,
    Icon::Technical,
    Icon::Pin,
    Icon::Button,
    Icon::Curve,
    Icon::Polygon,
    Icon::Trash,
    Icon::Expand,
    Icon::Iron,
    Icon::Snowflake,
    Icon::Pleat,
    Icon::Points,
    Icon::Wave,
    Icon::Stitch,
    Icon::Layers,
    Icon::Scissors,
    Icon::SelectArea,
    Icon::SplitView,
    Icon::Sheet,
    Icon::Lock,
    Icon::Brush,
    Icon::Lasso,
    Icon::PinBox,
    Icon::PinLasso,
    Icon::StyleEdit,
    Icon::StyleMove,
    Icon::StyleScale,
    Icon::StyleDraw,
    Icon::PinAttach,
    Icon::PinDetach,
    Icon::Tear,
    Icon::TearEdit,
    Icon::ResetSheet,
    Icon::ResetSelected,
    Icon::Fold,
    Icon::Drape,
    Icon::FitBody,
    Icon::SewFree,
    Icon::SewChain,
    Icon::SewChainFree,
    Icon::Quads,
    Icon::Triangles,
    Icon::MeshFiner,
    Icon::MeshCoarser,
    Icon::LayerOver,
    Icon::LayerUnder,
    Icon::LayerSide,
    Icon::Drop,
    Icon::Shield,
    Icon::Block,
    Icon::Piece,
    Icon::PieceOff,
    Icon::PieceOn,
    Icon::PieceSeamOff,
    Icon::Tape,
    Icon::Height,
    Icon::Girth,
    Icon::Loop,
    Icon::Dimension,
    Icon::ShapeSize,
    Icon::Pose,
    Icon::AvatarEdit,
    Icon::AvatarLibrary,
    Icon::SelectAll,
    Icon::Paste,
    Icon::AddPoint,
    Icon::Smooth,
    Icon::Extend,
    Icon::Trim,
    Icon::Replace,
    Icon::Spiral,
    Icon::InternalLine,
    Icon::InternalRect,
    Icon::InternalCircle,
    Icon::InternalDart,
    Icon::Trace,
    Icon::Guide,
    Icon::Align,
    Icon::Unlock,
    Icon::Spread,
    Icon::PiecePlus,
    Icon::AutoSew,
    Icon::LengthCheck,
    Icon::TapeBand,
    Icon::TapeRoll,
    Icon::StitchSeam,
    Icon::StitchFree,
    Icon::Piping,
    Icon::Buttonhole,
    Icon::Fasten,
    Icon::Zipper,
    Icon::GraphicMove,
    Icon::Texture,
    Icon::PleatSingle,
    Icon::PleatSew,
    Icon::Eye,
    Icon::Tag,
    Icon::Colours,
    Icon::Force,
    Icon::Pressure,
    Icon::Shadow,
    Icon::Floor,
    Icon::Page,
    Icon::PagePlus,
    Icon::RulerCorner,
    Icon::Bolt,
    Icon::Gem,
    Icon::Swatch,
    Icon::Chat,
    Icon::Sliders,
    Icon::Restore,
    Icon::Paperclip,
    Icon::Stop,
];

// ---------------------------------------------------------------------------
// Geometry construction
// ---------------------------------------------------------------------------

fn seg(a: [f32; 2], b: [f32; 2]) -> Prim {
    Prim::Line { a, b }
}

fn path(points: &[[f32; 2]]) -> Prim {
    Prim::Path {
        points: points.to_vec(),
    }
}

/// Closed outline: repeats the first point so a stroked path returns to its start.
fn closed(points: &[[f32; 2]]) -> Prim {
    let mut points = points.to_vec();
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    Prim::Path { points }
}

fn circle(center: [f32; 2], radius: f32) -> Prim {
    Prim::Circle { center, radius }
}

fn stroke_rect(min: [f32; 2], max: [f32; 2]) -> Prim {
    Prim::Rect { min, max }
}

fn dot(center: [f32; 2], radius: f32) -> Prim {
    Prim::Dot { center, radius }
}

/// Sample a circular arc. Angles are in degrees, `y` grows downward.
fn arc(center: [f32; 2], radius: f32, start_deg: f32, end_deg: f32, segments: usize) -> Prim {
    let segments = segments.max(2);
    let mut points = Vec::with_capacity(segments + 1);
    for step in 0..=segments {
        let t = step as f32 / segments as f32;
        let angle = (start_deg + (end_deg - start_deg) * t).to_radians();
        points.push([
            center[0] + radius * angle.cos(),
            center[1] + radius * angle.sin(),
        ]);
    }
    Prim::Path { points }
}

/// Sample an elliptical arc. Angles are in degrees, `y` grows downward.
fn ellipse(
    center: [f32; 2],
    radius_x: f32,
    radius_y: f32,
    start_deg: f32,
    end_deg: f32,
    segments: usize,
) -> Prim {
    let segments = segments.max(2);
    let points = (0..=segments)
        .map(|step| {
            let t = step as f32 / segments as f32;
            let angle = (start_deg + (end_deg - start_deg) * t).to_radians();
            [
                center[0] + radius_x * angle.cos(),
                center[1] + radius_y * angle.sin(),
            ]
        })
        .collect();
    Prim::Path { points }
}

/// Sample an Archimedean spiral growing from `inner` to `outer` radius over `turns` turns.
fn spiral(center: [f32; 2], inner: f32, outer: f32, turns: f32, segments: usize) -> Prim {
    let segments = segments.max(2);
    let points = (0..=segments)
        .map(|step| {
            let t = step as f32 / segments as f32;
            let angle = t * turns * std::f32::consts::TAU;
            let radius = inner + (outer - inner) * t;
            [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]
        })
        .collect();
    Prim::Path { points }
}

/// Sample a cubic Bezier curve.
fn cubic(p0: [f32; 2], c0: [f32; 2], c1: [f32; 2], p1: [f32; 2], segments: usize) -> Prim {
    let segments = segments.max(2);
    let mut points = Vec::with_capacity(segments + 1);
    for step in 0..=segments {
        let t = step as f32 / segments as f32;
        let u = 1.0 - t;
        points.push([
            u * u * u * p0[0]
                + 3.0 * u * u * t * c0[0]
                + 3.0 * u * t * t * c1[0]
                + t * t * t * p1[0],
            u * u * u * p0[1]
                + 3.0 * u * u * t * c0[1]
                + 3.0 * u * t * t * c1[1]
                + t * t * t * p1[1],
        ]);
    }
    Prim::Path { points }
}

/// Two-barb chevron whose vertex sits on `tip` and which opens away from `toward`.
///
/// `toward` is the direction the arrow points; it is normalised here so callers
/// can pass arc tangents directly.
fn arrow_head(tip: [f32; 2], toward: [f32; 2], length: f32, half_angle_deg: f32) -> Prim {
    let magnitude = (toward[0] * toward[0] + toward[1] * toward[1]).sqrt();
    let (dx, dy) = if magnitude > f32::EPSILON {
        (toward[0] / magnitude, toward[1] / magnitude)
    } else {
        (0.0, -1.0)
    };
    let back = (-dy).atan2(-dx);
    let half = half_angle_deg.to_radians();
    Prim::Path {
        points: vec![
            [
                tip[0] + length * (back + half).cos(),
                tip[1] + length * (back + half).sin(),
            ],
            tip,
            [
                tip[0] + length * (back - half).cos(),
                tip[1] + length * (back - half).sin(),
            ],
        ],
    }
}

/// The full primitive list for one icon, in the normalised unit square.
///
/// Pure data: no painter, no GPU and no fonts, which is what makes the glyphs
/// testable. Coordinates stay inside `0..1` including stroke extents.
pub(super) fn geometry(icon: Icon) -> Vec<Prim> {
    match icon {
        Icon::Measure => vec![
            stroke_rect([0.10, 0.30], [0.90, 0.70]),
            seg([0.25, 0.30], [0.25, 0.50]),
            seg([0.40, 0.30], [0.40, 0.58]),
            seg([0.55, 0.30], [0.55, 0.50]),
            seg([0.70, 0.30], [0.70, 0.58]),
        ],
        Icon::Graphic => vec![
            stroke_rect([0.12, 0.18], [0.88, 0.82]),
            path(&[
                [0.18, 0.74],
                [0.42, 0.46],
                [0.58, 0.62],
                [0.68, 0.52],
                [0.82, 0.74],
            ]),
            circle([0.66, 0.34], 0.07),
        ],
        Icon::Ellipse => vec![circle([0.5, 0.5], 0.34)],
        Icon::Arc => vec![
            arc([0.5, 0.5], 0.36, -145., 80., 20),
            path(&[[0.205, 0.293], [0.5, 0.5], [0.563, 0.855]]),
        ],
        Icon::Union => vec![closed(&[
            [0.12, 0.15],
            [0.58, 0.15],
            [0.58, 0.42],
            [0.88, 0.42],
            [0.88, 0.88],
            [0.42, 0.88],
            [0.42, 0.58],
            [0.12, 0.58],
        ])],
        Icon::Subtract => vec![
            closed(&[
                [0.12, 0.15],
                [0.63, 0.15],
                [0.63, 0.4],
                [0.4, 0.4],
                [0.4, 0.7],
                [0.12, 0.7],
            ]),
            path(&[
                [0.7, 0.43],
                [0.88, 0.43],
                [0.88, 0.88],
                [0.43, 0.88],
                [0.43, 0.77],
            ]),
        ],
        Icon::Intersect => vec![
            stroke_rect([0.12, 0.15], [0.64, 0.65]),
            stroke_rect([0.4, 0.4], [0.88, 0.88]),
            seg([0.42, 0.62], [0.62, 0.42]),
            seg([0.42, 0.51], [0.51, 0.42]),
        ],
        // Pointer arrow.
        Icon::Select => vec![closed(&[
            [0.30, 0.10],
            [0.30, 0.80],
            [0.44, 0.66],
            [0.55, 0.90],
            [0.67, 0.85],
            [0.56, 0.62],
            [0.74, 0.62],
        ])],
        // Pencil body, ferrule and sharpened edge.
        Icon::Edit => vec![
            closed(&[[0.30, 0.74], [0.62, 0.16], [0.80, 0.30], [0.48, 0.88]]),
            seg([0.56, 0.24], [0.74, 0.38]),
            seg([0.38, 0.72], [0.56, 0.86]),
        ],
        // Bezier pen: one curve between two editable anchors.
        Icon::Pen => vec![
            cubic([0.18, 0.80], [0.30, 0.18], [0.70, 0.82], [0.84, 0.22], 20),
            dot([0.18, 0.80], 0.07),
            dot([0.84, 0.22], 0.07),
        ],
        // Rectangle outline with a draggable corner handle.
        Icon::Rectangle => vec![
            stroke_rect([0.14, 0.20], [0.86, 0.80]),
            dot([0.86, 0.80], 0.055),
        ],
        // Two panel edges joined by a locking stitch.
        Icon::Sew => vec![
            seg([0.26, 0.14], [0.26, 0.86]),
            seg([0.74, 0.14], [0.74, 0.86]),
            seg([0.26, 0.30], [0.74, 0.42]),
            seg([0.74, 0.46], [0.26, 0.58]),
            seg([0.26, 0.62], [0.74, 0.74]),
        ],
        // Four-way translate handle with a centred origin.
        Icon::Move3d => vec![
            seg([0.14, 0.50], [0.86, 0.50]),
            arrow_head([0.10, 0.50], [-1.0, 0.0], 0.16, 30.0),
            arrow_head([0.90, 0.50], [1.0, 0.0], 0.16, 30.0),
            seg([0.50, 0.14], [0.50, 0.86]),
            arrow_head([0.50, 0.10], [0.0, -1.0], 0.16, 30.0),
            arrow_head([0.50, 0.90], [0.0, 1.0], 0.16, 30.0),
            dot([0.50, 0.50], 0.06),
        ],
        // Rotation ring around a vertical axis.
        Icon::Rotate3d => vec![
            arc([0.50, 0.50], 0.30, 140.0, 420.0, 28),
            arrow_head([0.65, 0.76], [-0.866, 0.5], 0.16, 30.0),
            seg([0.50, 0.14], [0.50, 0.86]),
        ],
        // Open hand grabbing fabric.
        Icon::Grab => vec![closed(&[
            [0.74, 0.82],
            [0.74, 0.48],
            [0.68, 0.42],
            [0.60, 0.42],
            [0.60, 0.32],
            [0.52, 0.32],
            [0.52, 0.26],
            [0.44, 0.26],
            [0.44, 0.30],
            [0.36, 0.30],
            [0.36, 0.42],
            [0.28, 0.42],
            [0.28, 0.60],
            [0.16, 0.66],
            [0.30, 0.82],
        ])],
        // Arrow curving back over the top with its head at the left.
        Icon::Undo => vec![
            arc([0.50, 0.58], 0.30, 180.0, 360.0, 20),
            arrow_head([0.20, 0.58], [0.0, 1.0], 0.16, 30.0),
        ],
        // Mirror of the undo arrow, head at the right.
        Icon::Redo => vec![
            arc([0.50, 0.58], 0.30, 180.0, 360.0, 20),
            arrow_head([0.80, 0.58], [0.0, 1.0], 0.16, 30.0),
        ],
        // One piece offset behind another.
        Icon::Duplicate => vec![
            stroke_rect([0.14, 0.14], [0.62, 0.62]),
            stroke_rect([0.38, 0.38], [0.86, 0.86]),
        ],
        // A shape and its reflection across a symmetry axis.
        Icon::Mirror => vec![
            seg([0.50, 0.12], [0.50, 0.88]),
            closed(&[[0.40, 0.28], [0.40, 0.72], [0.14, 0.50]]),
            closed(&[[0.60, 0.28], [0.60, 0.72], [0.86, 0.50]]),
        ],
        // Three-by-three draft grid.
        Icon::Grid => vec![
            stroke_rect([0.14, 0.14], [0.86, 0.86]),
            seg([0.38, 0.14], [0.38, 0.86]),
            seg([0.62, 0.14], [0.62, 0.86]),
            seg([0.14, 0.38], [0.86, 0.38]),
            seg([0.14, 0.62], [0.86, 0.62]),
        ],
        // Magnet: snaps markers onto a point.
        Icon::Snap => vec![
            seg([0.26, 0.74], [0.26, 0.44]),
            seg([0.74, 0.74], [0.74, 0.44]),
            arc([0.50, 0.44], 0.24, 180.0, 360.0, 16),
            seg([0.16, 0.74], [0.36, 0.74]),
            seg([0.64, 0.74], [0.84, 0.74]),
        ],
        // Corner brackets closing on the content, plus a centre mark.
        Icon::Fit => vec![
            path(&[[0.16, 0.32], [0.16, 0.16], [0.32, 0.16]]),
            path(&[[0.68, 0.16], [0.84, 0.16], [0.84, 0.32]]),
            path(&[[0.84, 0.68], [0.84, 0.84], [0.68, 0.84]]),
            path(&[[0.32, 0.84], [0.16, 0.84], [0.16, 0.68]]),
            dot([0.50, 0.50], 0.055),
        ],
        // Isometric box.
        Icon::ViewIso => vec![
            closed(&[[0.50, 0.12], [0.86, 0.32], [0.50, 0.52], [0.14, 0.32]]),
            closed(&[[0.14, 0.32], [0.50, 0.52], [0.50, 0.88], [0.14, 0.68]]),
            closed(&[[0.86, 0.32], [0.50, 0.52], [0.50, 0.88], [0.86, 0.68]]),
        ],
        // Face-on elevation with its centre mark.
        Icon::ViewFront => vec![
            stroke_rect([0.22, 0.20], [0.78, 0.80]),
            dot([0.50, 0.50], 0.07),
        ],
        // Same elevation seen from behind: the near edge becomes a hidden line.
        Icon::ViewBack => vec![
            stroke_rect([0.22, 0.20], [0.78, 0.80]),
            seg([0.22, 0.50], [0.42, 0.50]),
            seg([0.58, 0.50], [0.78, 0.50]),
        ],
        // Side elevation with the view direction leaving it.
        Icon::ViewLeft => vec![
            stroke_rect([0.62, 0.18], [0.86, 0.82]),
            seg([0.50, 0.50], [0.18, 0.50]),
            arrow_head([0.16, 0.50], [-1.0, 0.0], 0.16, 30.0),
        ],
        Icon::ViewRight => vec![
            stroke_rect([0.14, 0.18], [0.38, 0.82]),
            seg([0.50, 0.50], [0.82, 0.50]),
            arrow_head([0.84, 0.50], [1.0, 0.0], 0.16, 30.0),
        ],
        // Plan view: a flattened plate with the direction looking down onto it.
        Icon::ViewTop => vec![
            stroke_rect([0.18, 0.38], [0.82, 0.64]),
            seg([0.50, 0.14], [0.50, 0.34]),
            arrow_head([0.50, 0.36], [0.0, 1.0], 0.16, 30.0),
        ],
        // Gear: hub ring with six teeth.
        Icon::Settings => {
            let mut prims = vec![circle([0.50, 0.50], 0.22)];
            for index in 0..6 {
                let angle = index as f32 * 60.0_f32.to_radians();
                prims.push(seg(
                    [0.50 + 0.28 * angle.cos(), 0.50 + 0.28 * angle.sin()],
                    [0.50 + 0.42 * angle.cos(), 0.50 + 0.42 * angle.sin()],
                ));
            }
            prims
        }
        // Half-shaded disc: light and dark workspace themes.
        Icon::Theme => vec![
            circle([0.50, 0.50], 0.30),
            seg([0.50, 0.20], [0.50, 0.80]),
            seg([0.56, 0.36], [0.76, 0.36]),
            seg([0.56, 0.50], [0.78, 0.50]),
            seg([0.56, 0.64], [0.76, 0.64]),
        ],
        // Draped panel with two fold lines.
        Icon::Fabric => vec![
            closed(&[
                [0.14, 0.30],
                [0.32, 0.22],
                [0.50, 0.30],
                [0.68, 0.22],
                [0.86, 0.30],
                [0.86, 0.70],
                [0.68, 0.78],
                [0.50, 0.70],
                [0.32, 0.78],
                [0.14, 0.70],
            ]),
            seg([0.34, 0.27], [0.34, 0.73]),
            seg([0.66, 0.27], [0.66, 0.73]),
        ],
        // Play triangle.
        Icon::Simulate => vec![closed(&[[0.30, 0.18], [0.30, 0.82], [0.84, 0.50]])],
        // Single solver step.
        Icon::Step => vec![
            closed(&[[0.22, 0.20], [0.22, 0.80], [0.64, 0.50]]),
            seg([0.76, 0.20], [0.76, 0.80]),
        ],
        // Circular arrow returning to the start.
        Icon::Reset => vec![
            arc([0.50, 0.50], 0.30, 20.0, 320.0, 26),
            arrow_head([0.73, 0.31], [0.643, 0.766], 0.16, 30.0),
        ],
        // Tray with the payload leaving it.
        Icon::Export => vec![
            path(&[[0.18, 0.60], [0.18, 0.84], [0.82, 0.84], [0.82, 0.60]]),
            seg([0.50, 0.66], [0.50, 0.24]),
            arrow_head([0.50, 0.16], [0.0, -1.0], 0.16, 30.0),
        ],
        // Magnifier.
        Icon::Search => vec![circle([0.44, 0.42], 0.24), seg([0.61, 0.59], [0.84, 0.84])],
        // Information marker.
        Icon::Info => vec![
            circle([0.50, 0.50], 0.34),
            dot([0.50, 0.31], 0.05),
            seg([0.50, 0.44], [0.50, 0.70]),
        ],
        // Warning triangle.
        Icon::Warning => vec![
            closed(&[[0.50, 0.12], [0.90, 0.86], [0.10, 0.86]]),
            seg([0.50, 0.42], [0.50, 0.62]),
            dot([0.50, 0.72], 0.05),
        ],
        // Seam in section: two panels meeting, with stitch ticks.
        Icon::Seam => vec![
            path(&[[0.12, 0.30], [0.50, 0.66], [0.88, 0.30]]),
            seg([0.31, 0.46], [0.22, 0.62]),
            seg([0.50, 0.66], [0.50, 0.82]),
            seg([0.69, 0.46], [0.78, 0.62]),
        ],
        // Notch cut into a panel edge.
        Icon::Notch => vec![
            seg([0.12, 0.36], [0.38, 0.36]),
            seg([0.62, 0.36], [0.88, 0.36]),
            path(&[[0.38, 0.36], [0.50, 0.64], [0.62, 0.36]]),
        ],
        // Rising tonal ramp.
        Icon::Gradient => vec![
            seg([0.24, 0.34], [0.24, 0.66]),
            seg([0.41, 0.27], [0.41, 0.73]),
            seg([0.59, 0.20], [0.59, 0.80]),
            seg([0.76, 0.13], [0.76, 0.87]),
        ],
        Icon::Avatar => vec![
            circle([0.50, 0.19], 0.10),
            closed(&[[0.36, 0.34], [0.64, 0.34], [0.60, 0.60], [0.40, 0.60]]),
            seg([0.36, 0.36], [0.26, 0.60]),
            seg([0.64, 0.36], [0.74, 0.60]),
            seg([0.44, 0.60], [0.41, 0.88]),
            seg([0.56, 0.60], [0.59, 0.88]),
        ],
        Icon::Garment => vec![closed(&[
            [0.36, 0.18],
            [0.44, 0.25],
            [0.56, 0.25],
            [0.64, 0.18],
            [0.86, 0.32],
            [0.78, 0.47],
            [0.70, 0.42],
            [0.70, 0.84],
            [0.30, 0.84],
            [0.30, 0.42],
            [0.22, 0.47],
            [0.14, 0.32],
        ])],
        Icon::Skeleton => vec![
            seg([0.32, 0.68], [0.68, 0.32]),
            circle([0.27, 0.67], 0.07),
            circle([0.33, 0.73], 0.07),
            circle([0.67, 0.27], 0.07),
            circle([0.73, 0.33], 0.07),
        ],
        Icon::Technical => vec![
            closed(&[
                [0.36, 0.18],
                [0.44, 0.25],
                [0.56, 0.25],
                [0.64, 0.18],
                [0.86, 0.32],
                [0.78, 0.47],
                [0.70, 0.42],
                [0.70, 0.84],
                [0.30, 0.84],
                [0.30, 0.42],
                [0.22, 0.47],
                [0.14, 0.32],
            ]),
            dot([0.50, 0.40], 0.025),
            dot([0.50, 0.52], 0.025),
            dot([0.50, 0.64], 0.025),
            dot([0.50, 0.76], 0.025),
        ],
        Icon::Pin => vec![
            closed(&[
                [0.36, 0.12],
                [0.64, 0.12],
                [0.59, 0.38],
                [0.72, 0.54],
                [0.28, 0.54],
                [0.41, 0.38],
            ]),
            seg([0.50, 0.54], [0.50, 0.90]),
        ],
        Icon::Button => vec![
            circle([0.50, 0.50], 0.34),
            dot([0.42, 0.42], 0.045),
            dot([0.58, 0.42], 0.045),
            dot([0.42, 0.58], 0.045),
            dot([0.58, 0.58], 0.045),
        ],
        Icon::Curve => vec![cubic(
            [0.12, 0.78],
            [0.30, 0.08],
            [0.70, 0.92],
            [0.88, 0.22],
            18,
        )],
        Icon::Iron => vec![
            closed(&[[0.12, 0.70], [0.30, 0.38], [0.80, 0.38], [0.88, 0.70]]),
            path(&[[0.42, 0.38], [0.46, 0.20], [0.78, 0.20], [0.78, 0.38]]),
            dot([0.32, 0.84], 0.035),
            dot([0.50, 0.84], 0.035),
            dot([0.68, 0.84], 0.035),
        ],
        Icon::Snowflake => vec![
            seg([0.50, 0.12], [0.50, 0.88]),
            seg([0.17, 0.31], [0.83, 0.69]),
            seg([0.17, 0.69], [0.83, 0.31]),
        ],
        Icon::Pleat => vec![
            path(&[
                [0.12, 0.28],
                [0.30, 0.72],
                [0.48, 0.28],
                [0.66, 0.72],
                [0.84, 0.28],
            ]),
            seg([0.12, 0.86], [0.88, 0.86]),
        ],
        Icon::Points => vec![
            dot([0.24, 0.28], 0.06),
            dot([0.76, 0.28], 0.06),
            dot([0.24, 0.72], 0.06),
            dot([0.76, 0.72], 0.06),
            circle([0.50, 0.50], 0.10),
        ],
        Icon::Wave => vec![
            cubic([0.10, 0.50], [0.30, 0.18], [0.40, 0.82], [0.50, 0.50], 10),
            cubic([0.50, 0.50], [0.60, 0.18], [0.70, 0.82], [0.90, 0.50], 10),
        ],
        Icon::Stitch => vec![
            seg([0.10, 0.36], [0.24, 0.36]),
            seg([0.38, 0.36], [0.52, 0.36]),
            seg([0.66, 0.36], [0.80, 0.36]),
            seg([0.10, 0.66], [0.90, 0.66]),
        ],
        Icon::Layers => vec![
            stroke_rect([0.14, 0.14], [0.62, 0.62]),
            stroke_rect([0.38, 0.38], [0.86, 0.86]),
        ],
        Icon::Scissors => vec![
            circle([0.28, 0.74], 0.11),
            circle([0.72, 0.74], 0.11),
            seg([0.35, 0.65], [0.76, 0.14]),
            seg([0.65, 0.65], [0.24, 0.14]),
        ],
        Icon::SelectArea => vec![
            path(&[[0.12, 0.34], [0.12, 0.12], [0.34, 0.12]]),
            path(&[[0.66, 0.12], [0.88, 0.12], [0.88, 0.34]]),
            path(&[[0.88, 0.66], [0.88, 0.88], [0.66, 0.88]]),
            path(&[[0.34, 0.88], [0.12, 0.88], [0.12, 0.66]]),
        ],
        Icon::SplitView => vec![
            stroke_rect([0.12, 0.20], [0.88, 0.80]),
            seg([0.50, 0.20], [0.50, 0.80]),
        ],
        Icon::Brush => vec![
            seg([0.86, 0.14], [0.58, 0.42]),
            closed(&[[0.52, 0.38], [0.62, 0.48], [0.54, 0.56], [0.44, 0.46]]),
            cubic([0.44, 0.46], [0.26, 0.50], [0.20, 0.70], [0.14, 0.86], 10),
            cubic([0.14, 0.86], [0.32, 0.84], [0.50, 0.76], [0.54, 0.56], 10),
        ],
        Icon::Lasso => vec![
            cubic([0.36, 0.64], [0.06, 0.58], [0.08, 0.14], [0.50, 0.14], 14),
            cubic([0.50, 0.14], [0.94, 0.14], [0.94, 0.62], [0.36, 0.64], 14),
            cubic([0.36, 0.64], [0.28, 0.72], [0.42, 0.80], [0.30, 0.92], 8),
        ],
        Icon::PinBox => vec![
            path(&[[0.10, 0.28], [0.10, 0.10], [0.28, 0.10]]),
            path(&[[0.72, 0.10], [0.90, 0.10], [0.90, 0.28]]),
            path(&[[0.90, 0.72], [0.90, 0.90], [0.72, 0.90]]),
            path(&[[0.28, 0.90], [0.10, 0.90], [0.10, 0.72]]),
            closed(&[
                [0.41, 0.24],
                [0.59, 0.24],
                [0.56, 0.40],
                [0.65, 0.50],
                [0.35, 0.50],
                [0.44, 0.40],
            ]),
            seg([0.50, 0.50], [0.50, 0.76]),
        ],
        Icon::PinLasso => vec![
            cubic([0.34, 0.84], [0.04, 0.72], [0.06, 0.10], [0.50, 0.10], 14),
            cubic([0.50, 0.10], [0.96, 0.10], [0.96, 0.80], [0.46, 0.84], 14),
            cubic([0.34, 0.84], [0.30, 0.90], [0.36, 0.94], [0.28, 0.96], 6),
            closed(&[
                [0.43, 0.26],
                [0.57, 0.26],
                [0.55, 0.40],
                [0.63, 0.48],
                [0.37, 0.48],
                [0.45, 0.40],
            ]),
            seg([0.50, 0.48], [0.50, 0.68]),
        ],
        Icon::StyleEdit => vec![
            cubic([0.10, 0.88], [0.18, 0.46], [0.40, 0.82], [0.56, 0.42], 18),
            closed(&[
                [0.58, 0.10],
                [0.58, 0.52],
                [0.68, 0.43],
                [0.75, 0.58],
                [0.81, 0.55],
                [0.74, 0.41],
                [0.88, 0.41],
            ]),
        ],
        Icon::StyleMove => vec![
            cubic([0.10, 0.88], [0.18, 0.46], [0.40, 0.82], [0.56, 0.42], 18),
            seg([0.50, 0.16], [0.92, 0.16]),
            arrow_head([0.48, 0.16], [-1.0, 0.0], 0.13, 35.0),
            arrow_head([0.94, 0.16], [1.0, 0.0], 0.13, 35.0),
        ],
        Icon::StyleScale => vec![
            cubic([0.10, 0.88], [0.18, 0.46], [0.40, 0.82], [0.56, 0.42], 18),
            seg([0.66, 0.32], [0.90, 0.08]),
            arrow_head([0.92, 0.06], [1.0, -1.0], 0.13, 35.0),
            arrow_head([0.64, 0.34], [-1.0, 1.0], 0.13, 35.0),
        ],
        Icon::StyleDraw => vec![
            cubic([0.10, 0.88], [0.18, 0.46], [0.40, 0.82], [0.56, 0.42], 18),
            closed(&[
                [0.56, 0.42],
                [0.687, 0.377],
                [0.927, 0.137],
                [0.843, 0.053],
                [0.603, 0.293],
            ]),
            seg([0.603, 0.293], [0.687, 0.377]),
        ],
        Icon::PinAttach => vec![
            closed(&[
                [0.36, 0.08],
                [0.64, 0.08],
                [0.59, 0.30],
                [0.72, 0.44],
                [0.28, 0.44],
                [0.41, 0.30],
            ]),
            seg([0.50, 0.44], [0.50, 0.80]),
            seg([0.10, 0.72], [0.90, 0.72]),
        ],
        Icon::PinDetach => vec![
            closed(&[
                [0.28, 0.06],
                [0.52, 0.06],
                [0.48, 0.24],
                [0.58, 0.36],
                [0.22, 0.36],
                [0.32, 0.24],
            ]),
            seg([0.40, 0.36], [0.40, 0.58]),
            seg([0.10, 0.80], [0.90, 0.80]),
            seg([0.80, 0.62], [0.80, 0.30]),
            arrow_head([0.80, 0.26], [0.0, -1.0], 0.13, 35.0),
        ],
        Icon::Tear => vec![
            closed(&[
                [0.08, 0.16],
                [0.46, 0.16],
                [0.38, 0.34],
                [0.48, 0.52],
                [0.38, 0.70],
                [0.44, 0.86],
                [0.08, 0.86],
            ]),
            closed(&[
                [0.58, 0.16],
                [0.92, 0.16],
                [0.92, 0.86],
                [0.56, 0.86],
                [0.50, 0.70],
                [0.60, 0.52],
                [0.50, 0.34],
            ]),
        ],
        Icon::TearEdit => vec![
            path(&[
                [0.50, 0.08],
                [0.38, 0.30],
                [0.58, 0.50],
                [0.40, 0.70],
                [0.52, 0.92],
            ]),
            dot([0.38, 0.30], 0.055),
            dot([0.58, 0.50], 0.055),
            dot([0.40, 0.70], 0.055),
            seg([0.16, 0.10], [0.16, 0.90]),
            seg([0.84, 0.10], [0.84, 0.90]),
        ],
        Icon::ResetSheet => vec![
            arc([0.50, 0.50], 0.30, 20.0, 320.0, 26),
            arrow_head([0.73, 0.31], [0.643, 0.766], 0.16, 30.0),
            stroke_rect([0.38, 0.40], [0.62, 0.60]),
        ],
        Icon::ResetSelected => vec![
            arc([0.50, 0.50], 0.30, 20.0, 320.0, 26),
            arrow_head([0.73, 0.31], [0.643, 0.766], 0.16, 30.0),
            dot([0.50, 0.50], 0.08),
        ],
        Icon::Fold => vec![
            closed(&[[0.12, 0.18], [0.54, 0.18], [0.54, 0.82], [0.12, 0.82]]),
            closed(&[[0.54, 0.18], [0.86, 0.32], [0.86, 0.94], [0.54, 0.82]]),
        ],
        Icon::Drape => vec![
            arc([0.50, 0.66], 0.22, 180.0, 360.0, 16),
            path(&[
                [0.10, 0.90],
                [0.16, 0.62],
                [0.30, 0.40],
                [0.50, 0.34],
                [0.70, 0.40],
                [0.84, 0.62],
                [0.90, 0.90],
            ]),
            seg([0.50, 0.06], [0.50, 0.20]),
            arrow_head([0.50, 0.24], [0.0, 1.0], 0.13, 35.0),
        ],
        Icon::FitBody => vec![
            closed(&[[0.34, 0.14], [0.66, 0.14], [0.62, 0.86], [0.38, 0.86]]),
            seg([0.04, 0.50], [0.24, 0.50]),
            arrow_head([0.26, 0.50], [1.0, 0.0], 0.13, 35.0),
            seg([0.96, 0.50], [0.76, 0.50]),
            arrow_head([0.74, 0.50], [-1.0, 0.0], 0.13, 35.0),
        ],
        Icon::SewFree => vec![
            cubic([0.24, 0.12], [0.10, 0.40], [0.38, 0.60], [0.22, 0.88], 14),
            cubic([0.76, 0.12], [0.62, 0.40], [0.90, 0.60], [0.74, 0.88], 14),
            seg([0.25, 0.34], [0.72, 0.40]),
            seg([0.72, 0.62], [0.26, 0.68]),
        ],
        Icon::SewChain => vec![
            seg([0.24, 0.10], [0.24, 0.44]),
            seg([0.24, 0.56], [0.24, 0.90]),
            seg([0.76, 0.10], [0.76, 0.90]),
            seg([0.24, 0.27], [0.76, 0.36]),
            seg([0.24, 0.73], [0.76, 0.64]),
        ],
        Icon::SewChainFree => vec![
            cubic([0.24, 0.10], [0.14, 0.22], [0.32, 0.34], [0.22, 0.44], 8),
            cubic([0.22, 0.56], [0.14, 0.66], [0.32, 0.78], [0.24, 0.90], 8),
            cubic([0.76, 0.10], [0.64, 0.40], [0.90, 0.60], [0.76, 0.90], 14),
            seg([0.26, 0.27], [0.72, 0.34]),
            seg([0.26, 0.73], [0.74, 0.66]),
        ],
        Icon::Quads => vec![
            stroke_rect([0.12, 0.12], [0.88, 0.88]),
            seg([0.50, 0.12], [0.50, 0.88]),
            seg([0.12, 0.50], [0.88, 0.50]),
        ],
        Icon::Triangles => vec![
            stroke_rect([0.12, 0.12], [0.88, 0.88]),
            seg([0.50, 0.12], [0.50, 0.88]),
            seg([0.12, 0.50], [0.88, 0.50]),
            seg([0.12, 0.50], [0.50, 0.12]),
            seg([0.12, 0.88], [0.88, 0.12]),
            seg([0.50, 0.88], [0.88, 0.50]),
        ],
        Icon::MeshFiner => vec![
            stroke_rect([0.08, 0.30], [0.70, 0.92]),
            seg([0.287, 0.30], [0.287, 0.92]),
            seg([0.493, 0.30], [0.493, 0.92]),
            seg([0.08, 0.507], [0.70, 0.507]),
            seg([0.08, 0.713], [0.70, 0.713]),
            seg([0.86, 0.42], [0.86, 0.10]),
            arrow_head([0.86, 0.06], [0.0, -1.0], 0.13, 35.0),
        ],
        Icon::MeshCoarser => vec![
            stroke_rect([0.08, 0.30], [0.70, 0.92]),
            seg([0.39, 0.30], [0.39, 0.92]),
            seg([0.08, 0.61], [0.70, 0.61]),
            seg([0.86, 0.06], [0.86, 0.38]),
            arrow_head([0.86, 0.42], [0.0, 1.0], 0.13, 35.0),
        ],
        Icon::LayerOver => vec![
            stroke_rect([0.10, 0.58], [0.90, 0.90]),
            seg([0.50, 0.08], [0.50, 0.42]),
            arrow_head([0.50, 0.46], [0.0, 1.0], 0.13, 35.0),
        ],
        Icon::LayerUnder => vec![
            stroke_rect([0.10, 0.10], [0.90, 0.42]),
            seg([0.50, 0.92], [0.50, 0.58]),
            arrow_head([0.50, 0.54], [0.0, -1.0], 0.13, 35.0),
        ],
        Icon::LayerSide => vec![
            stroke_rect([0.06, 0.24], [0.40, 0.76]),
            stroke_rect([0.60, 0.24], [0.94, 0.76]),
            seg([0.44, 0.50], [0.54, 0.50]),
            arrow_head([0.56, 0.50], [1.0, 0.0], 0.10, 35.0),
        ],
        Icon::Drop => vec![
            arc([0.50, 0.62], 0.24, -30.0, 210.0, 18),
            seg([0.708, 0.50], [0.50, 0.12]),
            seg([0.292, 0.50], [0.50, 0.12]),
        ],
        Icon::Shield => vec![
            closed(&[
                [0.50, 0.08],
                [0.84, 0.20],
                [0.80, 0.56],
                [0.50, 0.90],
                [0.20, 0.56],
                [0.16, 0.20],
            ]),
            seg([0.50, 0.08], [0.50, 0.90]),
        ],
        Icon::Block => vec![
            stroke_rect([0.10, 0.10], [0.90, 0.90]),
            stroke_rect([0.30, 0.30], [0.70, 0.70]),
            seg([0.10, 0.10], [0.30, 0.30]),
            seg([0.90, 0.10], [0.70, 0.30]),
            seg([0.90, 0.90], [0.70, 0.70]),
            seg([0.10, 0.90], [0.30, 0.70]),
        ],
        Icon::Piece => vec![
            closed(&[
                [0.18, 0.12],
                [0.40, 0.12],
                [0.50, 0.24],
                [0.60, 0.12],
                [0.82, 0.12],
                [0.86, 0.88],
                [0.14, 0.88],
            ]),
            seg([0.50, 0.40], [0.50, 0.76]),
            arrow_head([0.50, 0.38], [0.0, -1.0], 0.10, 35.0),
            arrow_head([0.50, 0.78], [0.0, 1.0], 0.10, 35.0),
        ],
        Icon::PieceOff => vec![
            closed(&[
                [0.18, 0.12],
                [0.40, 0.12],
                [0.50, 0.24],
                [0.60, 0.12],
                [0.82, 0.12],
                [0.86, 0.88],
                [0.14, 0.88],
            ]),
            seg([0.08, 0.94], [0.92, 0.06]),
        ],
        Icon::PieceOn => vec![
            closed(&[
                [0.18, 0.12],
                [0.40, 0.12],
                [0.50, 0.24],
                [0.60, 0.12],
                [0.82, 0.12],
                [0.86, 0.88],
                [0.14, 0.88],
            ]),
            path(&[[0.32, 0.54], [0.46, 0.68], [0.70, 0.40]]),
        ],
        Icon::PieceSeamOff => vec![
            closed(&[
                [0.18, 0.12],
                [0.40, 0.12],
                [0.50, 0.24],
                [0.60, 0.12],
                [0.82, 0.12],
                [0.86, 0.88],
                [0.14, 0.88],
            ]),
            seg([0.22, 0.76], [0.34, 0.76]),
            seg([0.44, 0.76], [0.56, 0.76]),
            seg([0.66, 0.76], [0.78, 0.76]),
            seg([0.08, 0.94], [0.92, 0.06]),
        ],
        Icon::Tape => vec![
            cubic([0.08, 0.72], [0.32, 0.18], [0.68, 0.18], [0.92, 0.72], 18),
            seg([0.279, 0.416], [0.329, 0.502]),
            seg([0.50, 0.315], [0.50, 0.415]),
            seg([0.721, 0.416], [0.671, 0.502]),
            dot([0.08, 0.72], 0.045),
            dot([0.92, 0.72], 0.045),
        ],
        Icon::Height => vec![
            seg([0.50, 0.16], [0.50, 0.84]),
            seg([0.30, 0.08], [0.70, 0.08]),
            seg([0.30, 0.92], [0.70, 0.92]),
            arrow_head([0.50, 0.12], [0.0, -1.0], 0.13, 35.0),
            arrow_head([0.50, 0.88], [0.0, 1.0], 0.13, 35.0),
        ],
        Icon::Girth => vec![
            seg([0.26, 0.08], [0.26, 0.92]),
            seg([0.74, 0.08], [0.74, 0.92]),
            ellipse([0.50, 0.50], 0.40, 0.14, 0.0, 360.0, 28),
        ],
        Icon::Loop => vec![
            ellipse([0.50, 0.50], 0.40, 0.22, 0.0, 360.0, 28),
            arrow_head([0.62, 0.712], [1.0, -0.15], 0.13, 35.0),
        ],
        Icon::Dimension => vec![
            seg([0.18, 0.82], [0.82, 0.18]),
            seg([0.06, 0.74], [0.26, 0.94]),
            seg([0.74, 0.06], [0.94, 0.26]),
            arrow_head([0.16, 0.84], [-1.0, 1.0], 0.13, 35.0),
            arrow_head([0.84, 0.16], [1.0, -1.0], 0.13, 35.0),
        ],
        Icon::ShapeSize => vec![
            stroke_rect([0.14, 0.40], [0.86, 0.88]),
            seg([0.14, 0.18], [0.86, 0.18]),
            seg([0.14, 0.10], [0.14, 0.26]),
            seg([0.86, 0.10], [0.86, 0.26]),
        ],
        Icon::Pose => vec![
            circle([0.52, 0.14], 0.09),
            seg([0.50, 0.25], [0.46, 0.58]),
            path(&[[0.30, 0.92], [0.46, 0.58], [0.62, 0.92]]),
            path(&[
                [0.18, 0.10],
                [0.30, 0.34],
                [0.48, 0.34],
                [0.70, 0.44],
                [0.84, 0.30],
            ]),
        ],
        Icon::AvatarEdit => vec![
            circle([0.38, 0.30], 0.14),
            arc([0.38, 0.90], 0.30, 180.0, 360.0, 16),
            seg([0.84, 0.10], [0.84, 0.90]),
            circle([0.84, 0.40], 0.07),
        ],
        Icon::AvatarLibrary => vec![
            circle([0.66, 0.26], 0.11),
            arc([0.66, 0.70], 0.22, 215.0, 360.0, 12),
            circle([0.40, 0.36], 0.13),
            arc([0.40, 0.92], 0.28, 180.0, 360.0, 14),
        ],
        Icon::SelectAll => vec![
            stroke_rect([0.18, 0.18], [0.82, 0.82]),
            dot([0.18, 0.18], 0.06),
            dot([0.82, 0.18], 0.06),
            dot([0.82, 0.82], 0.06),
            dot([0.18, 0.82], 0.06),
        ],
        Icon::Paste => vec![
            stroke_rect([0.18, 0.16], [0.82, 0.92]),
            stroke_rect([0.36, 0.08], [0.64, 0.24]),
            seg([0.32, 0.46], [0.68, 0.46]),
            seg([0.32, 0.64], [0.60, 0.64]),
        ],
        Icon::AddPoint => vec![
            seg([0.10, 0.82], [0.90, 0.30]),
            dot([0.50, 0.56], 0.07),
            seg([0.24, 0.08], [0.24, 0.36]),
            seg([0.10, 0.22], [0.38, 0.22]),
        ],
        Icon::Smooth => vec![
            cubic([0.10, 0.80], [0.30, 0.20], [0.70, 0.90], [0.90, 0.30], 18),
            seg([0.24, 0.507], [0.76, 0.593]),
            dot([0.24, 0.507], 0.04),
            dot([0.76, 0.593], 0.04),
            dot([0.50, 0.55], 0.055),
        ],
        Icon::Extend => vec![
            seg([0.08, 0.50], [0.74, 0.50]),
            arrow_head([0.76, 0.50], [1.0, 0.0], 0.13, 35.0),
            seg([0.88, 0.14], [0.88, 0.86]),
        ],
        Icon::Trim => vec![
            seg([0.62, 0.10], [0.62, 0.90]),
            seg([0.10, 0.36], [0.62, 0.36]),
            seg([0.74, 0.36], [0.90, 0.36]),
            seg([0.10, 0.68], [0.50, 0.68]),
            arrow_head([0.58, 0.68], [1.0, 0.0], 0.10, 35.0),
        ],
        Icon::Replace => vec![
            stroke_rect([0.06, 0.30], [0.38, 0.62]),
            circle([0.74, 0.46], 0.18),
            seg([0.44, 0.46], [0.52, 0.46]),
            arrow_head([0.54, 0.46], [1.0, 0.0], 0.10, 35.0),
        ],
        Icon::Spiral => vec![spiral([0.50, 0.50], 0.04, 0.38, 2.25, 54)],
        Icon::InternalLine => vec![
            stroke_rect([0.08, 0.08], [0.92, 0.92]),
            seg([0.28, 0.70], [0.72, 0.30]),
            dot([0.28, 0.70], 0.05),
            dot([0.72, 0.30], 0.05),
        ],
        Icon::InternalRect => vec![
            stroke_rect([0.08, 0.08], [0.92, 0.92]),
            stroke_rect([0.30, 0.34], [0.70, 0.66]),
        ],
        Icon::InternalCircle => vec![
            stroke_rect([0.08, 0.08], [0.92, 0.92]),
            circle([0.50, 0.50], 0.20),
        ],
        Icon::InternalDart => vec![
            stroke_rect([0.08, 0.08], [0.92, 0.92]),
            closed(&[[0.50, 0.22], [0.62, 0.50], [0.50, 0.78], [0.38, 0.50]]),
        ],
        Icon::Trace => vec![
            path(&[[0.14, 0.40], [0.50, 0.14], [0.86, 0.40]]),
            dot([0.50, 0.14], 0.05),
            dot([0.86, 0.40], 0.05),
            dot([0.72, 0.84], 0.05),
            dot([0.28, 0.84], 0.05),
            dot([0.14, 0.40], 0.05),
        ],
        Icon::Guide => vec![
            seg([0.06, 0.36], [0.94, 0.36]),
            seg([0.64, 0.06], [0.64, 0.94]),
            dot([0.64, 0.36], 0.06),
        ],
        Icon::Align => vec![
            seg([0.06, 0.36], [0.94, 0.36]),
            stroke_rect([0.14, 0.36], [0.42, 0.66]),
            stroke_rect([0.58, 0.36], [0.86, 0.84]),
        ],
        Icon::Unlock => vec![
            stroke_rect([0.24, 0.46], [0.76, 0.86]),
            arc([0.50, 0.24], 0.15, 180.0, 360.0, 12),
            seg([0.65, 0.24], [0.65, 0.46]),
            seg([0.35, 0.24], [0.35, 0.32]),
            dot([0.50, 0.65], 0.045),
        ],
        Icon::Spread => vec![
            arc([0.50, 0.88], 0.72, 240.0, 300.0, 12),
            seg([0.50, 0.88], [0.14, 0.256]),
            seg([0.50, 0.88], [0.86, 0.256]),
            seg([0.50, 0.88], [0.40, 0.167]),
            seg([0.50, 0.88], [0.60, 0.167]),
        ],
        Icon::PiecePlus => vec![
            closed(&[
                [0.10, 0.10],
                [0.28, 0.10],
                [0.36, 0.18],
                [0.44, 0.10],
                [0.62, 0.10],
                [0.66, 0.70],
                [0.06, 0.70],
            ]),
            seg([0.80, 0.62], [0.80, 0.94]),
            seg([0.64, 0.78], [0.96, 0.78]),
        ],
        Icon::AutoSew => vec![
            seg([0.14, 0.14], [0.14, 0.86]),
            seg([0.54, 0.14], [0.54, 0.86]),
            seg([0.14, 0.30], [0.54, 0.42]),
            seg([0.54, 0.50], [0.14, 0.62]),
            seg([0.14, 0.70], [0.54, 0.80]),
            seg([0.80, 0.08], [0.80, 0.40]),
            seg([0.64, 0.24], [0.96, 0.24]),
        ],
        Icon::LengthCheck => vec![
            seg([0.10, 0.28], [0.90, 0.28]),
            seg([0.10, 0.16], [0.10, 0.40]),
            seg([0.90, 0.16], [0.90, 0.40]),
            path(&[[0.30, 0.64], [0.46, 0.80], [0.74, 0.50]]),
        ],
        Icon::TapeBand => vec![
            stroke_rect([0.08, 0.34], [0.92, 0.66]),
            seg([0.16, 0.50], [0.30, 0.50]),
            seg([0.43, 0.50], [0.57, 0.50]),
            seg([0.70, 0.50], [0.84, 0.50]),
        ],
        Icon::TapeRoll => vec![
            circle([0.42, 0.42], 0.30),
            circle([0.42, 0.42], 0.11),
            seg([0.42, 0.72], [0.94, 0.72]),
        ],
        Icon::StitchSeam => vec![
            seg([0.08, 0.50], [0.92, 0.50]),
            seg([0.10, 0.28], [0.24, 0.28]),
            seg([0.43, 0.28], [0.57, 0.28]),
            seg([0.76, 0.28], [0.90, 0.28]),
            seg([0.10, 0.72], [0.24, 0.72]),
            seg([0.43, 0.72], [0.57, 0.72]),
            seg([0.76, 0.72], [0.90, 0.72]),
        ],
        Icon::StitchFree => vec![
            arc([0.50, 0.95], 0.60, 222.0, 238.0, 4),
            arc([0.50, 0.95], 0.60, 248.0, 264.0, 4),
            arc([0.50, 0.95], 0.60, 276.0, 292.0, 4),
            arc([0.50, 0.95], 0.60, 302.0, 318.0, 4),
            seg([0.10, 0.80], [0.90, 0.80]),
        ],
        Icon::Piping => vec![
            seg([0.08, 0.36], [0.62, 0.36]),
            seg([0.08, 0.64], [0.62, 0.64]),
            circle([0.76, 0.50], 0.15),
        ],
        Icon::Buttonhole => vec![
            arc([0.30, 0.50], 0.12, 90.0, 270.0, 8),
            arc([0.70, 0.50], 0.12, -90.0, 90.0, 8),
            seg([0.30, 0.38], [0.70, 0.38]),
            seg([0.30, 0.62], [0.70, 0.62]),
            seg([0.14, 0.30], [0.14, 0.70]),
            seg([0.86, 0.30], [0.86, 0.70]),
        ],
        Icon::Fasten => vec![
            circle([0.50, 0.50], 0.32),
            seg([0.40, 0.40], [0.60, 0.60]),
            seg([0.60, 0.40], [0.40, 0.60]),
        ],
        Icon::Zipper => vec![
            seg([0.50, 0.06], [0.50, 0.66]),
            seg([0.36, 0.14], [0.50, 0.14]),
            seg([0.50, 0.26], [0.64, 0.26]),
            seg([0.36, 0.38], [0.50, 0.38]),
            seg([0.50, 0.50], [0.64, 0.50]),
            stroke_rect([0.40, 0.66], [0.60, 0.92]),
        ],
        Icon::GraphicMove => vec![
            stroke_rect([0.16, 0.16], [0.84, 0.84]),
            dot([0.16, 0.16], 0.055),
            dot([0.84, 0.16], 0.055),
            dot([0.84, 0.84], 0.055),
            dot([0.16, 0.84], 0.055),
            path(&[
                [0.28, 0.70],
                [0.44, 0.46],
                [0.56, 0.60],
                [0.64, 0.52],
                [0.74, 0.70],
            ]),
        ],
        Icon::Texture => vec![
            stroke_rect([0.12, 0.12], [0.88, 0.88]),
            cubic([0.12, 0.38], [0.37, 0.22], [0.63, 0.52], [0.88, 0.38], 14),
            cubic([0.12, 0.64], [0.37, 0.48], [0.63, 0.78], [0.88, 0.64], 14),
        ],
        Icon::PleatSingle => vec![path(&[
            [0.08, 0.30],
            [0.64, 0.30],
            [0.36, 0.70],
            [0.92, 0.70],
        ])],
        Icon::PleatSew => vec![
            path(&[
                [0.08, 0.16],
                [0.29, 0.56],
                [0.50, 0.16],
                [0.71, 0.56],
                [0.92, 0.16],
            ]),
            seg([0.10, 0.80], [0.26, 0.80]),
            seg([0.42, 0.80], [0.58, 0.80]),
            seg([0.74, 0.80], [0.90, 0.80]),
        ],
        Icon::Eye => vec![
            cubic([0.06, 0.50], [0.30, 0.16], [0.70, 0.16], [0.94, 0.50], 16),
            cubic([0.94, 0.50], [0.70, 0.84], [0.30, 0.84], [0.06, 0.50], 16),
            circle([0.50, 0.50], 0.13),
        ],
        Icon::Tag => vec![
            closed(&[
                [0.08, 0.30],
                [0.62, 0.30],
                [0.92, 0.50],
                [0.62, 0.70],
                [0.08, 0.70],
            ]),
            dot([0.68, 0.50], 0.045),
            seg([0.18, 0.50], [0.48, 0.50]),
        ],
        Icon::Colours => vec![
            circle([0.37, 0.38], 0.22),
            circle([0.63, 0.38], 0.22),
            circle([0.50, 0.62], 0.22),
        ],
        Icon::Force => vec![
            stroke_rect([0.38, 0.32], [0.62, 0.68]),
            seg([0.10, 0.50], [0.32, 0.50]),
            arrow_head([0.08, 0.50], [-1.0, 0.0], 0.13, 35.0),
            seg([0.68, 0.50], [0.90, 0.50]),
            arrow_head([0.92, 0.50], [1.0, 0.0], 0.13, 35.0),
        ],
        Icon::Pressure => vec![
            seg([0.30, 0.10], [0.30, 0.44]),
            arrow_head([0.30, 0.48], [0.0, 1.0], 0.13, 35.0),
            seg([0.70, 0.10], [0.70, 0.44]),
            arrow_head([0.70, 0.48], [0.0, 1.0], 0.13, 35.0),
            cubic([0.06, 0.66], [0.36, 0.54], [0.64, 0.54], [0.94, 0.66], 14),
        ],
        Icon::Shadow => vec![
            circle([0.50, 0.38], 0.24),
            ellipse([0.50, 0.82], 0.32, 0.08, 0.0, 360.0, 24),
        ],
        Icon::Floor => vec![
            closed(&[[0.22, 0.36], [0.78, 0.36], [0.94, 0.80], [0.06, 0.80]]),
            seg([0.50, 0.36], [0.50, 0.80]),
            seg([0.14, 0.58], [0.86, 0.58]),
        ],
        Icon::Page => vec![
            closed(&[
                [0.20, 0.08],
                [0.62, 0.08],
                [0.82, 0.28],
                [0.82, 0.92],
                [0.20, 0.92],
            ]),
            path(&[[0.62, 0.08], [0.62, 0.28], [0.82, 0.28]]),
        ],
        Icon::PagePlus => vec![
            closed(&[
                [0.14, 0.06],
                [0.48, 0.06],
                [0.64, 0.22],
                [0.64, 0.74],
                [0.14, 0.74],
            ]),
            path(&[[0.48, 0.06], [0.48, 0.22], [0.64, 0.22]]),
            seg([0.80, 0.62], [0.80, 0.94]),
            seg([0.64, 0.78], [0.96, 0.78]),
        ],
        Icon::RulerCorner => vec![
            closed(&[
                [0.10, 0.10],
                [0.90, 0.10],
                [0.90, 0.30],
                [0.30, 0.30],
                [0.30, 0.90],
                [0.10, 0.90],
            ]),
            seg([0.46, 0.10], [0.46, 0.20]),
            seg([0.68, 0.10], [0.68, 0.20]),
            seg([0.10, 0.46], [0.20, 0.46]),
            seg([0.10, 0.68], [0.20, 0.68]),
        ],
        Icon::Bolt => vec![closed(&[
            [0.58, 0.06],
            [0.22, 0.56],
            [0.48, 0.56],
            [0.40, 0.94],
            [0.78, 0.42],
            [0.53, 0.42],
            [0.66, 0.06],
        ])],
        Icon::Gem => vec![
            closed(&[
                [0.20, 0.36],
                [0.34, 0.16],
                [0.66, 0.16],
                [0.80, 0.36],
                [0.50, 0.86],
            ]),
            seg([0.20, 0.36], [0.80, 0.36]),
            path(&[[0.40, 0.16], [0.36, 0.36], [0.50, 0.86]]),
            path(&[[0.60, 0.16], [0.64, 0.36], [0.50, 0.86]]),
        ],
        Icon::Swatch => vec![dot([0.50, 0.50], 0.30)],
        Icon::Chat => vec![
            closed(&[
                [0.10, 0.16],
                [0.90, 0.16],
                [0.90, 0.66],
                [0.44, 0.66],
                [0.24, 0.86],
                [0.26, 0.66],
                [0.10, 0.66],
            ]),
            dot([0.32, 0.41], 0.045),
            dot([0.50, 0.41], 0.045),
            dot([0.68, 0.41], 0.045),
        ],
        Icon::Sliders => vec![
            seg([0.10, 0.30], [0.52, 0.30]),
            seg([0.72, 0.30], [0.90, 0.30]),
            circle([0.62, 0.30], 0.10),
            seg([0.10, 0.72], [0.28, 0.72]),
            seg([0.48, 0.72], [0.90, 0.72]),
            circle([0.38, 0.72], 0.10),
        ],
        Icon::Restore => vec![
            arc([0.54, 0.54], 0.32, 200.0, 495.0, 24),
            path(&[[0.16, 0.22], [0.18, 0.46], [0.40, 0.42]]),
        ],
        Icon::Paperclip => vec![path(&[
            [0.78, 0.46],
            [0.46, 0.80],
            [0.30, 0.84],
            [0.18, 0.72],
            [0.22, 0.56],
            [0.58, 0.20],
            [0.72, 0.16],
            [0.82, 0.26],
            [0.78, 0.40],
            [0.44, 0.74],
            [0.36, 0.72],
            [0.34, 0.64],
            [0.64, 0.34],
        ])],
        // Nested squares read as a filled one at icon size.
        // A filled square: outlines inside each other at a small size.
        Icon::Stop => vec![
            stroke_rect([0.12, 0.12], [0.88, 0.88]),
            stroke_rect([0.25, 0.25], [0.75, 0.75]),
            stroke_rect([0.38, 0.38], [0.62, 0.62]),
        ],
        Icon::Lock => vec![
            stroke_rect([0.24, 0.46], [0.76, 0.86]),
            arc([0.50, 0.32], 0.15, 180.0, 360.0, 12),
            seg([0.35, 0.32], [0.35, 0.46]),
            seg([0.65, 0.32], [0.65, 0.46]),
            dot([0.50, 0.65], 0.045),
        ],
        Icon::Sheet => vec![
            stroke_rect([0.12, 0.20], [0.88, 0.80]),
            seg([0.12, 0.40], [0.88, 0.40]),
            seg([0.34, 0.20], [0.34, 0.80]),
        ],
        Icon::Expand => vec![
            path(&[[0.58, 0.16], [0.84, 0.16], [0.84, 0.42]]),
            seg([0.84, 0.16], [0.58, 0.42]),
            path(&[[0.42, 0.84], [0.16, 0.84], [0.16, 0.58]]),
            seg([0.16, 0.84], [0.42, 0.58]),
        ],
        Icon::Trash => vec![
            seg([0.18, 0.27], [0.82, 0.27]),
            path(&[[0.40, 0.27], [0.42, 0.15], [0.58, 0.15], [0.60, 0.27]]),
            path(&[[0.27, 0.27], [0.32, 0.86], [0.68, 0.86], [0.73, 0.27]]),
        ],
        Icon::Polygon => vec![closed(&[
            [0.50, 0.14],
            [0.86, 0.40],
            [0.72, 0.84],
            [0.28, 0.84],
            [0.14, 0.40],
        ])],
    }
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

/// Largest square centred inside `rect`.
///
/// Backing the glyph with a square rather than the raw rectangle keeps stroke
/// weight and proportions identical in a wide toolbar button and a square
/// icon-only button.
fn square_for(rect: Rect) -> Rect {
    let side = rect.width().min(rect.height()).max(0.0);
    Rect::from_center_size(rect.center(), Vec2::splat(side))
}

/// Map one normalised point onto a painter rectangle.
fn map(square: Rect, point: [f32; 2]) -> Pos2 {
    egui::pos2(
        square.left() + point[0] * square.width(),
        square.top() + point[1] * square.height(),
    )
}

/// Draw `prims` into `rect`, honouring the caller's colour and stroke weight.
fn paint(painter: &egui::Painter, rect: Rect, prims: &[Prim], color: Color32, stroke: f32) {
    let side = rect.width().min(rect.height());
    if !side.is_finite() || side <= 0.0 {
        return;
    }
    let stroke = if stroke.is_finite() && stroke > 0.0 {
        stroke
    } else {
        1.0
    };
    let style = Stroke::new(stroke, color);
    for prim in prims {
        match prim {
            Prim::Line { a, b } => {
                painter.line_segment([map(rect, *a), map(rect, *b)], style);
            }
            Prim::Path { points } if points.len() >= 2 => {
                let mapped: Vec<Pos2> = points.iter().map(|p| map(rect, *p)).collect();
                painter.add(Shape::line(mapped, style));
            }
            Prim::Path { .. } => {}
            Prim::Circle { center, radius } => {
                painter.circle_stroke(map(rect, *center), radius * side, style);
            }
            Prim::Rect { min, max } => {
                painter.rect_stroke(
                    Rect::from_min_max(map(rect, *min), map(rect, *max)),
                    CornerRadius::same(2),
                    style,
                    StrokeKind::Inside,
                );
            }
            Prim::Dot { center, radius } => {
                painter.circle_filled(map(rect, *center), (radius * side).max(1.0), color);
            }
        }
    }
}

/// Draw one glyph inside `rect`.
pub(super) fn glyph(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: Icon,
    color: Color32,
    stroke: f32,
) {
    let square = square_for(rect);
    if square.width() <= 0.0 {
        return;
    }
    paint(painter, square, &geometry(icon), color, stroke);
}

/// Background and icon colours for one button state.
/// The tool in use is tinted with the accent and drawn in it, not filled solid: a row of
/// solid accent squares shouted over the work.
fn button_fill(visuals: &egui::Visuals, active: bool, hovered: bool) -> Color32 {
    if active {
        super::workspace::accent(visuals).gamma_multiply(if hovered { 0.30 } else { 0.20 })
    } else if hovered {
        visuals.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    }
}

fn button_ink(visuals: &egui::Visuals, active: bool) -> Color32 {
    if active {
        super::workspace::accent(visuals)
    } else {
        visuals.text_color()
    }
}

/// Icon plus label, sized to its own text. Returns the click response.
pub(super) fn tool_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    active: bool,
    tip: &str,
) -> Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let ink = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, ink);
    let height = ui.spacing().interact_size.y.max(26.0);
    let padding = 6.0;
    let icon_size = height - 10.0;
    let width = padding + icon_size + 6.0 + galley.size().x + padding;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = button_fill(visuals, active, response.hovered());
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
        }
        let icon_rect = Rect::from_min_size(
            egui::pos2(rect.left() + padding, rect.center().y - icon_size * 0.5),
            Vec2::splat(icon_size),
        );
        glyph(
            ui.painter(),
            icon_rect,
            icon,
            button_ink(visuals, active),
            1.6,
        );
        ui.painter().galley(
            egui::pos2(
                icon_rect.right() + 6.0,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            ink,
        );
    }
    with_tip(response, tip)
}

/// Icon and a vector chevron: no dependence on font coverage for the menu arrow.
pub(super) fn dropdown_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(42., 28.), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let ink = ui.visuals().text_color();
        let fill = button_fill(ui.visuals(), false, response.hovered());
        ui.painter().rect_filled(rect, 3., fill);
        glyph(
            ui.painter(),
            Rect::from_center_size(rect.center() - Vec2::new(6., 0.), Vec2::splat(18.)),
            icon,
            ink,
            1.6,
        );
        let center = rect.center() + Vec2::new(12., 0.);
        ui.painter().add(Shape::convex_polygon(
            vec![
                center + Vec2::new(-3., -1.5),
                center + Vec2::new(3., -1.5),
                center + Vec2::new(0., 2.),
            ],
            ink,
            Stroke::NONE,
        ));
    }
    with_tip(response, tip)
}

/// Square icon-only button for dense rows. Returns the click response.
pub(super) fn icon_only(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    active: bool,
    tip: &str,
) -> Response {
    let size = if size.is_finite() && size > 0.0 {
        size
    } else {
        20.0
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = button_fill(visuals, active, response.hovered());
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
        }
        glyph(
            ui.painter(),
            rect.shrink(size * 0.18),
            icon,
            button_ink(visuals, active),
            1.6,
        );
    }
    with_tip(response, tip)
}

/// A square icon button standing for a group of tools, with a small triangle in its corner as
/// desktop design tools draw it. The second value is true when the group's list should open: a
/// click on the triangle or a secondary click. A plain click runs the tool shown.
pub(super) fn flyout_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    active: bool,
    tip: &str,
) -> (Response, bool) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let corner = size * 0.38;
    let on_corner = response.clicked()
        && response
            .interact_pointer_pos()
            .is_some_and(|p| p.x >= rect.right() - corner && p.y >= rect.bottom() - corner);
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = button_fill(visuals, active, response.hovered());
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
        }
        let ink = button_ink(visuals, active);
        glyph(ui.painter(), rect.shrink(size * 0.2), icon, ink, 1.5);
        let tip_point = rect.right_bottom() - Vec2::splat(3.);
        ui.painter().add(Shape::convex_polygon(
            vec![
                tip_point,
                tip_point - Vec2::new(0., 4.),
                tip_point - Vec2::new(4., 0.),
            ],
            ink.gamma_multiply(0.75),
            Stroke::NONE,
        ));
    }
    let open = on_corner || response.secondary_clicked();
    (with_tip(response, tip), open)
}

/// The width a [`menu_row`] needs for `label` and `keys`.
pub(super) fn menu_row_width(ui: &egui::Ui, label: &str, keys: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let measure = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    36. + measure(label) + 28. + measure(keys) + 10.
}

/// One row of a tool group's list: its icon, its name and its keys, highlighted when it is the
/// tool in use. Returns the click response.
pub(super) fn menu_row(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    label: &str,
    keys: &str,
    checked: bool,
) -> Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let visuals = ui.visuals();
    let ink = if ui.is_enabled() {
        visuals.text_color()
    } else {
        visuals.weak_text_color()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), ink);
    let keys = ui
        .painter()
        .layout_no_wrap(keys.to_owned(), font, visuals.weak_text_color());
    let height = 26.0;
    let width = menu_row_width(ui, label, keys.text()).max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        // The row in use is only tinted, its icon (or a check) in the accent colour: a filled
        // bar per choice made the list loud.
        let accent = super::workspace::accent(visuals);
        let fill = if response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else if checked {
            accent.gamma_multiply(0.14)
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
        }
        let icon_rect = Rect::from_center_size(
            egui::pos2(rect.left() + 8. + 9., rect.center().y),
            Vec2::splat(16.),
        );
        match icon {
            Some(icon) => glyph(
                ui.painter(),
                icon_rect,
                icon,
                if checked { accent } else { ink },
                1.5,
            ),
            None if checked => {
                let mark = icon_rect.shrink(3.);
                let at = |x: f32, y: f32| mark.min + Vec2::new(x * mark.width(), y * mark.height());
                ui.painter().add(Shape::line(
                    vec![at(0.08, 0.55), at(0.38, 0.84), at(0.92, 0.18)],
                    Stroke::new(1.6, accent),
                ));
            }
            None => {}
        }
        ui.painter().galley(
            egui::pos2(rect.left() + 36., rect.center().y - galley.size().y * 0.5),
            galley,
            ink,
        );
        let keys_size = keys.size();
        ui.painter().galley(
            egui::pos2(
                rect.right() - 10. - keys_size.x,
                rect.center().y - keys_size.y * 0.5,
            ),
            keys,
            ink,
        );
    }
    response
}

/// One row of a display menu: a check in FashionCraft's colour when on, the item's icon, its
/// name and its keys, as desktop 3D tools list what a view shows.
pub(super) fn check_row(
    ui: &mut egui::Ui,
    checked: bool,
    icon: Icon,
    label: &str,
    keys: &str,
) -> Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let visuals = ui.visuals();
    let ink = if ui.is_enabled() {
        visuals.text_color()
    } else {
        visuals.weak_text_color()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), ink);
    let keys = ui
        .painter()
        .layout_no_wrap(keys.to_owned(), font, visuals.weak_text_color());
    let width = (64. + galley.size().x + 28. + keys.size().x + 10.).max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(3), visuals.widgets.hovered.bg_fill);
        }
        let accent = super::workspace::accent(visuals);
        if checked {
            let mark = Rect::from_center_size(
                egui::pos2(rect.left() + 14., rect.center().y),
                Vec2::splat(11.),
            );
            let at = |x: f32, y: f32| mark.min + Vec2::new(x * mark.width(), y * mark.height());
            ui.painter().add(Shape::line(
                vec![at(0.05, 0.55), at(0.38, 0.86), at(0.95, 0.15)],
                Stroke::new(1.8, accent),
            ));
        }
        glyph(
            ui.painter(),
            Rect::from_center_size(
                egui::pos2(rect.left() + 39., rect.center().y),
                Vec2::splat(17.),
            ),
            icon,
            ink,
            1.5,
        );
        ui.painter().galley(
            egui::pos2(rect.left() + 58., rect.center().y - galley.size().y * 0.5),
            galley,
            ink,
        );
        let keys_size = keys.size();
        ui.painter().galley(
            egui::pos2(
                rect.right() - 10. - keys_size.x,
                rect.center().y - keys_size.y * 0.5,
            ),
            keys,
            ink,
        );
    }
    response
}

fn with_tip(response: Response, tip: &str) -> Response {
    if tip.is_empty() {
        response
    } else {
        response.on_hover_text(tip)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Points that bound a primitive, including stroke extents.
    fn extent_points(prim: &Prim) -> Vec<[f32; 2]> {
        match prim {
            Prim::Line { a, b } => vec![*a, *b],
            Prim::Path { points } => points.clone(),
            Prim::Circle { center, radius } => vec![
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
            ],
            Prim::Rect { min, max } => vec![*min, *max, [min[0], max[1]], [max[0], min[1]]],
            Prim::Dot { center, radius } => vec![
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
            ],
        }
    }

    /// A primitive that would paint nothing visible.
    fn is_degenerate(prim: &Prim) -> bool {
        match prim {
            Prim::Line { a, b } => squared_distance(a, b) <= 1e-6,
            Prim::Path { points } => {
                points.len() < 2
                    || points
                        .iter()
                        .all(|p| squared_distance(p, &points[0]) <= 1e-6)
            }
            Prim::Circle { radius, .. } => *radius <= 1e-4,
            Prim::Rect { min, max } => (max[0] - min[0]) <= 1e-4 || (max[1] - min[1]) <= 1e-4,
            Prim::Dot { radius, .. } => *radius <= 1e-4,
        }
    }

    fn squared_distance(a: &[f32; 2], b: &[f32; 2]) -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        dx * dx + dy * dy
    }

    #[test]
    fn every_icon_draws_something() {
        for icon in ALL {
            let prims = geometry(icon);
            assert!(!prims.is_empty(), "{icon:?} generates no geometry");
            for prim in &prims {
                assert!(
                    !is_degenerate(prim),
                    "{icon:?} contains a primitive that would not paint: {prim:?}"
                );
            }
        }
    }

    #[test]
    fn every_primitive_stays_inside_the_unit_square() {
        for icon in ALL {
            for prim in geometry(icon) {
                for point in extent_points(&prim) {
                    assert!(
                        (0.0..=1.0).contains(&point[0]) && (0.0..=1.0).contains(&point[1]),
                        "{icon:?} leaves the unit square at {point:?} ({prim:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn primitive_counts_are_stable() {
        // Recorded per icon. If a glyph silently loses its geometry (or gains a
        // stray primitive), the count changes and this fails instead of the
        // blank button reaching the user.
        let expected: [(Icon, usize); 156] = [
            (Icon::Select, 1),
            (Icon::Edit, 3),
            (Icon::Pen, 3),
            (Icon::Rectangle, 2),
            (Icon::Sew, 5),
            (Icon::Move3d, 7),
            (Icon::Rotate3d, 3),
            (Icon::Grab, 1),
            (Icon::Undo, 2),
            (Icon::Redo, 2),
            (Icon::Duplicate, 2),
            (Icon::Mirror, 3),
            (Icon::Grid, 5),
            (Icon::Snap, 5),
            (Icon::Fit, 5),
            (Icon::ViewIso, 3),
            (Icon::ViewFront, 2),
            (Icon::ViewBack, 3),
            (Icon::ViewLeft, 3),
            (Icon::ViewRight, 3),
            (Icon::ViewTop, 3),
            (Icon::Settings, 7),
            (Icon::Theme, 5),
            (Icon::Fabric, 3),
            (Icon::Simulate, 1),
            (Icon::Step, 2),
            (Icon::Reset, 2),
            (Icon::Export, 3),
            (Icon::Search, 2),
            (Icon::Info, 3),
            (Icon::Warning, 3),
            (Icon::Seam, 4),
            (Icon::Notch, 3),
            (Icon::Gradient, 4),
            (Icon::Ellipse, 1),
            (Icon::Arc, 2),
            (Icon::Union, 1),
            (Icon::Subtract, 2),
            (Icon::Intersect, 4),
            (Icon::Measure, 5),
            (Icon::Graphic, 3),
            (Icon::Avatar, 6),
            (Icon::Garment, 1),
            (Icon::Skeleton, 5),
            (Icon::Technical, 5),
            (Icon::Pin, 2),
            (Icon::Button, 5),
            (Icon::Curve, 1),
            (Icon::Polygon, 1),
            (Icon::Trash, 3),
            (Icon::Expand, 4),
            (Icon::Iron, 5),
            (Icon::Snowflake, 3),
            (Icon::Pleat, 2),
            (Icon::Points, 5),
            (Icon::Wave, 2),
            (Icon::Stitch, 4),
            (Icon::Layers, 2),
            (Icon::Scissors, 4),
            (Icon::SelectArea, 4),
            (Icon::SplitView, 2),
            (Icon::Sheet, 3),
            (Icon::Lock, 5),
            (Icon::Brush, 4),
            (Icon::Lasso, 3),
            (Icon::PinBox, 6),
            (Icon::PinLasso, 5),
            (Icon::StyleEdit, 2),
            (Icon::StyleMove, 4),
            (Icon::StyleScale, 4),
            (Icon::StyleDraw, 3),
            (Icon::PinAttach, 3),
            (Icon::PinDetach, 5),
            (Icon::Tear, 2),
            (Icon::TearEdit, 6),
            (Icon::ResetSheet, 3),
            (Icon::ResetSelected, 3),
            (Icon::Fold, 2),
            (Icon::Drape, 4),
            (Icon::FitBody, 5),
            (Icon::SewFree, 4),
            (Icon::SewChain, 5),
            (Icon::SewChainFree, 5),
            (Icon::Quads, 3),
            (Icon::Triangles, 6),
            (Icon::MeshFiner, 7),
            (Icon::MeshCoarser, 5),
            (Icon::LayerOver, 3),
            (Icon::LayerUnder, 3),
            (Icon::LayerSide, 4),
            (Icon::Drop, 3),
            (Icon::Shield, 2),
            (Icon::Block, 6),
            (Icon::Piece, 4),
            (Icon::PieceOff, 2),
            (Icon::PieceOn, 2),
            (Icon::PieceSeamOff, 5),
            (Icon::Tape, 6),
            (Icon::Height, 5),
            (Icon::Girth, 3),
            (Icon::Loop, 2),
            (Icon::Dimension, 5),
            (Icon::ShapeSize, 4),
            (Icon::Pose, 4),
            (Icon::AvatarEdit, 4),
            (Icon::AvatarLibrary, 4),
            (Icon::SelectAll, 5),
            (Icon::Paste, 4),
            (Icon::AddPoint, 4),
            (Icon::Smooth, 5),
            (Icon::Extend, 3),
            (Icon::Trim, 5),
            (Icon::Replace, 4),
            (Icon::Spiral, 1),
            (Icon::InternalLine, 4),
            (Icon::InternalRect, 2),
            (Icon::InternalCircle, 2),
            (Icon::InternalDart, 2),
            (Icon::Trace, 6),
            (Icon::Guide, 3),
            (Icon::Align, 3),
            (Icon::Unlock, 5),
            (Icon::Spread, 5),
            (Icon::PiecePlus, 3),
            (Icon::AutoSew, 7),
            (Icon::LengthCheck, 4),
            (Icon::TapeBand, 4),
            (Icon::TapeRoll, 3),
            (Icon::StitchSeam, 7),
            (Icon::StitchFree, 5),
            (Icon::Piping, 3),
            (Icon::Buttonhole, 6),
            (Icon::Fasten, 3),
            (Icon::Zipper, 6),
            (Icon::GraphicMove, 6),
            (Icon::Texture, 3),
            (Icon::PleatSingle, 1),
            (Icon::PleatSew, 4),
            (Icon::Eye, 3),
            (Icon::Tag, 3),
            (Icon::Colours, 3),
            (Icon::Force, 5),
            (Icon::Pressure, 5),
            (Icon::Shadow, 2),
            (Icon::Floor, 3),
            (Icon::Page, 2),
            (Icon::PagePlus, 4),
            (Icon::RulerCorner, 5),
            (Icon::Bolt, 1),
            (Icon::Gem, 4),
            (Icon::Swatch, 1),
            (Icon::Chat, 4),
            (Icon::Sliders, 6),
            (Icon::Restore, 2),
            (Icon::Paperclip, 1),
            (Icon::Stop, 3),
        ];
        // A new variant without a recorded count is itself a regression.
        assert_eq!(expected.len(), ALL.len());
        let mut total = 0;
        for (slot, (icon, count)) in expected.iter().enumerate() {
            assert_eq!(*icon, ALL[slot], "recorded icons and ALL drifted apart");
            assert!(*count > 0, "{icon:?} is recorded as empty");
            let actual = geometry(*icon).len();
            assert_eq!(actual, *count, "{icon:?} primitive count changed");
            total += actual;
        }
        assert!(total >= 100, "the palette collapsed to {total} primitives");
    }

    #[test]
    fn the_degenerate_guard_rejects_bad_geometry() {
        // The guard must actually fail on a glyph that would render as nothing,
        // otherwise the tests above prove little.
        assert!(is_degenerate(&Prim::Line {
            a: [0.5, 0.5],
            b: [0.5, 0.5],
        }));
        assert!(is_degenerate(&Prim::Path { points: vec![] }));
        assert!(is_degenerate(&Prim::Path {
            points: vec![[0.4, 0.4], [0.4, 0.4]],
        }));
        assert!(is_degenerate(&Prim::Circle {
            center: [0.5, 0.5],
            radius: 0.0,
        }));
        assert!(is_degenerate(&Prim::Rect {
            min: [0.2, 0.2],
            max: [0.2, 0.8],
        }));
        assert!(is_degenerate(&Prim::Dot {
            center: [0.5, 0.5],
            radius: 0.0,
        }));
        // ...while accepting a healthy primitive of each kind.
        assert!(!is_degenerate(&Prim::Line {
            a: [0.2, 0.2],
            b: [0.8, 0.8],
        }));
        assert!(!is_degenerate(&circle([0.5, 0.5], 0.3)));
        assert!(!is_degenerate(&stroke_rect([0.2, 0.2], [0.8, 0.8])));
        assert!(!is_degenerate(&dot([0.5, 0.5], 0.05)));
    }

    #[test]
    fn glyphs_are_painted_into_a_centred_square_inside_the_caller_rect() {
        let wide = Rect::from_min_size(egui::pos2(10.0, 20.0), Vec2::new(100.0, 40.0));
        let square = square_for(wide);
        assert_eq!(square.width(), 40.0);
        assert_eq!(square.height(), 40.0);
        assert_eq!(square.center(), wide.center());

        for icon in ALL {
            for prim in geometry(icon) {
                for point in extent_points(&prim) {
                    let mapped = map(square, point);
                    assert!(
                        wide.contains(mapped),
                        "{icon:?} paints outside the rect it was given"
                    );
                }
            }
        }
    }

    #[test]
    fn palette_widgets_allocate_clickable_rects() {
        // Headless: layout only, no window, no GPU.
        let ctx = egui::Context::default();
        let mut sizes = Vec::new();
        // `run_ui` needs a root `Ui`; the returned output carries font texture
        // deltas that must be cleared, since there is no renderer to apply them.
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let labelled = tool_button(ui, Icon::Sew, "Sew", false, "Stitch two edges");
            let compact = icon_only(ui, Icon::Fit, 20.0, true, "Fit the view");
            sizes.push((labelled.rect.size(), compact.rect.size()));
        });
        output.drop_without_applying_deltas();
        let (labelled, compact) = sizes[0];
        assert!(labelled.x > 0.0 && labelled.y > 0.0);
        assert!(labelled.x > labelled.y, "the label should widen the button");
        assert_eq!(compact, Vec2::splat(20.0));
    }
}
