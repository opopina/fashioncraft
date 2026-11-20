//! Edits that change a piece's topology, carrying along what points into it. Cut and sew
//! splits a piece in two along a straight internal line carried on to the outline, sews the
//! halves together along the cut and moves every seam, marking and setting to the half it
//! belongs to; a seam that crosses the cut is split in two. Graphics, trims, tack ends and
//! holes go with the half they lie in, and move with their piece when it is merged or
//! unfolded. Anything it cannot carry over unambiguously (curved or annotated cut
//! boundaries, pins, something lying across the cut…) is refused rather than dropped.
use super::*;
use crate::garment::seam::{BoundaryRef, SeamSpec};
use std::collections::{BTreeMap, BTreeSet};

/// Cut points closer than this to a corner, as a fraction of their boundary, are refused.
const CORNER_MARGIN: f64 = 1e-3;

/// Where the line through `a` and `b` meets the outline of `points`, both ways: the boundary
/// index and the fraction along it, for the hits nearest to the segment on either side.
fn cut_ends(points: &[DVec2], a: DVec2, b: DVec2) -> Result<[(usize, f64, DVec2); 2], String> {
    let dir = b - a;
    if dir.length() < 1e-6 {
        return Err("The cut line is too short".into());
    }
    let n = points.len();
    // Hits as (position along the line, boundary, fraction along the boundary, point).
    let mut hits: Vec<(f64, usize, f64, DVec2)> = vec![];
    for k in 0..n {
        let p = points[k];
        let q = points[(k + 1) % n];
        let e = q - p;
        let denom = dir.perp_dot(e);
        if denom.abs() < 1e-12 {
            continue;
        }
        let s = (p - a).perp_dot(e) / denom;
        let t = (p - a).perp_dot(dir) / denom;
        if (0.0..=1.0).contains(&t) {
            hits.push((s, k, t, a + dir * s));
        }
    }
    let before = hits
        .iter()
        .filter(|h| h.0 <= 0.)
        .max_by(|x, y| x.0.total_cmp(&y.0));
    let after = hits
        .iter()
        .filter(|h| h.0 >= 1.)
        .min_by(|x, y| x.0.total_cmp(&y.0));
    match (before, after) {
        (Some(p), Some(q)) => Ok([(p.1, p.2, p.3), (q.1, q.2, q.3)]),
        _ => Err("The cut line must run across the piece from outline to outline".into()),
    }
}

/// Something lying on a piece outside its outline data: a graphic, a trim or one end of a tack.
#[derive(Clone, Copy, Debug)]
enum Carried {
    Graphic(usize),
    Trim(usize),
    TackA(usize),
    TackB(usize),
}

/// What lies on `panel`, each with the material points it covers.
fn carried(template: &GarmentTemplate, panel: &str) -> Vec<(Carried, Vec<DVec2>)> {
    use crate::garment::trims::TrimKind;
    let mut out = vec![];
    for (k, g) in template.graphics.iter().enumerate() {
        if g.panel_id == panel {
            let points = g.corners().map_or(vec![g.center_m], |c| c.to_vec());
            out.push((Carried::Graphic(k), points));
        }
    }
    for (k, t) in template.trims.iter().enumerate() {
        if t.panel_id == panel {
            let points = match &t.kind {
                TrimKind::Button {
                    center_m,
                    diameter_m,
                } => [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y]
                    .map(|d| *center_m + d * (diameter_m / 2.))
                    .to_vec(),
                TrimKind::Buttonhole { start_m, end_m, .. } => vec![*start_m, *end_m],
                TrimKind::Object { center_m, .. } => vec![*center_m],
                TrimKind::Zipper { path_m, .. }
                | TrimKind::Topstitch { path_m, .. }
                | TrimKind::Piping { path_m, .. } => path_m.clone(),
            };
            out.push((Carried::Trim(k), points));
        }
    }
    for (k, t) in template.tacks.iter().enumerate() {
        if t.a.panel == panel {
            out.push((Carried::TackA(k), vec![t.a.uv]));
        }
        if t.b.panel == panel {
            out.push((Carried::TackB(k), vec![t.b.uv]));
        }
    }
    out
}

/// What `item` is, for messages.
fn carried_name(template: &GarmentTemplate, item: Carried) -> String {
    match item {
        Carried::Graphic(k) => format!("Graphic {}", template.graphics[k].id),
        Carried::Trim(k) => format!("Trim {}", template.trims[k].id),
        Carried::TackA(k) | Carried::TackB(k) => format!("Tack {}", template.tacks[k].id),
    }
}

/// Put `item` on piece `to`, its points mapped by `map` and graphics turned by `turn_degrees`.
fn carry(
    template: &mut GarmentTemplate,
    item: Carried,
    to: &str,
    map: impl Fn(DVec2) -> DVec2,
    turn_degrees: f64,
) {
    use crate::garment::trims::TrimKind;
    match item {
        Carried::Graphic(k) => {
            let g = &mut template.graphics[k];
            g.panel_id = to.to_string();
            g.center_m = map(g.center_m);
            if turn_degrees != 0. {
                g.rotation_degrees = (g.rotation_degrees + turn_degrees).rem_euclid(360.);
            }
        }
        Carried::Trim(k) => {
            let t = &mut template.trims[k];
            t.panel_id = to.to_string();
            match &mut t.kind {
                TrimKind::Button { center_m, .. } | TrimKind::Object { center_m, .. } => {
                    *center_m = map(*center_m)
                }
                TrimKind::Buttonhole { start_m, end_m, .. } => {
                    *start_m = map(*start_m);
                    *end_m = map(*end_m);
                }
                TrimKind::Zipper { path_m, .. }
                | TrimKind::Topstitch { path_m, .. }
                | TrimKind::Piping { path_m, .. } => {
                    for p in path_m {
                        *p = map(*p);
                    }
                }
            }
        }
        Carried::TackA(k) => {
            let a = &mut template.tacks[k].a;
            a.panel = to.to_string();
            a.uv = map(a.uv);
        }
        Carried::TackB(k) => {
            let b = &mut template.tacks[k].b;
            b.panel = to.to_string();
            b.uv = map(b.uv);
        }
    }
}

/// The corner `normalize` moves to the origin: the outline's lowest point and handle.
fn outline_low(panel: &Panel) -> DVec2 {
    let poly = panel.polygon.as_ref().expect("a polygon piece");
    poly.points
        .iter()
        .chain(poly.curves.values().flatten())
        .copied()
        .fold(DVec2::splat(f64::INFINITY), DVec2::min)
}

