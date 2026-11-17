//! Board arrangement of contour points and panels: rotate/scale points about a pivot, align
//! points or panels along one axis, distribute panels with a fixed gap and scale a whole panel.
//! All coordinates here are board coordinates in metres (v grows downward), except where a
//! function says it works in a panel's material frame.
use super::*;
use serde::{Deserialize, Serialize};

/// Where aligned points or panels line up. Top is the smallest v (the board's v grows
/// downward). The `ToPoint*` modes line up with a reference point's coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignMode {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
    ToPointX,
    ToPointY,
}

impl AlignMode {
    pub const ALL: [Self; 8] = [
        Self::Left,
        Self::Center,
        Self::Right,
        Self::Top,
        Self::Middle,
        Self::Bottom,
        Self::ToPointX,
        Self::ToPointY,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Middle => "Middle",
            Self::Bottom => "Bottom",
            Self::ToPointX => "To point X",
            Self::ToPointY => "To point Y",
        }
    }
    /// The board axis the mode moves along (0 = u, 1 = v).
    pub fn axis(self) -> usize {
        match self {
            Self::Left | Self::Center | Self::Right | Self::ToPointX => 0,
            _ => 1,
        }
    }
}

/// Board axis for distribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// `p` rotated by `rotation_degrees` and scaled by `scale` about `pivot` (scale first, along
/// the board axes).
pub fn affine(p: DVec2, pivot: DVec2, rotation_degrees: f64, scale: DVec2) -> DVec2 {
    pivot + rotate_2d((p - pivot) * scale, rotation_degrees)
}

/// Board bounds `(min, max)` of a set of points.
pub fn bounds(points: &[DVec2]) -> Option<(DVec2, DVec2)> {
    let first = *points.first()?;
    Some(
        points
            .iter()
            .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
    )
}

/// Where each point goes when the set is aligned. Only the mode's axis changes. `reference`
/// is required by the `ToPoint*` modes.
pub fn align_points(
    points: &[DVec2],
    mode: AlignMode,
    reference: Option<DVec2>,
) -> Result<Vec<DVec2>, String> {
    let (lo, hi) = bounds(points).ok_or("Select points to align")?;
    let axis = mode.axis();
    let target = match mode {
        AlignMode::Left | AlignMode::Top => lo[axis],
        AlignMode::Right | AlignMode::Bottom => hi[axis],
        AlignMode::Center | AlignMode::Middle => (lo[axis] + hi[axis]) * 0.5,
        AlignMode::ToPointX | AlignMode::ToPointY => {
            reference.ok_or("Choose the point to align to")?[axis]
        }
    };
    Ok(points
        .iter()
        .map(|p| {
            let mut q = *p;
            q[axis] = target;
            q
        })
        .collect())
}

/// Offsets that line each box up with `reference` (a box is `(min, max)`).
pub fn align_box_offsets(
    boxes: &[(DVec2, DVec2)],
    reference: (DVec2, DVec2),
    mode: AlignMode,
) -> Result<Vec<DVec2>, String> {
    let axis = mode.axis();
    let edge = |(lo, hi): (DVec2, DVec2)| match mode {
        AlignMode::Left | AlignMode::Top => Ok(lo[axis]),
        AlignMode::Right | AlignMode::Bottom => Ok(hi[axis]),
        AlignMode::Center | AlignMode::Middle => Ok((lo[axis] + hi[axis]) * 0.5),
        AlignMode::ToPointX | AlignMode::ToPointY => {
            Err("Pieces align by their edges or centres, not to a point".to_string())
        }
    };
    let target = edge(reference)?;
    boxes
        .iter()
        .map(|b| {
            let mut delta = DVec2::ZERO;
            delta[axis] = target - edge(*b)?;
            Ok(delta)
        })
        .collect()
}

/// Offsets that space the boxes along `axis` with `gap` between neighbours, in their current
/// order along that axis; the first box stays where it is.
pub fn distribute_offsets(boxes: &[(DVec2, DVec2)], axis: Axis, gap: f64) -> Vec<DVec2> {
    let k = match axis {
        Axis::Horizontal => 0,
        Axis::Vertical => 1,
    };
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|a, b| boxes[*a].0[k].total_cmp(&boxes[*b].0[k]).then(a.cmp(b)));
    let mut offsets = vec![DVec2::ZERO; boxes.len()];
    let mut cursor: Option<f64> = None;
    for i in order {
        let (lo, hi) = boxes[i];
        if let Some(next) = cursor {
            offsets[i][k] = next - lo[k];
        }
        cursor = Some(hi[k] + offsets[i][k] + gap);
    }
    offsets
}

