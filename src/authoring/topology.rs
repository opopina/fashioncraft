//! Edits that change a piece's topology, carrying along what points into it. Cut and sew
//! splits a piece in two along a straight internal line carried on to the outline, sews the
//! halves together along the cut and moves every seam, marking and setting to the half it
//! belongs to; a seam that crosses the cut is split in two. Anything it cannot carry over
//! unambiguously (curved or annotated cut boundaries, holes, pins, graphics…) is refused
//! rather than dropped.
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

/// Cut piece `panel` along its straight internal line `line`, carried on to the outline, into
/// two pieces sewn together along the cut. Returns the new piece's id.
pub(super) fn cut_and_sew(
    template: &mut GarmentTemplate,
    panel: &str,
    line: &str,
) -> Result<String, String> {
    let held = template.pin_targets.iter().any(|p| p.panel == panel)
        || template.frozen_panels.iter().any(|p| p.panel == panel)
        || template.solidified_panels.iter().any(|p| p.panel == panel)
        || template.deactivated_panels.iter().any(|p| p.panel == panel);
    if held {
        return Err("Release pins, unfreeze and activate the piece before cutting it".into());
    }
    if template.graphics.iter().any(|g| g.panel_id == panel)
        || template.trims.iter().any(|t| t.panel_id == panel)
        || template
            .tacks
            .iter()
            .any(|t| t.a.panel == panel || t.b.panel == panel)
    {
        return Err("Remove the piece's graphics, trims and tacks before cutting it".into());
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
    if !polygon.holes.is_empty() {
        return Err("Pieces with holes cannot be cut yet".into());
    }
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

    let b_id = unused_id("panel", template.panels.iter().map(|p| p.id.clone()));
    // Which piece each original boundary name now belongs to (the cut parts are added below).
    let in_b: BTreeSet<String> = b_edges.iter().cloned().collect();

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
        b_poly.points = b_points.clone();
        b_poly.edges = b_names.to_vec();
        b_poly.lines = b_lines.clone();
        for (poly, names) in [(&mut a_poly, a_names), (&mut b_poly, b_names)] {
            let keep = |e: &String| names.contains(e);
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
    let mut a_panel = source.clone();
    a_panel.polygon = Some(a_poly);
    a_panel.pin_points.clear();
    let mut b_panel = source.clone();
    b_panel.id = b_id.clone();
    b_panel.name = format!("{} 2", source.name);
    b_panel.polygon = Some(b_poly);
    b_panel.pin_points.clear();
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
    Ok(b_id)
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
        assert_eq!(crossing(&r, 0.4), Some(0.5));
        assert_eq!(crossing(&r, 0.7), None);
        assert_eq!(sub_range(&r, 0., 0.5).range, [0.2, 0.4]);
        let back = BoundaryRef {
            reversed: true,
            ..r.clone()
        };
        assert_eq!(crossing(&back, 0.5), Some(0.25));
        assert_eq!(sub_range(&back, 0., 0.25).range, [0.5, 0.6]);
    }
}