/// Whether `p` lies inside the closed polygon `points` (even–odd rule).
fn inside(points: &[DVec2], p: DVec2) -> bool {
    let n = points.len();
    let mut odd = false;
    for k in 0..n {
        let (a, b) = (points[k], points[(k + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            odd = !odd;
        }
    }
    odd
}

/// The sub-range of a boundary reference covering the seam parameters `s0..s1` (0 at the
/// reference's start, 1 at its end along its direction).
fn sub_range(r: &BoundaryRef, s0: f64, s1: f64) -> BoundaryRef {
    let [lo, hi] = r.range;
    let at = |s: f64| {
        if r.reversed {
            hi - s * (hi - lo)
        } else {
            lo + s * (hi - lo)
        }
    };
    let (x, y) = (at(s0), at(s1));
    BoundaryRef {
        range: [x.min(y), x.max(y)],
        ..r.clone()
    }
}

/// The seam parameter (of side a) where `r`, a side of the seam, crosses fraction `t` of its
/// boundary, if it does so strictly inside its range.
fn crossing(r: &BoundaryRef, t: f64) -> Option<f64> {
    let [lo, hi] = r.range;
    if hi - lo < 1e-9 || t <= lo + 1e-9 || t >= hi - 1e-9 {
        return None;
    }
    let s = (t - lo) / (hi - lo);
    Some(if r.reversed { 1. - s } else { s })
}

/// Split every seam whose single-segment side crosses a cut, so each piece of seam lies on
/// one side of each cut. Seams with tails or interior correspondences there are refused.
fn split_crossing_seams(
    seams: &mut Vec<SeamSpec>,
    panel: &str,
    cuts: &[(&str, f64)],
) -> Result<(), String> {
    for _ in 0..64 {
        let mut split = None;
        'find: for (k, seam) in seams.iter().enumerate() {
            for (on_b, r) in [(false, &seam.a), (true, &seam.b)] {
                if r.panel != panel {
                    continue;
                }
                for &(edge, t) in cuts {
                    if r.edge.as_deref() != Some(edge) {
                        continue;
                    }
                    if let Some(s) = crossing(r, t) {
                        let s = if on_b && seam.reverse_b { 1. - s } else { s };
                        split = Some((k, s));
                        break 'find;
                    }
                }
            }
            let tails = seam.a_tail.iter().chain(&seam.b_tail);
            for r in tails {
                if r.panel == panel
                    && cuts.iter().any(|&(edge, t)| {
                        r.edge.as_deref() == Some(edge) && crossing(r, t).is_some()
                    })
                {
                    return Err(format!(
                        "Seam {} continues across the cut; split it before cutting",
                        seam.id
                    ));
                }
            }
        }
        let Some((k, s)) = split else {
            return Ok(());
        };
        let seam = seams[k].clone();
        if !seam.a_tail.is_empty() || !seam.b_tail.is_empty() || !seam.notches.is_empty() {
            return Err(format!(
                "Seam {} crosses the cut but has tails or matched points; split it first",
                seam.id
            ));
        }
        // Side b runs the other way along the seam when reversed.
        let b_part = |s0: f64, s1: f64| {
            if seam.reverse_b {
                sub_range(&seam.b, 1. - s1, 1. - s0)
            } else {
                sub_range(&seam.b, s0, s1)
            }
        };
        let mut first = seam.clone();
        first.a = sub_range(&seam.a, 0., s);
        first.b = b_part(0., s);
        let mut second = seam.clone();
        second.id = unused_id("seam", seams.iter().map(|s| s.id.clone()));
        second.a = sub_range(&seam.a, s, 1.);
        second.b = b_part(s, 1.);
        seams[k] = first;
        seams.push(second);
    }
    Err("The cut splits too many seams".into())
}

/// A piece split in two along a straight internal line carried on to the outline: each half's
/// outline, markings, lines and holes, with seams, elastic and folds already moved to their half
/// (the second half being piece `b_id`) and what lies on the piece sorted by half.
struct Split {
    source: Panel,
    a_points: Vec<DVec2>,
    a_edges: Vec<String>,
    b_points: Vec<DVec2>,
    b_edges: Vec<String>,
    cut_edge: String,
    a_poly: crate::garment::polygon::PolygonPattern,
    b_poly: crate::garment::polygon::PolygonPattern,
    a_pins: Vec<DVec2>,
    b_pins: Vec<DVec2>,
    /// Each item on the piece and whether it lies on the second half.
    moving: Vec<(Carried, bool)>,
}

/// Split piece `panel` along its straight internal line `line`, carried on to the outline. A
/// runs P → … → Q and B runs Q → … → P, where P is where the line leaves the outline behind its
/// first point and Q beyond its second; both close along the cut, named `cut_edge`.
fn split_along(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
    b_id: &str,
) -> Result<Split, String> {
    let b_id = b_id.to_string();
    let held = template.pin_targets.iter().any(|p| p.panel == panel)
        || template.frozen_panels.iter().any(|p| p.panel == panel)
        || template.solidified_panels.iter().any(|p| p.panel == panel)
        || template.deactivated_panels.iter().any(|p| p.panel == panel);
    if held {
        return Err("Release pins, unfreeze and activate the piece before cutting it".into());
    }
    if template
        .mirror_links
        .iter()
        .any(|l| l.a == panel || l.b == panel)
    {
        return Err("Unlink the mirrored pair before cutting it".into());
    }
    let source = template
        .panels
        .iter()
        .find(|p| p.id == panel)
        .ok_or("Panel not found")?
        .clone();
    let polygon = source
        .polygon
        .as_ref()
        .ok_or("Cutting needs an editable contour")?;

    let cut_line = polygon
        .lines
        .iter()
        .find(|l| l.id == line)
        .ok_or("Internal line not found")?;
    if cut_line.closed
        || cut_line.points.len() != 2
        || cut_line.tangents.iter().any(|t| t.length() > 1e-12)
    {
        return Err("Cut and sew needs a straight internal line with two points".into());
    }
    if template
        .line_springs
        .iter()
        .any(|s| s.panel == panel && s.line == line)
        || template
            .line_folds
            .iter()
            .any(|f| f.panel == panel && f.line == line)
    {
        return Err("Clear the elastic, tape or fold on the line before cutting along it".into());
    }
    let points = &polygon.points;
    let n = points.len();
    let [(i, ti, p), (j, tj, q)] = cut_ends(points, cut_line.points[0], cut_line.points[1])?;
    if i == j {
        return Err("The cut line must leave through another boundary".into());
    }
    if [ti, tj]
        .iter()
        .any(|t| *t < CORNER_MARGIN || *t > 1. - CORNER_MARGIN)
    {
        return Err("Move the cut line off the corners of the piece".into());
    }
    let (ei, ej) = (polygon.edges[i].clone(), polygon.edges[j].clone());
    for e in [&ei, &ej] {
        if polygon.curves.contains_key(e) {
            return Err(format!(
                "The cut crosses the curved boundary {e}; cut across straight boundaries"
            ));
        }
        if polygon.target_lengths.contains_key(e) || polygon.notches.contains_key(e) {
            return Err(format!(
                "The cut crosses boundary {e}, which has a target length or notches; clear them first"
            ));
        }
        if template
            .line_springs
            .iter()
            .any(|s| s.panel == panel && &s.line == e)
        {
            return Err(format!("Clear the elastic or tape on boundary {e} first"));
        }
    }
    let unused = |base: &str, taken: &[&String]| {
        (1..)
            .map(|k| format!("{base}_{k}"))
            .find(|name| !taken.contains(&name) && !polygon.edges.contains(name))
            .unwrap()
    };
    let ei_cut = unused(&format!("{ei}_cut"), &[]);
    let ej_cut = unused(&format!("{ej}_cut"), &[&ei_cut]);
    let cut_edge = unused("cut", &[&ei_cut, &ej_cut]);

    // A runs P → (i+1 … j) → Q, B runs Q → (j+1 … i) → P; both close along the cut.
    let mut a_points = vec![p];
    let mut a_edges = vec![ei_cut.clone()];
    let mut k = (i + 1) % n;
    loop {
        a_points.push(points[k]);
        a_edges.push(if k == j {
            ej.clone()
        } else {
            polygon.edges[k].clone()
        });
        if k == j {
            break;
        }
        k = (k + 1) % n;
    }
    a_points.push(q);
    a_edges.push(cut_edge.clone());
    let mut b_points = vec![q];
    let mut b_edges = vec![ej_cut.clone()];
    let mut k = (j + 1) % n;
    loop {
        b_points.push(points[k]);
        b_edges.push(if k == i {
            ei.clone()
        } else {
            polygon.edges[k].clone()
        });
        if k == i {
            break;
        }
        k = (k + 1) % n;
    }
    b_points.push(p);
    b_edges.push(cut_edge.clone());

    // Holes go with the half they lie in; one across the cut is refused.
    let (mut a_holes, mut b_holes) = (vec![], vec![]);
    for hole in &polygon.holes {
        if hole.points.iter().all(|p| inside(&a_points, *p)) {
            a_holes.push(hole.clone());
        } else if hole.points.iter().all(|p| inside(&b_points, *p)) {
            b_holes.push(hole.clone());
        } else {
            return Err("A hole lies across the cut; move the cut line".into());
        }
    }
    // Which piece each original boundary name now belongs to (the cut parts are added below).
    let mut in_b: BTreeSet<String> = b_edges.iter().cloned().collect();
    in_b.extend(b_holes.iter().flat_map(|h| h.edges.iter().cloned()));

    // Seams: split those crossing the cut, then move every reference to its piece and part.
    split_crossing_seams(&mut template.seams, panel, &[(&ei, ti), (&ej, tj)])?;
    let move_ref = |r: &mut BoundaryRef| {
        if r.panel != panel {
            return;
        }
        let Some(edge) = r.edge.clone() else {
            return;
        };
        let rescale = |r: &mut BoundaryRef, lo: f64, hi: f64| {
            r.range = r.range.map(|x| ((x - lo) / (hi - lo)).clamp(0., 1.));
        };
        if edge == ei {
            if r.range[1] <= ti + 1e-9 {
                r.panel = b_id.clone();
                rescale(r, 0., ti);
            } else {
                r.edge = Some(ei_cut.clone());
                rescale(r, ti, 1.);
            }
        } else if edge == ej {
            if r.range[1] <= tj + 1e-9 {
                rescale(r, 0., tj);
            } else {
                r.panel = b_id.clone();
                r.edge = Some(ej_cut.clone());
                rescale(r, tj, 1.);
            }
        } else if in_b.contains(&edge) {
            r.panel = b_id.clone();
        }
    };
    for seam in &mut template.seams {
        for r in seam.references_mut() {
            move_ref(r);
        }
    }
    // Internal lines go with the piece holding them; a line across the cut is refused.
    let mut a_lines = vec![];
    let mut b_lines = vec![];
    for l in polygon.lines.iter().filter(|l| l.id != line) {
        let samples = l.samples()?;
        let in_a = samples.iter().all(|s| inside(&a_points, *s));
        let in_bb = samples.iter().all(|s| inside(&b_points, *s));
        match (in_a, in_bb) {
            (true, _) => a_lines.push(l.clone()),
            (false, true) => b_lines.push(l.clone()),
            _ => {
                return Err(format!(
                    "Internal line {} crosses the cut; move or delete it first",
                    l.id
                ));
            }
        }
    }
    // Graphics, trims and tack ends go with the half they lie on (a graphic may hang over the
    // outline); one lying on both halves is refused.
    let mut moving = vec![];
    for (item, points) in carried(template, panel) {
        let on_a = points.iter().any(|p| inside(&a_points, *p));
        let on_b = points.iter().any(|p| inside(&b_points, *p));
        if on_a && on_b {
            return Err(format!(
                "{} lies across the cut; move it first",
                carried_name(template, item)
            ));
        }
        moving.push((item, on_b));
    }
    let b_line_ids: BTreeSet<String> = b_lines.iter().map(|l| l.id.clone()).collect();
    for s in &mut template.line_springs {
        if s.panel == panel && (b_line_ids.contains(&s.line) || in_b.contains(&s.line)) {
            s.panel = b_id.clone();
        }
    }
    for f in &mut template.line_folds {
        if f.panel == panel && b_line_ids.contains(&f.line) {
            f.panel = b_id.clone();
        }
    }
    // Per-boundary markings follow their boundary; the two parts of a cut boundary keep its
    // seam allowance.
    let split_maps = |a_names: &[String], b_names: &[String]| {
        let mut a_poly = polygon.clone();
        let mut b_poly = polygon.clone();
        a_poly.points = a_points.clone();
        a_poly.edges = a_names.to_vec();
        a_poly.lines = a_lines.clone();
        a_poly.holes = a_holes.clone();
        b_poly.points = b_points.clone();
        b_poly.edges = b_names.to_vec();
        b_poly.lines = b_lines.clone();
        b_poly.holes = b_holes.clone();
        for (poly, names) in [(&mut a_poly, a_names), (&mut b_poly, b_names)] {
            let rims: Vec<String> = poly.holes.iter().flat_map(|h| h.edges.clone()).collect();
            let keep = |e: &String| names.contains(e) || rims.contains(e);
            poly.curves.retain(|e, _| keep(e));
            poly.target_lengths.retain(|e, _| keep(e));
            poly.notches.retain(|e, _| keep(e));
            let mut allowances: BTreeMap<String, f64> = polygon
                .seam_allowances
                .iter()
                .filter(|(e, _)| keep(e))
                .map(|(e, v)| (e.clone(), *v))
                .collect();
            for (part, whole) in [(&ei_cut, &ei), (&ej_cut, &ej)] {
                if names.contains(part)
                    && let Some(v) = polygon.seam_allowances.get(whole)
                {
                    allowances.insert(part.clone(), *v);
                }
            }
            poly.seam_allowances = allowances;
        }
        (a_poly, b_poly)
    };
    let (a_poly, b_poly) = split_maps(&a_edges, &b_edges);
    // Pinned outline corners stay with the half whose outline holds them.
    let pinned_on = |points: &[DVec2]| -> Vec<DVec2> {
        source
            .pin_points
            .iter()
            .filter(|pin| points.iter().any(|p| p.distance(**pin) < 1e-9))
            .copied()
            .collect()
    };
    let (a_pins, b_pins) = (pinned_on(&a_points), pinned_on(&b_points));
    Ok(Split {
        source: source.clone(),
        a_points,
        a_edges,
        b_points,
        b_edges,
        cut_edge,
        a_poly,
        b_poly,
        a_pins,
        b_pins,
        moving,
    })
}

/// Cut piece `panel` along its straight internal line `line`, carried on to the outline, into
/// two pieces sewn together along the cut. Returns the new piece's id.
pub(super) fn cut_and_sew(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
) -> Result<String, String> {
    let b_id = unused_id("panel", template.panels.iter().map(|p| p.id.clone()));
    let Split {
        source,
        a_poly,
        b_poly,
        a_pins,
        b_pins,
        cut_edge,
        moving,
        ..
    } = split_along(template, panel, line, &b_id)?;
    let mut a_panel = source.clone();
    a_panel.polygon = Some(a_poly);
    a_panel.pin_points = a_pins;
    let mut b_panel = source.clone();
    b_panel.id = b_id.clone();
    b_panel.name = format!("{} 2", source.name);
    b_panel.polygon = Some(b_poly);
    b_panel.pin_points = b_pins;
    let (lo_a, lo_b) = (outline_low(&a_panel), outline_low(&b_panel));
    normalize(&mut a_panel);
    normalize(&mut b_panel);
    let index = template
        .panels
        .iter()
        .position(|p| p.id == panel)
        .expect("panel found above");
    template.panels[index] = a_panel;
    template.panels.insert(index + 1, b_panel);
    // Per-piece settings apply to both halves.
    if let Some(v) = template.panel_layers.get(panel).copied() {
        template.panel_layers.insert(b_id.clone(), v);
    }
    if let Some(v) = template.panel_pressure.get(panel).copied() {
        template.panel_pressure.insert(b_id.clone(), v);
    }
    if let Some(v) = template.panel_shrink.get(panel).copied() {
        template.panel_shrink.insert(b_id.clone(), v);
    }
    if let Some(v) = template.panel_wraps.get(panel).cloned() {
        template.panel_wraps.insert(b_id.clone(), v);
    }
    for set in [
        &mut template.flipped_panels,
        &mut template.strengthened_panels,
        &mut template.locked_panels,
    ] {
        if set.contains(panel) {
            set.insert(b_id.clone());
        }
    }
    // Sew the halves along the cut: A runs Q → P there, B runs P → Q.
    let seam_id = unused_id("seam", template.seams.iter().map(|s| s.id.clone()));
    let side = |piece: &str| BoundaryRef {
        reversed: false,
        panel: piece.to_string(),
        side: crate::garment::seam::Side::Top,
        edge: Some(cut_edge.clone()),
        range: [0., 1.],
    };
    template.seams.push(SeamSpec {
        id: seam_id,
        a: side(panel),
        b: side(&b_id),
        a_tail: vec![],
        b_tail: vec![],
        reverse_b: true,
        notches: vec![],
        stiffness: 50_000.,
        allow_ease: false,
    });
    for (item, to_b) in moving {
        if to_b {
            carry(template, item, &b_id, |p| p - lo_b, 0.);
        } else {
            carry(template, item, panel, |p| p - lo_a, 0.);
        }
    }
    Ok(b_id)
}

/// Slash piece `panel` along its straight internal line `line`, carried on to the outline, and
/// turn one part (or both, by half each) about the hinge where the line leaves the outline
/// beyond its second point, so a wedge of `degrees` opens where it leaves behind its first
/// point. The parts turn apart rather than overlap. Everything on each part turns with it;
/// seams keep their boundaries, and the wedge's mouth is a new boundary.
pub(super) fn slash_spread(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
    degrees: f64,
    side: super::SpreadSide,
) -> Result<(), String> {
    if !degrees.is_finite() || !(0.1..=90.).contains(&degrees.abs()) {
        return Err("Open the wedge by 0.1° to 90°".into());
    }
    let (first, second) = {
        let l = template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|poly| poly.lines.iter().find(|l| l.id == line))
            .ok_or("Internal line not found")?;
        (l.points[0], *l.points.last().unwrap())
    };
    let Split {
        source,
        a_points,
        a_edges,
        b_points,
        b_edges,
        cut_edge,
        a_poly,
        b_poly,
        a_pins,
        b_pins,
        moving,
    } = split_along(template, panel, line, panel)?;
    let hinge = *a_points.last().unwrap();
    // How far each part turns, by which side of the line it lies on as seen on the board,
    // which shows material y downwards.
    let centre = a_points.iter().copied().sum::<DVec2>() / a_points.len() as f64;
    let a_left = (second - first).perp_dot(centre - first) < 0.;
    let half = degrees.abs().to_radians() / 2.;
    let (left, right) = match side {
        super::SpreadSide::Left => (2. * half, 0.),
        super::SpreadSide::Right => (0., 2. * half),
        super::SpreadSide::Both => (half, half),
    };
    let (turn_a, turn_b) = if a_left { (left, right) } else { (right, left) };
    let turned = |x: DVec2, angle: f64| hinge + DVec2::from_angle(angle).rotate(x - hinge);
    // The parts turn apart, opening the piece: of the two senses, the one with more area.
    let outline_for = |sign: f64| -> Vec<DVec2> {
        let (ra, rb) = (-sign * turn_a, sign * turn_b);
        a_points
            .iter()
            .map(|x| turned(*x, ra))
            .chain(b_points[1..].iter().map(|x| turned(*x, rb)))
            .collect()
    };
    let sign = if signed_area(&outline_for(1.)).abs() >= signed_area(&outline_for(-1.)).abs() {
        1.
    } else {
        -1.
    };
    let (ra, rb) = (-sign * turn_a, sign * turn_b);
    let mut poly = a_poly.clone();
    poly.points = outline_for(sign);
    poly.edges = a_edges[..a_edges.len() - 1]
        .iter()
        .chain(&b_edges[..b_edges.len() - 1])
        .cloned()
        .chain(std::iter::once(cut_edge))
        .collect();
    poly.curves = BTreeMap::new();
    for (part, angle) in [(&a_poly, ra), (&b_poly, rb)] {
        for (e, c) in &part.curves {
            poly.curves.insert(e.clone(), c.map(|x| turned(x, angle)));
        }
    }
    // Per-boundary markings of both parts (the first part's are there already).
    poly.target_lengths.extend(b_poly.target_lengths.clone());
    poly.notches.extend(b_poly.notches.clone());
    poly.seam_allowances.extend(b_poly.seam_allowances.clone());
    let rotation = |angle: f64| DVec2::from_angle(angle);
    poly.lines = vec![];
    poly.holes = vec![];
    for (part, angle) in [(&a_poly, ra), (&b_poly, rb)] {
        for l in &part.lines {
            let mut l = l.clone();
            l.points = l.points.iter().map(|x| turned(*x, angle)).collect();
            l.tangents = l
                .tangents
                .iter()
                .map(|v| rotation(angle).rotate(*v))
                .collect();
            poly.lines.push(l);
        }
        for h in &part.holes {
            let mut h = h.clone();
            h.points = h.points.iter().map(|x| turned(*x, angle)).collect();
            poly.holes.push(h);
        }
    }
    let mut piece = source.clone();
    piece.polygon = Some(poly);
    piece.pin_points = a_pins
        .iter()
        .map(|x| turned(*x, ra))
        .chain(b_pins.iter().map(|x| turned(*x, rb)))
        .collect();
    let lo = outline_low(&piece);
    normalize(&mut piece);
    let index = template
        .panels
        .iter()
        .position(|p| p.id == panel)
        .expect("panel found above");
    template.panels[index] = piece;
    for (item, on_b) in moving {
        let angle = if on_b { rb } else { ra };
        carry(
            template,
            item,
            panel,
            |x| turned(x, angle) - lo,
            angle.to_degrees(),
        );
    }
    Ok(())
}