/// The gap that spaces the boxes evenly within their current extent along `axis`: the
/// average of the gaps between neighbours, which is what an even distribution keeps.
pub fn average_gap(boxes: &[(DVec2, DVec2)], axis: Axis) -> Option<f64> {
    if boxes.len() < 2 {
        return None;
    }
    let k = match axis {
        Axis::Horizontal => 0,
        Axis::Vertical => 1,
    };
    let start = boxes.iter().map(|b| b.0[k]).fold(f64::INFINITY, f64::min);
    let end = boxes
        .iter()
        .map(|b| b.1[k])
        .fold(f64::NEG_INFINITY, f64::max);
    let filled: f64 = boxes.iter().map(|b| b.1[k] - b.0[k]).sum();
    Some((end - start - filled) / (boxes.len() - 1) as f64)
}

/// Board position of a material point of panel `index`.
pub fn board_point(template: &GarmentTemplate, index: usize, uv: DVec2) -> DVec2 {
    layout(template, index) + rotate_2d(uv, template.panels[index].layout_rotation_degrees)
}

/// Material position of a board point for panel `index`.
pub fn material_point(template: &GarmentTemplate, index: usize, board: DVec2) -> DVec2 {
    rotate_2d(
        board - layout(template, index),
        -template.panels[index].layout_rotation_degrees,
    )
}

/// Board bounds of panel `index`'s outline, curves flattened.
pub fn board_bounds(template: &GarmentTemplate, index: usize) -> Result<(DVec2, DVec2), String> {
    let panel = &template.panels[index];
    let outline: Vec<DVec2> = match &panel.polygon {
        Some(poly) => (0..poly.points.len())
            .map(|k| poly.edge_points(k))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect(),
        None => vec![
            DVec2::ZERO,
            DVec2::new(panel.pattern.width, 0.),
            DVec2::new(panel.pattern.width, panel.pattern.height),
            DVec2::new(0., panel.pattern.height),
        ],
    };
    let board: Vec<DVec2> = outline
        .into_iter()
        .map(|uv| board_point(template, index, uv))
        .collect();
    bounds(&board).ok_or_else(|| "Empty panel outline".to_string())
}

/// Apply `map` (material frame) to contour points and to the Bézier handles attached to
/// them, so an affine map turns curves with their points.
pub(super) fn transform_anchors(
    panel: &mut Panel,
    indices: &[usize],
    map: impl Fn(DVec2) -> DVec2,
) -> Result<(), String> {
    let polygon = panel
        .polygon
        .as_mut()
        .ok_or("This panel has no editable contour")?;
    let count = polygon.points.len();
    let mut unique = indices.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.is_empty() {
        return Err("Select at least one point".into());
    }
    if unique.iter().any(|k| *k >= count) {
        return Err("Point index out of range".into());
    }
    let mut moved = Vec::with_capacity(unique.len());
    for &k in &unique {
        let before = polygon.points[k];
        polygon.points[k] = map(before);
        moved.push((k, before));
        let leaving = polygon.edges[k].clone();
        if let Some(controls) = polygon.curves.get_mut(&leaving) {
            controls[0] = map(controls[0]);
        }
        let arriving = polygon.edges[(k + count - 1) % count].clone();
        if let Some(controls) = polygon.curves.get_mut(&arriving) {
            controls[1] = map(controls[1]);
        }
    }
    if polygon.points.iter().any(|p| !p.is_finite()) {
        return Err("The transform produced an invalid point".into());
    }
    super::follow_pins(panel, &moved);
    Ok(())
}

/// Scale everything drawn in a panel's material frame (contour, curves, holes, internal
/// lines, pins) about `pivot`.
pub(super) fn scale_panel_material(
    panel: &mut Panel,
    pivot: DVec2,
    scale: DVec2,
) -> Result<(), String> {
    let polygon = panel
        .polygon
        .as_mut()
        .ok_or("Only drafted pieces can be scaled")?;
    let map = |p: DVec2| pivot + (p - pivot) * scale;
    for p in polygon
        .points
        .iter_mut()
        .chain(polygon.curves.values_mut().flatten())
    {
        *p = map(*p);
    }
    for hole in &mut polygon.holes {
        for p in &mut hole.points {
            *p = map(*p);
        }
    }
    for line in &mut polygon.lines {
        for p in &mut line.points {
            *p = map(*p);
        }
        for t in &mut line.tangents {
            *t *= scale;
        }
    }
    for p in &mut panel.pin_points {
        *p = map(*p);
    }
    Ok(())
}

pub(super) fn validate_scale(scale: DVec2) -> Result<(), String> {
    if !scale.is_finite() || scale.min_element() < 0.01 || scale.max_element() > 100. {
        return Err("Scale must be between 1 % and 10 000 %".into());
    }
    Ok(())
}
