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
    if template.graphics.iter().any(|g| g.panel_id == b_id)
        || template.trims.iter().any(|t| t.panel_id == b_id)
        || template
            .tacks
            .iter()
            .any(|t| t.a.panel == b_id || t.b.panel == b_id)
    {
        return Err(
            "Move the graphics, trims and tacks off the second piece before merging".into(),
        );
    }
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
    if !pa.holes.is_empty() || !pb.holes.is_empty() {
        return Err("Pieces with holes cannot be merged yet".into());
    }
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
    for name in pb.edges.iter().chain(pb.lines.iter().map(|l| &l.id)) {
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
    let mut kept = a.clone();
    kept.polygon = Some(merged);
    normalize(&mut kept);
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