/// Signed area of a closed polygon (positive counter-clockwise).
fn signed_area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| points[k].perp_dot(points[(k + 1) % n]))
        .sum::<f64>()
        / 2.
}

/// Merge the two pieces joined edge to edge by seam `seam_id` into one: the second piece is
/// laid against the first along the seam, the seam disappears and everything that pointed
/// into the second piece now points into the first. Returns the kept piece's id.
pub(super) fn merge_along_seam(
    template: &mut GarmentTemplate,
    seam_id: &str,
) -> Result<String, String> {
    let seam = template
        .seams
        .iter()
        .find(|s| s.id == seam_id)
        .ok_or("Seam not found")?
        .clone();
    let (a_id, b_id) = (seam.a.panel.clone(), seam.b.panel.clone());
    if a_id == b_id {
        return Err("This seam joins a piece to itself".into());
    }
    let whole = |r: &BoundaryRef| r.range == [0., 1.] && !r.reversed && r.edge.is_some();
    if !seam.a_tail.is_empty()
        || !seam.b_tail.is_empty()
        || !seam.notches.is_empty()
        || seam.allow_ease
        || !whole(&seam.a)
        || !whole(&seam.b)
    {
        return Err(
            "Merging needs a seam that joins two whole boundaries edge to edge, without ease"
                .into(),
        );
    }
    if !seam.reverse_b {
        return Err(
            "The pieces face opposite ways along this seam; flip one before merging".into(),
        );
    }
    for id in [&a_id, &b_id] {
        let held = template.pin_targets.iter().any(|p| &p.panel == id)
            || template.frozen_panels.iter().any(|p| &p.panel == id)
            || template.solidified_panels.iter().any(|p| &p.panel == id)
            || template.deactivated_panels.iter().any(|p| &p.panel == id);
        if held {
            return Err("Release pins, unfreeze and activate both pieces before merging".into());
        }
        if template
            .mirror_links
            .iter()
            .any(|l| &l.a == id || &l.b == id)
        {
            return Err("Unlink mirrored pieces before merging".into());
        }
    }
    // What lies on either piece moves with it into the merged one.
    let on_a: Vec<Carried> = carried(template, &a_id).into_iter().map(|c| c.0).collect();
    let on_b: Vec<Carried> = carried(template, &b_id).into_iter().map(|c| c.0).collect();
    let same = |a: Option<String>, b: Option<String>| a == b;
    let fmt = |v: Option<&[f64; 2]>| v.map(|v| format!("{v:?}"));
    if !same(
        template.panel_layers.get(&a_id).map(|v| v.to_string()),
        template.panel_layers.get(&b_id).map(|v| v.to_string()),
    ) || !same(
        template.panel_pressure.get(&a_id).map(|v| v.to_string()),
        template.panel_pressure.get(&b_id).map(|v| v.to_string()),
    ) || !same(
        fmt(template.panel_shrink.get(&a_id)),
        fmt(template.panel_shrink.get(&b_id)),
    ) || template.flipped_panels.contains(&a_id) != template.flipped_panels.contains(&b_id)
        || template.strengthened_panels.contains(&a_id)
            != template.strengthened_panels.contains(&b_id)
    {
        return Err("The pieces have different simulation settings; match them first".into());
    }
    let a = template
        .panels
        .iter()
        .find(|p| p.id == a_id)
        .ok_or("Panel not found")?
        .clone();
    let b = template
        .panels
        .iter()
        .find(|p| p.id == b_id)
        .ok_or("Panel not found")?
        .clone();
    if a.fabric != b.fabric {
        return Err("The pieces are of different fabrics".into());
    }
    let (pa, pb) = (
        a.polygon
            .as_ref()
            .ok_or("Merging needs editable contours")?,
        b.polygon
            .as_ref()
            .ok_or("Merging needs editable contours")?,
    );
    let (ea, eb) = (seam.a.edge.clone().unwrap(), seam.b.edge.clone().unwrap());
    let ia = pa
        .edges
        .iter()
        .position(|e| e == &ea)
        .ok_or("Seam boundary not found")?;
    let ib = pb
        .edges
        .iter()
        .position(|e| e == &eb)
        .ok_or("Seam boundary not found")?;
    for (poly, e) in [(pa, &ea), (pb, &eb)] {
        if poly.curves.contains_key(e)
            || poly.target_lengths.contains_key(e)
            || poly.notches.contains_key(e)
        {
            return Err(format!(
                "Boundary {e} is curved or has a target length or notches; merging needs a plain straight seam"
            ));
        }
    }
    let others = template.seams.iter().any(|s| {
        s.id != seam.id
            && s.references().any(|r| {
                (r.panel == a_id && r.edge.as_deref() == Some(&ea))
                    || (r.panel == b_id && r.edge.as_deref() == Some(&eb))
            })
    });
    if others {
        return Err("Another seam uses the merged boundaries; delete it first".into());
    }
    let (na, nb) = (pa.points.len(), pb.points.len());
    if signed_area(&pa.points).signum() != signed_area(&pb.points).signum() {
        return Err("The pieces wind opposite ways; flip one before merging".into());
    }
    let (a0, a1) = (pa.points[ia], pa.points[(ia + 1) % na]);
    let (b0, b1) = (pb.points[ib], pb.points[(ib + 1) % nb]);
    if ((a1 - a0).length() - (b1 - b0).length()).abs() > 5e-4 {
        return Err("The sewn boundaries differ in length by more than 0.5 mm".into());
    }
    // B's edge start meets A's edge end and the other way round.
    let angle = (a0 - a1).to_angle() - (b1 - b0).to_angle();
    let rot = DVec2::from_angle(angle);
    let place = |x: DVec2| a1 + rot.rotate(x - b0);
    let grain = |d: f64| d.rem_euclid(360.);
    if (grain(b.grain_degrees + angle.to_degrees()) - grain(a.grain_degrees)).abs() > 0.5
        && (grain(b.grain_degrees + angle.to_degrees()) - grain(a.grain_degrees)).abs() < 359.5
    {
        return Err("The grain lines would not match once merged".into());
    }
    // B's names that clash with A's get a suffix.
    let mut rename: BTreeMap<String, String> = BTreeMap::new();
    let mut taken: BTreeSet<String> = pa.edges.iter().cloned().collect();
    taken.extend(pa.lines.iter().map(|l| l.id.clone()));
    taken.extend(pa.holes.iter().flat_map(|h| h.edges.iter().cloned()));
    for name in pb
        .edges
        .iter()
        .chain(pb.lines.iter().map(|l| &l.id))
        .chain(pb.holes.iter().flat_map(|h| h.edges.iter()))
    {
        if name == &eb {
            continue;
        }
        let fresh = if taken.contains(name) {
            (2..)
                .map(|k| format!("{name}_{k}"))
                .find(|c| !taken.contains(c))
                .unwrap()
        } else {
            name.clone()
        };
        taken.insert(fresh.clone());
        rename.insert(name.clone(), fresh);
    }
    let mut merged = pa.clone();
    merged.points = (0..na).map(|k| pa.points[(ia + 1 + k) % na]).collect();
    merged.edges = (0..na - 1)
        .map(|k| pa.edges[(ia + 1 + k) % na].clone())
        .collect();
    for k in 0..nb - 2 {
        merged.points.push(place(pb.points[(ib + 2 + k) % nb]));
    }
    for k in 0..nb - 1 {
        merged
            .edges
            .push(rename[&pb.edges[(ib + 1 + k) % nb]].clone());
    }
    merged.curves.remove(&ea);
    merged.target_lengths.remove(&ea);
    merged.seam_allowances.remove(&ea);
    merged.notches.remove(&ea);
    for (e, c) in &pb.curves {
        merged.curves.insert(rename[e].clone(), c.map(place));
    }
    for (e, v) in &pb.target_lengths {
        merged.target_lengths.insert(rename[e].clone(), *v);
    }
    for (e, v) in &pb.seam_allowances {
        if e != &eb {
            merged.seam_allowances.insert(rename[e].clone(), *v);
        }
    }
    for (e, v) in &pb.notches {
        merged.notches.insert(rename[e].clone(), v.clone());
    }
    for l in &pb.lines {
        let mut l = l.clone();
        l.id = rename[&l.id].clone();
        l.points = l.points.iter().map(|p| place(*p)).collect();
        l.tangents = l.tangents.iter().map(|t| rot.rotate(*t)).collect();
        merged.lines.push(l);
    }
    for hole in &pb.holes {
        let mut hole = hole.clone();
        hole.points = hole.points.iter().map(|p| place(*p)).collect();
        hole.edges = hole.edges.iter().map(|e| rename[e].clone()).collect();
        merged.holes.push(hole);
    }
    let mut kept = a.clone();
    kept.polygon = Some(merged);
    for pin in &b.pin_points {
        let pin = place(*pin);
        if !kept.pin_points.iter().any(|p| p.distance(pin) < 1e-9) {
            kept.pin_points.push(pin);
        }
    }
    let lo = outline_low(&kept);
    normalize(&mut kept);
    for item in on_a {
        carry(template, item, &a_id, |p| p - lo, 0.);
    }
    for item in on_b {
        carry(template, item, &a_id, |p| place(p) - lo, angle.to_degrees());
    }
    let index = template.panels.iter().position(|p| p.id == a_id).unwrap();
    template.panels[index] = kept;
    template.panels.retain(|p| p.id != b_id);
    template.seams.retain(|s| s.id != seam.id);
    for s in &mut template.seams {
        for r in s.references_mut() {
            if r.panel == b_id {
                r.panel = a_id.clone();
                if let Some(e) = r.edge.as_mut() {
                    *e = rename.get(e).cloned().unwrap_or(e.clone());
                }
            }
        }
    }
    for s in &mut template.line_springs {
        if s.panel == b_id {
            s.panel = a_id.clone();
            s.line = rename.get(&s.line).cloned().unwrap_or(s.line.clone());
        }
    }
    for f in &mut template.line_folds {
        if f.panel == b_id {
            f.panel = a_id.clone();
            f.line = rename.get(&f.line).cloned().unwrap_or(f.line.clone());
        }
    }
    template.panel_layers.remove(&b_id);
    template.panel_pressure.remove(&b_id);
    template.panel_shrink.remove(&b_id);
    template.panel_wraps.remove(&b_id);
    template.flipped_panels.remove(&b_id);
    template.strengthened_panels.remove(&b_id);
    template.locked_panels.remove(&b_id);
    Ok(a_id)
}

/// Unfold a half piece across its straight boundary `edge`: the piece gains its mirror image
/// on the other side of that boundary, which disappears inside the whole piece. Boundaries,
/// curves, markings and internal lines are mirrored with `_mirror` names; elastic, tape and
/// folds on mirrored lines are copied. Seams on the fold, or pieces it cannot carry over, are
/// refused.
pub(super) fn unfold(
    template: &mut GarmentTemplate,
    panel: &str,
    edge: &str,
) -> Result<(), String> {
    let held = template.pin_targets.iter().any(|p| p.panel == panel)
        || template.frozen_panels.iter().any(|p| p.panel == panel)
        || template.solidified_panels.iter().any(|p| p.panel == panel)
        || template.deactivated_panels.iter().any(|p| p.panel == panel);
    if held {
        return Err("Release pins, unfreeze and activate the piece before unfolding it".into());
    }
    if template
        .mirror_links
        .iter()
        .any(|l| l.a == panel || l.b == panel)
    {
        return Err("Unlink the mirrored pair before unfolding".into());
    }
    if template.seams.iter().any(|s| {
        s.references()
            .any(|r| r.panel == panel && r.edge.as_deref() == Some(edge))
    }) {
        return Err("The fold boundary is sewn; delete its seam before unfolding".into());
    }
    let index = template
        .panels
        .iter()
        .position(|p| p.id == panel)
        .ok_or("Panel not found")?;
    let source = template.panels[index].clone();
    let polygon = source
        .polygon
        .as_ref()
        .ok_or("Unfolding needs an editable contour")?;
    let n = polygon.points.len();
    let i = polygon
        .edges
        .iter()
        .position(|e| e == edge)
        .ok_or("Boundary not found")?;
    if polygon.curves.contains_key(edge) {
        return Err("Unfold across a straight boundary".into());
    }
    if polygon.target_lengths.contains_key(edge) || polygon.notches.contains_key(edge) {
        return Err("The fold boundary has a target length or notches; clear them first".into());
    }
    if template
        .line_springs
        .iter()
        .any(|s| s.panel == panel && s.line == edge)
    {
        return Err("Clear the elastic or tape on the fold boundary first".into());
    }
    let (p0, p1) = (polygon.points[i], polygon.points[(i + 1) % n]);
    let d = (p1 - p0).normalize_or_zero();
    if d == DVec2::ZERO {
        return Err("The fold boundary has no length".into());
    }
    let reflect_v = |v: DVec2| 2. * v.dot(d) * d - v;
    let reflect = |x: DVec2| p0 + reflect_v(x - p0);
    let mut taken: BTreeSet<String> = polygon.edges.iter().cloned().collect();
    taken.extend(polygon.lines.iter().map(|l| l.id.clone()));
    taken.extend(polygon.holes.iter().flat_map(|h| h.edges.iter().cloned()));
    let mut mirror_name = BTreeMap::new();
    for name in polygon
        .edges
        .iter()
        .chain(polygon.lines.iter().map(|l| &l.id))
        .chain(polygon.holes.iter().flat_map(|h| h.edges.iter()))
    {
        let base = format!("{name}_mirror");
        let fresh = if taken.contains(&base) {
            (2..)
                .map(|k| format!("{base}_{k}"))
                .find(|c| !taken.contains(c))
                .unwrap()
        } else {
            base
        };
        taken.insert(fresh.clone());
        mirror_name.insert(name.clone(), fresh);
    }
    let mut whole = polygon.clone();
    whole.points = (0..n).map(|k| polygon.points[(i + 1 + k) % n]).collect();
    whole.edges = (0..n - 1)
        .map(|k| polygon.edges[(i + 1 + k) % n].clone())
        .collect();
    // Walk the mirror image backwards from the fold's start to its end.
    for k in 1..n - 1 {
        whole.points.push(reflect(polygon.points[(i + n - k) % n]));
    }
    for k in 1..n {
        let original = &polygon.edges[(i + n - k) % n];
        whole.edges.push(mirror_name[original].clone());
    }
    whole.seam_allowances.remove(edge);
    for (e, c) in &polygon.curves {
        whole
            .curves
            .insert(mirror_name[e].clone(), [reflect(c[1]), reflect(c[0])]);
    }
    for (e, v) in &polygon.target_lengths {
        whole.target_lengths.insert(mirror_name[e].clone(), *v);
    }
    for (e, v) in &polygon.seam_allowances {
        if e != edge {
            whole.seam_allowances.insert(mirror_name[e].clone(), *v);
        }
    }
    for (e, notches) in &polygon.notches {
        let flipped = notches
            .iter()
            .map(|notch| {
                let mut notch = *notch;
                notch.at = 1. - notch.at;
                notch
            })
            .collect();
        whole.notches.insert(mirror_name[e].clone(), flipped);
    }
    for l in &polygon.lines {
        let mut m = l.clone();
        m.id = mirror_name[&l.id].clone();
        m.points = l.points.iter().map(|p| reflect(*p)).collect();
        m.tangents = l.tangents.iter().map(|t| reflect_v(*t)).collect();
        if let Some(group) = &l.group {
            m.group = Some(format!("{group}_mirror"));
        }
        whole.lines.push(m);
    }
    // Each hole gains its mirror image, walked backwards to keep its winding.
    for hole in &polygon.holes {
        let m = hole.points.len();
        whole.holes.push(crate::garment::domain::Hole {
            points: (0..m).map(|j| reflect(hole.points[(m - j) % m])).collect(),
            edges: (0..m)
                .map(|j| mirror_name[&hole.edges[m - 1 - j]].clone())
                .collect(),
        });
    }
    let springs: Vec<_> = template
        .line_springs
        .iter()
        .filter(|s| s.panel == panel)
        .filter_map(|s| {
            let mut copy = s.clone();
            copy.line = mirror_name.get(&s.line)?.clone();
            Some(copy)
        })
        .collect();
    template.line_springs.extend(springs);
    let folds: Vec<_> = template
        .line_folds
        .iter()
        .filter(|f| f.panel == panel)
        .filter_map(|f| {
            let mut copy = f.clone();
            copy.line = mirror_name.get(&f.line)?.clone();
            Some(copy)
        })
        .collect();
    template.line_folds.extend(folds);
    let mut piece = source.clone();
    piece.polygon = Some(whole);
    let lo = outline_low(&piece);
    normalize(&mut piece);
    template.panels[index] = piece;
    // Graphics, trims and tack ends stay where they were on the original half.
    for (item, _) in carried(template, panel) {
        carry(template, item, panel, |p| p - lo, 0.);
    }
    Ok(())
}

/// Refuse to reshape an internal line that a seam sews.
fn line_unsewn(template: &GarmentTemplate, panel: &str, line: &str) -> Result<(), String> {
    if template.seams.iter().any(|s| {
        s.references()
            .any(|r| r.panel == panel && r.edge.as_deref() == Some(line))
    }) {
        return Err("This line is sewn; delete its seam before changing it".into());
    }
    Ok(())
}

/// Carry one end of internal line `line` on straight up to the outline or the nearest line.
pub(super) fn extend_line(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
    at_start: bool,
) -> Result<(), String> {
    line_unsewn(template, panel, line)?;
    let polygon = template
        .panels
        .iter_mut()
        .find(|p| p.id == panel)
        .ok_or("Panel not found")?
        .polygon
        .as_mut()
        .ok_or("This panel has no editable contour")?;
    let end = crate::garment::lines2d::extended_end(polygon, line, at_start)?;
    let l = polygon
        .lines
        .iter_mut()
        .find(|l| l.id == line)
        .ok_or("Internal line not found")?;
    let k = if at_start { 0 } else { l.points.len() - 1 };
    l.points[k] = end;
    l.samples()?;
    Ok(())
}

/// Split open internal line `line` in two at its inner point `point`: the first part keeps
/// the name, the second gets a new one, and they stop 0.25 mm short of the point on either
/// side, since internal lines may not touch; elastic, tape and folds apply to both parts.
pub(super) fn split_line(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
    point: usize,
) -> Result<(), String> {
    line_unsewn(template, panel, line)?;
    let polygon = template
        .panels
        .iter_mut()
        .find(|p| p.id == panel)
        .ok_or("Panel not found")?
        .polygon
        .as_mut()
        .ok_or("This panel has no editable contour")?;
    let taken: BTreeSet<String> = polygon
        .edges
        .iter()
        .chain(polygon.lines.iter().map(|l| &l.id))
        .cloned()
        .collect();
    let index = polygon
        .lines
        .iter()
        .position(|l| l.id == line)
        .ok_or("Internal line not found")?;
    let l = polygon.lines[index].clone();
    if l.closed {
        return Err("Split an open line; a closed shape has no ends".into());
    }
    if point == 0 || point + 1 >= l.points.len() {
        return Err("Split the line at one of its inner points".into());
    }
    let fresh = (2..)
        .map(|k| format!("{line}_{k}"))
        .find(|c| !taken.contains(c))
        .unwrap();
    // Lines may not touch: the parts stop half the outline gap short of the split point.
    let half_gap = crate::garment::lines2d::OUTLINE_GAP_M / 2.;
    let at = l.points[point];
    let (before, after) = (l.points[point - 1], l.points[point + 1]);
    if at.distance(before) < 4. * half_gap || at.distance(after) < 4. * half_gap {
        return Err("The segments beside the split point are too short".into());
    }
    let mut first = l.clone();
    first.points.truncate(point + 1);
    first.tangents.truncate(point + 1);
    first.points[point] = at + (before - at).normalize() * half_gap;
    first.tangents[point] = DVec2::ZERO;
    let mut second = l.clone();
    second.id = fresh.clone();
    second.points.drain(..point);
    second.tangents.drain(..point);
    second.points[0] = at + (after - at).normalize() * half_gap;
    second.tangents[0] = DVec2::ZERO;
    first.samples()?;
    second.samples()?;
    polygon.lines[index] = first;
    polygon.lines.insert(index + 1, second);
    let springs: Vec<_> = template
        .line_springs
        .iter()
        .filter(|s| s.panel == panel && s.line == line)
        .map(|s| {
            let mut copy = s.clone();
            copy.line = fresh.clone();
            copy
        })
        .collect();
    template.line_springs.extend(springs);
    let folds: Vec<_> = template
        .line_folds
        .iter()
        .filter(|f| f.panel == panel && f.line == line)
        .map(|f| {
            let mut copy = f.clone();
            copy.line = fresh.clone();
            copy
        })
        .collect();
    template.line_folds.extend(folds);
    Ok(())
}

/// Remove segment `segment` (from point `segment` to the next) of internal line `line` on
/// `panel`. An open line keeps the parts on either side, the second under a fresh name with the
/// line's elastic, tape and folds; a closed shape opens there into one line.
pub(super) fn trim_line(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
    segment: usize,
) -> Result<(), String> {
    line_unsewn(template, panel, line)?;
    let polygon = template
        .panels
        .iter_mut()
        .find(|p| p.id == panel)
        .ok_or("Panel not found")?
        .polygon
        .as_mut()
        .ok_or("This panel has no editable contour")?;
    let taken: BTreeSet<String> = polygon
        .edges
        .iter()
        .chain(polygon.lines.iter().map(|l| &l.id))
        .chain(polygon.holes.iter().flat_map(|h| h.edges.iter()))
        .cloned()
        .collect();
    let index = polygon
        .lines
        .iter()
        .position(|l| l.id == line)
        .ok_or("Internal line not found")?;
    let l = polygon.lines[index].clone();
    let n = l.points.len();
    if l.closed {
        if n < 3 || segment >= n {
            return Err("Pick one of the shape's segments".into());
        }
        // The shape opens at the segment: it now runs from the segment's end round to its start.
        let order: Vec<usize> = (0..n).map(|k| (segment + 1 + k) % n).collect();
        let mut open = l.clone();
        open.closed = false;
        open.points = order.iter().map(|&k| l.points[k]).collect();
        open.tangents = order.iter().map(|&k| l.tangents[k]).collect();
        open.samples()?;
        polygon.lines[index] = open;
        return Ok(());
    }
    if n < 3 {
        return Err("A line with one segment is deleted, not trimmed".into());
    }
    if segment + 1 >= n {
        return Err("Pick one of the line's segments".into());
    }
    let mut first = l.clone();
    first.points.truncate(segment + 1);
    first.tangents.truncate(segment + 1);
    let mut second = l.clone();
    second.points.drain(..=segment);
    second.tangents.drain(..=segment);
    // An end segment leaves one part, which keeps the line's name.
    let parts: Vec<crate::garment::domain::InternalLine> = [first, second]
        .into_iter()
        .filter(|p| p.points.len() >= 2)
        .collect();
    for p in &parts {
        p.samples()?;
    }
    if parts.len() == 1 {
        let mut only = parts.into_iter().next().unwrap();
        only.id = line.to_string();
        polygon.lines[index] = only;
        return Ok(());
    }
    let fresh = (2..)
        .map(|k| format!("{line}_{k}"))
        .find(|c| !taken.contains(c))
        .unwrap();
    let mut parts = parts.into_iter();
    polygon.lines[index] = parts.next().unwrap();
    let mut second = parts.next().unwrap();
    second.id = fresh.clone();
    polygon.lines.insert(index + 1, second);
    let springs: Vec<_> = template
        .line_springs
        .iter()
        .filter(|s| s.panel == panel && s.line == line)
        .map(|s| {
            let mut copy = s.clone();
            copy.line = fresh.clone();
            copy
        })
        .collect();
    template.line_springs.extend(springs);
    let folds: Vec<_> = template
        .line_folds
        .iter()
        .filter(|f| f.panel == panel && f.line == line)
        .map(|f| {
            let mut copy = f.clone();
            copy.line = fresh.clone();
            copy
        })
        .collect();
    template.line_folds.extend(folds);
    Ok(())
}

/// Add a diamond dart to `panel`: a diamond-shaped hole `width × height` (along the material
/// axes) centred on `center`, whose two upper legs are sewn together and whose two lower legs
/// are sewn together, so the hole closes into a dart along its height. Returns the hole's name.
pub(super) fn diamond_dart(
    template: &mut GarmentTemplate,
    panel: &str,
    center: DVec2,
    width: f64,
    height: f64,
) -> Result<String, String> {
    if !center.is_finite()
        || !(0.002..=0.2).contains(&width)
        || !(0.01..=1.0).contains(&height)
        || height < 2. * width
    {
        return Err(
            "A diamond dart needs a 2–200 mm intake and a height of at least twice it, up to 1 m"
                .into(),
        );
    }
    let polygon = template
        .panels
        .iter_mut()
        .find(|p| p.id == panel)
        .ok_or("Panel not found")?
        .polygon
        .as_mut()
        .ok_or("This panel has no editable contour")?;
    let taken: BTreeSet<String> = polygon.paths()?.into_iter().map(|(n, _)| n).collect();
    let base = (1..)
        .map(|k| format!("diamond_{k}"))
        .find(|b| (1..=4).all(|i| !taken.contains(&format!("{b}_{i}"))))
        .unwrap();
    let (w, h) = (width / 2., height / 2.);
    let mut points = vec![
        center + DVec2::new(0., -h),
        center + DVec2::new(w, 0.),
        center + DVec2::new(0., h),
        center + DVec2::new(-w, 0.),
    ];
    // Holes run clockwise (negative area); the apexes are the points on the long axis.
    if signed_area(&points) > 0. {
        points.reverse();
    }
    let apexes = [center + DVec2::new(0., -h), center + DVec2::new(0., h)];
    let edges: Vec<String> = (1..=4).map(|i| format!("{base}_{i}")).collect();
    polygon.holes.push(crate::garment::domain::Hole {
        points: points.clone(),
        edges: edges.clone(),
    });
    // For each apex, the leg arriving at it is sewn to the leg leaving it, far ends together.
    for apex in apexes {
        let k = (0..4)
            .find(|&k| points[(k + 1) % 4].distance(apex) < 1e-12)
            .expect("the apex is a corner");
        let side = |edge: &String| BoundaryRef {
            reversed: false,
            panel: panel.to_string(),
            side: crate::garment::seam::Side::Top,
            edge: Some(edge.clone()),
            range: [0., 1.],
        };
        let id = unused_id("seam", template.seams.iter().map(|s| s.id.clone()));
        template.seams.push(SeamSpec {
            id,
            a: side(&edges[k]),
            b: side(&edges[(k + 1) % 4]),
            a_tail: vec![],
            b_tail: vec![],
            reverse_b: true,
            notches: vec![],
            stiffness: 50_000.,
            allow_ease: false,
        });
    }
    Ok(base)
}

/// Replace piece `panel`'s outline with its closed internal shape `line`. Internal lines,
/// holes, graphics, trims, tack ends and pinned corners inside the new outline stay where they
/// are on the board; anything that would end up outside it, or a seam, elastic or fold on the
/// old outline or the shape, is refused rather than dropped.
pub(super) fn replace_outline(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
) -> Result<(), String> {
    let held = template.pin_targets.iter().any(|p| p.panel == panel)
        || template.frozen_panels.iter().any(|p| p.panel == panel)
        || template.solidified_panels.iter().any(|p| p.panel == panel)
        || template.deactivated_panels.iter().any(|p| p.panel == panel);
    if held {
        return Err(
            "Release pins, unfreeze and activate the piece before replacing its outline".into(),
        );
    }
    if template
        .mirror_links
        .iter()
        .any(|l| l.a == panel || l.b == panel)
    {
        return Err("Unlink the mirrored pair before replacing the outline".into());
    }
    let index = template
        .panels
        .iter()
        .position(|p| p.id == panel)
        .ok_or("Panel not found")?;
    let source = template.panels[index].clone();
    let polygon = source
        .polygon
        .as_ref()
        .ok_or("Replacing the outline needs an editable contour")?;
    let shape = polygon
        .lines
        .iter()
        .find(|l| l.id == line)
        .ok_or("Internal shape not found")?;
    if !shape.closed {
        return Err("Only a closed internal shape can become the outline".into());
    }
    let gone = |name: &str| name == line || polygon.edges.iter().any(|e| e == name);
    if template.seams.iter().any(|s| {
        s.references()
            .any(|r| r.panel == panel && r.edge.as_deref().is_some_and(gone))
    }) {
        return Err("Delete the seams on the outline and on the shape first".into());
    }
    if template
        .line_springs
        .iter()
        .any(|s| s.panel == panel && gone(&s.line))
        || template
            .line_folds
            .iter()
            .any(|f| f.panel == panel && gone(&f.line))
    {
        return Err(
            "Clear the elastic, tape or folds on the outline and on the shape first".into(),
        );
    }
    let mut outline =
        crate::garment::shapes2d::shape_outline_polygon(shape, polygon.meshing_settings())?;
    let ring = shape.samples()?;
    // What lies inside the shape stays; anything else would be lost, so it is refused.
    let mut lines = vec![];
    for l in polygon.lines.iter().filter(|l| l.id != line) {
        if !l.samples()?.iter().all(|p| inside(&ring, *p)) {
            return Err(format!(
                "Internal line {} lies outside the new outline; delete it first",
                l.id
            ));
        }
        lines.push(l.clone());
    }
    for hole in &polygon.holes {
        if !hole.points.iter().all(|p| inside(&ring, *p)) {
            return Err("A hole lies outside the new outline; delete it first".into());
        }
    }
    let items = carried(template, panel);
    for (item, points) in &items {
        if !points.iter().any(|p| inside(&ring, *p)) {
            return Err(format!(
                "{} lies outside the new outline; move it first",
                carried_name(template, *item)
            ));
        }
    }
    if source
        .pin_points
        .iter()
        .any(|pin| !outline.points.iter().any(|p| p.distance(*pin) < 1e-9))
    {
        return Err("Remove the pins that are not on the new outline's corners first".into());
    }
    // The new boundaries take names the kept lines and holes do not use.
    let mut taken: BTreeSet<String> = lines.iter().map(|l| l.id.clone()).collect();
    taken.extend(polygon.holes.iter().flat_map(|h| h.edges.iter().cloned()));
    let mut curves = BTreeMap::new();
    for edge in &mut outline.edges {
        let fresh = if taken.contains(edge) {
            (1..)
                .map(|k| format!("{edge}_{k}"))
                .find(|c| !taken.contains(c))
                .unwrap()
        } else {
            edge.clone()
        };
        if let Some(c) = outline.curves.remove(edge) {
            curves.insert(fresh.clone(), c);
        }
        taken.insert(fresh.clone());
        *edge = fresh;
    }
    outline.curves = curves;
    outline.lines = lines;
    outline.holes = polygon.holes.clone();
    outline.subdivisions = polygon.subdivisions;
    outline.meshing = polygon.meshing.clone();
    let mut piece = source.clone();
    piece.polygon = Some(outline);
    let lo = outline_low(&piece);
    normalize(&mut piece);
    template.panels[index] = piece;
    for (item, _) in items {
        carry(template, item, panel, |p| p - lo, 0.);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_carried_to_the_outline_both_ways() {
        let square = [
            DVec2::ZERO,
            DVec2::new(1., 0.),
            DVec2::new(1., 1.),
            DVec2::new(0., 1.),
        ];
        let [(i, ti, p), (j, tj, q)] =
            cut_ends(&square, DVec2::new(0.5, 0.2), DVec2::new(0.5, 0.8)).unwrap();
        assert_eq!((i, j), (0, 2));
        assert!((ti - 0.5).abs() < 1e-12 && (tj - 0.5).abs() < 1e-12);
        assert!(p.distance(DVec2::new(0.5, 0.)) < 1e-12);
        assert!(q.distance(DVec2::new(0.5, 1.)) < 1e-12);
        assert!(inside(&square, DVec2::splat(0.5)));
        assert!(!inside(&square, DVec2::new(1.5, 0.5)));
    }

    #[test]
    fn a_seam_side_is_cut_into_matching_parts() {
        let r = BoundaryRef {
            reversed: false,
            panel: "p".into(),
            side: crate::garment::seam::Side::Top,
            edge: Some("e".into()),
            range: [0.2, 0.6],
        };
        let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-12);
        assert!(close(crossing(&r, 0.4), 0.5));
        assert_eq!(crossing(&r, 0.7), None);
        let part = sub_range(&r, 0., 0.5).range;
        assert!((part[0] - 0.2).abs() < 1e-12 && (part[1] - 0.4).abs() < 1e-12);
        let back = BoundaryRef {
            reversed: true,
            ..r.clone()
        };
        assert!(close(crossing(&back, 0.5), 0.25));
        let part = sub_range(&back, 0., 0.25).range;
        assert!((part[0] - 0.5).abs() < 1e-12 && (part[1] - 0.6).abs() < 1e-12);
    }
}
