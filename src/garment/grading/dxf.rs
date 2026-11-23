//! Grading through apparel DXF: grade rule numbers on the pieces' points, and the rule table
//! (`.RUL`) written beside the file.
//!
//! **Export.** Every grade point of a panel's outline carries a rule. So does any other
//! point of a written line whose growth differs by more than [`TOLERANCE`] from what a
//! reader gives a point without a rule: the proportional blend of the tagged points before
//! and after it along the line (zero on a line with none). That covers proportional points
//! next to curve rules, curve points, the cut line, cutout corners, internal lines and
//! notches that do not keep their share of the boundary, so a pattern CAD grades the same
//! outlines FashionCraft shows. Equal growths share one rule.
//!
//! **Import.** A rule number on an outline corner becomes that point's rule; corners without
//! one become proportional points. Numbers on curve points become the curve's rule (a least
//! squares fit of its middle's growth); numbers on notches become a notch rule keeping the
//! growth's place along the boundary. Numbers on cutouts and internal lines are not needed:
//! those follow the outline.
use super::{
    Grading, NotchMode, NotchRule, Rule, SizeGroup, Target, apply,
    rul::{self, RuleTable},
};
use crate::garment::{
    GarmentTemplate, curves,
    dxf_aama::{self, Dialect, GradePoint, PatternFile, Piece, Units, template::PieceOptions},
    markings::polyline_length,
    polygon::PolygonPattern,
};
use glam::DVec2;
use std::collections::BTreeMap;

/// A point whose growth differs from the proportional blend by more than this carries its
/// own rule, metres: half the 0.01 mm a metric table writes.
pub const TOLERANCE: f64 = 0.005e-3;
/// How near a rule number must sit to the point it grades when read, metres.
const MATCH_M: f64 = 0.05e-3;
/// How near a rule number must sit to a curve when read, metres: the importer fits curves
/// within 0.1 mm of their points, and the file rounds to 0.01 mm.
const CURVE_MATCH_M: f64 = 0.15e-3;

/// Material point to DXF point (+Y up).
fn out(p: DVec2) -> DVec2 {
    DVec2::new(p.x, -p.y)
}

/// The cubic of boundary `i`, or its straight segment as a cubic.
fn controls(p: &PolygonPattern, i: usize) -> [DVec2; 4] {
    let (a, b) = (p.points[i], p.points[(i + 1) % p.points.len()]);
    match p.curves.get(&p.edges[i]) {
        Some([c, d]) => [a, *c, *d, b],
        None => [a, a + (b - a) / 3., a + (b - a) * (2. / 3.), b],
    }
}

/// The parameter of `x`, a point on cubic `c`.
fn parameter(c: [DVec2; 4], x: DVec2) -> f64 {
    let mut t = (0..=64)
        .map(|k| k as f64 / 64.)
        .min_by(|a, b| {
            curves::cubic(c, *a)
                .distance_squared(x)
                .total_cmp(&curves::cubic(c, *b).distance_squared(x))
        })
        .unwrap_or(0.);
    for _ in 0..8 {
        let u = 1. - t;
        let d1 = (c[1] - c[0]) * (3. * u * u)
            + (c[2] - c[1]) * (6. * u * t)
            + (c[3] - c[2]) * (3. * t * t);
        let d2 = (c[2] - c[1] * 2. + c[0]) * (6. * u) + (c[3] - c[2] * 2. + c[1]) * (6. * t);
        let r = curves::cubic(c, t) - x;
        let f = r.dot(d1);
        let df = d1.dot(d1) + r.dot(d2);
        if df.abs() < 1e-30 {
            break;
        }
        t = (t - f / df).clamp(0., 1.);
    }
    t
}

/// One point of a written line, DXF frame: where it is and where it goes in each size.
#[derive(Clone, Debug)]
struct Mark {
    at: DVec2,
    growth: Vec<DVec2>,
    /// A grade point: it carries a rule whatever its growth.
    must: bool,
}

/// Which marks of a line carry a rule: the grade points, then every point a reader would
/// misplace by more than [`TOLERANCE`] when blending between the tagged points around it.
fn tag_line(marks: &[Mark], closed: bool) -> Vec<bool> {
    let n = marks.len();
    let mut tagged: Vec<bool> = marks.iter().map(|m| m.must).collect();
    if n == 0 {
        return tagged;
    }
    let sizes = marks[0].growth.len();
    let gap = |i: usize, j: usize| marks[i].at.distance(marks[j].at);
    for _ in 0..n {
        let mut more = vec![];
        for i in (0..n).filter(|i| !tagged[*i]) {
            let prediction: Vec<DVec2> = if !tagged.iter().any(|t| *t) {
                vec![DVec2::ZERO; sizes]
            } else {
                // The tagged points before and after along the line, with the distances.
                let walk = |forward: bool| {
                    let (mut k, mut d) = (i, 0.);
                    loop {
                        let next = if forward {
                            if k + 1 == n {
                                if !closed {
                                    return None;
                                }
                                0
                            } else {
                                k + 1
                            }
                        } else if k == 0 {
                            if !closed {
                                return None;
                            }
                            n - 1
                        } else {
                            k - 1
                        };
                        d += gap(k, next);
                        k = next;
                        if tagged[k] {
                            return Some((k, d));
                        }
                    }
                };
                match (walk(false), walk(true)) {
                    (Some((p, a)), Some((q, b))) => {
                        let t = if a + b > 0. { a / (a + b) } else { 0.5 };
                        (0..sizes)
                            .map(|s| marks[p].growth[s] * (1. - t) + marks[q].growth[s] * t)
                            .collect()
                    }
                    (Some((p, _)), None) => marks[p].growth.clone(),
                    (None, Some((q, _))) => marks[q].growth.clone(),
                    (None, None) => vec![DVec2::ZERO; sizes],
                }
            };
            let off = marks[i]
                .growth
                .iter()
                .zip(&prediction)
                .any(|(g, p)| g.distance(*p) > TOLERANCE);
            if off {
                more.push(i);
            }
        }
        if more.is_empty() {
            break;
        }
        for i in more {
            tagged[i] = true;
        }
    }
    tagged
}

/// Rules shared between equal growths, numbered from 1.
#[derive(Default)]
struct Numbering {
    rules: BTreeMap<i64, Vec<DVec2>>,
    by_growth: BTreeMap<Vec<(i64, i64)>, i64>,
}

impl Numbering {
    fn id(&mut self, growth: &[DVec2]) -> i64 {
        let key: Vec<(i64, i64)> = growth
            .iter()
            .map(|g| ((g.x * 1e7).round() as i64, (g.y * 1e7).round() as i64))
            .collect();
        if let Some(id) = self.by_growth.get(&key) {
            return *id;
        }
        let id = self.rules.len() as i64 + 1;
        self.by_growth.insert(key, id);
        self.rules.insert(id, growth.to_vec());
        id
    }
}

/// Pieces graded with rule numbers, and the rule table that grades them.
pub struct Export {
    pub pieces: Vec<Piece>,
    /// `None` when the garment is not graded, or shows only its base size.
    pub table: Option<RuleTable>,
}

/// Pieces for an apparel DXF in the base size, with grade rule numbers on their points and
/// the rule table that grows them into the group's shown sizes. Hidden sizes are left out.
/// The chosen pieces must be graded in one size group.
pub fn export(
    template: &GarmentTemplate,
    options: &PieceOptions,
    units: Units,
    dialect: Dialect,
) -> Result<Export, String> {
    let mut pieces = dxf_aama::template::pieces(template, options)?;
    let Some(grading) = &template.grading else {
        return Ok(Export {
            pieces,
            table: None,
        });
    };
    let panels: Vec<_> = template
        .panels
        .iter()
        .filter(|p| {
            options
                .panels
                .as_ref()
                .is_none_or(|ids| ids.contains(&p.id))
        })
        .collect();
    let mut group: Option<&SizeGroup> = None;
    for panel in &panels {
        let g = grading.group_of(&panel.id).ok_or("No size group")?;
        match group {
            Some(h) if h.id != g.id => {
                return Err(
                    "The pieces are graded in different size groups: export one group at a time"
                        .into(),
                );
            }
            _ => group = Some(g),
        }
    }
    let group = group.ok_or("There are no pieces to export")?;
    let sizes: Vec<_> = group.visible().collect();
    for piece in &mut pieces {
        piece.size = group.base_name().into();
    }
    if sizes.len() < 2 {
        return Ok(Export {
            pieces,
            table: None,
        });
    }
    // Every shown size: the whole garment, so cut lines and notches come out as written.
    let graded: Vec<Option<GarmentTemplate>> = sizes
        .iter()
        .map(|s| {
            if group.is_base(&s.id) {
                Ok(None)
            } else {
                apply::template_in_size(template, &group.id, &s.id).map(Some)
            }
        })
        .collect::<Result<_, String>>()?;
    let mut numbering = Numbering::default();
    for (panel, piece) in panels.iter().zip(&mut pieces) {
        let Some(base) = &panel.polygon else {
            continue;
        };
        // The panel and its piece in every shown size (the base for the base size).
        let mut polys = vec![];
        let mut fields = vec![];
        let mut grown = vec![];
        for (size, t) in sizes.iter().zip(&graded) {
            match t {
                None => {
                    polys.push(base.clone());
                    fields.push(apply::Field::default());
                    grown.push(piece.clone());
                }
                Some(t) => {
                    let p = t
                        .panels
                        .iter()
                        .find(|p| p.id == panel.id)
                        .ok_or("A graded panel is missing")?;
                    let g = apply::panel_in_size(template, &panel.id, &size.id)?
                        .ok_or("A graded panel lost its outline")?;
                    // The cut garment moves a panel that grows past zero over; growths are
                    // measured without that move.
                    let moved = p
                        .polygon
                        .as_ref()
                        .ok_or("A graded panel lost its outline")?;
                    let by = out(moved.points[0] - g.polygon.points[0]);
                    let mut piece = dxf_aama::template::piece(t, p, options)?;
                    shift_piece(&mut piece, -by);
                    polys.push(g.polygon);
                    fields.push(g.field);
                    grown.push(piece);
                }
            }
        }
        let n = base.points.len();
        // The sew line: every sample of the outline, its growth read on the same curve
        // parameter in each size.
        let mut sew = vec![];
        for i in 0..n {
            let samples = base.edge_points(i)?;
            let c = controls(base, i);
            for (k, x) in samples[..samples.len() - 1].iter().enumerate() {
                let t = if k == 0 { 0. } else { parameter(c, *x) };
                let growth = polys
                    .iter()
                    .map(|p| out(curves::cubic(controls(p, i), t)) - out(*x))
                    .collect();
                sew.push(Mark {
                    at: out(*x),
                    growth,
                    must: k == 0 && grading.is_grade_point(&panel.id, &base.edges[i]),
                });
            }
        }
        let mut points = vec![];
        let mut add = |marks: &[Mark], tags: &[bool], numbering: &mut Numbering| {
            for (m, tag) in marks.iter().zip(tags) {
                if *tag {
                    points.push(GradePoint {
                        at: m.at,
                        rule: numbering.id(&m.growth),
                        notch: false,
                    });
                }
            }
        };
        let sew_tags = tag_line(&sew, true);
        add(&sew, &sew_tags, &mut numbering);
        // The cut line, when it is apart from the sew line: corners by their order in each
        // size, other points with the growth of the nearest sew-line point.
        if piece.sew.is_some() {
            let nearest = |x: DVec2| {
                sew.iter()
                    .min_by(|a, b| a.at.distance(x).total_cmp(&b.at.distance(x)))
                    .expect("an outline has points")
            };
            let corners = |p: &Piece| -> Vec<DVec2> {
                p.cut
                    .points
                    .iter()
                    .zip(&p.cut.turns)
                    .filter(|(_, t)| **t)
                    .map(|(p, _)| *p)
                    .collect()
            };
            let base_corners = corners(piece);
            let ordered = grown.iter().all(|g| corners(g).len() == base_corners.len());
            let mut k = 0;
            let cut: Vec<Mark> = piece
                .cut
                .points
                .iter()
                .zip(&piece.cut.turns)
                .map(|(x, turn)| {
                    let near = nearest(*x);
                    let growth = if *turn && ordered {
                        let g = grown.iter().map(|g| corners(g)[k] - *x).collect();
                        k += 1;
                        g
                    } else {
                        near.growth.clone()
                    };
                    Mark {
                        at: *x,
                        growth,
                        must: *turn && near.must,
                    }
                })
                .collect();
            let tags = tag_line(&cut, true);
            add(&cut, &tags, &mut numbering);
        }
        // Cutouts: every corner, by its order.
        for (h, ring) in piece.cutouts.iter().enumerate() {
            let marks: Vec<Mark> = ring
                .points
                .iter()
                .enumerate()
                .map(|(k, x)| Mark {
                    at: *x,
                    growth: grown
                        .iter()
                        .map(|g| {
                            g.cutouts
                                .get(h)
                                .and_then(|r| r.points.get(k))
                                .map_or(DVec2::ZERO, |p| *p - *x)
                        })
                        .collect(),
                    must: false,
                })
                .collect();
            let tags = tag_line(&marks, true);
            add(&marks, &tags, &mut numbering);
        }
        // Internal lines and cutout sew lines follow the outline's field.
        let follow = |x: DVec2| -> Vec<DVec2> {
            fields.iter().map(|f| out(f.displacement(out(x)))).collect()
        };
        for ring in &piece.cutout_sews {
            let marks: Vec<Mark> = ring
                .points
                .iter()
                .map(|x| Mark {
                    at: *x,
                    growth: follow(*x),
                    must: false,
                })
                .collect();
            let tags = tag_line(&marks, true);
            add(&marks, &tags, &mut numbering);
        }
        for line in &piece.internal {
            let marks: Vec<Mark> = line
                .points
                .iter()
                .map(|x| Mark {
                    at: *x,
                    growth: follow(*x),
                    must: false,
                })
                .collect();
            let tags = tag_line(&marks, line.closed);
            add(&marks, &tags, &mut numbering);
        }
        // Notches that do not keep their share of the boundary.
        let rules = grading.panels.get(&panel.id);
        let mut k = 0;
        for (edge, list) in &base.notches {
            for notch in list {
                let graded_rule = rules.and_then(|r| r.notch(edge, notch.at)).is_some();
                if graded_rule && let Some(mark) = piece.notches.get(k) {
                    let growth: Vec<DVec2> = grown
                        .iter()
                        .map(|g| g.notches.get(k).map_or(DVec2::ZERO, |n| n.base - mark.base))
                        .collect();
                    points.push(GradePoint {
                        at: mark.base,
                        rule: numbering.id(&growth),
                        notch: true,
                    });
                }
                k += 1;
            }
        }
        piece.grade_points = points;
        let (lo, hi) = piece.cut.points.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        let (w, cy) = (hi.x - lo.x, (lo.y + hi.y) * 0.5);
        piece.grade_reference = Some([
            DVec2::new(lo.x + w * 0.25, cy),
            DVec2::new(lo.x + w * 0.75, cy),
        ]);
    }
    let (date, time) = dxf_aama::template::now_utc();
    let table = RuleTable {
        name: dxf_aama::template::file_stem(&template.name).to_uppercase(),
        units,
        sizes: sizes.iter().map(|s| rul::size_word(&s.name)).collect(),
        sample: rul::size_word(group.base_name()),
        author: dxf_aama::template::author(),
        created: Some((date, time)),
        version: if dialect == Dialect::Astm {
            rul::ASTM_VERSION.into()
        } else {
            String::new()
        },
        rules: numbering.rules,
        notes: vec![],
    };
    for piece in &mut pieces {
        piece.size = table.sample.clone();
    }
    Ok(Export {
        pieces,
        table: Some(table),
    })
}

/// The point at arclength `distance` along a polyline, and the share of its length.
fn nearest_share(points: &[DVec2], x: DVec2) -> (f64, f64) {
    let total = polyline_length(points);
    let (mut best, mut at, mut walked) = (f64::INFINITY, 0., 0.);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = a.distance(b);
        let t = if len > 0. {
            ((x - a).dot(b - a) / (len * len)).clamp(0., 1.)
        } else {
            0.
        };
        let d = (a + (b - a) * t).distance(x);
        if d < best {
            best = d;
            at = walked + t * len;
        }
        walked += len;
    }
    (best, if total > 0. { at / total } else { 0. })
}

/// Read grading for pieces just imported: `panels[k]` is the panel made from
/// `file.pieces[selected[k]]`. Sizes come from the table: an existing group with the same
/// sizes and base is reused, else a group is added. Returns notes for the import review.
pub fn attach(
    template: &mut GarmentTemplate,
    file: &PatternFile,
    selected: &[usize],
    panels: &[String],
    table: &RuleTable,
) -> Result<Vec<String>, String> {
    let mut notes = table.notes.clone();
    let names: Vec<String> = table.sizes.clone();
    let sample = names
        .iter()
        .position(|s| s == &table.sample)
        .ok_or("The sample size is not in the size list")?;
    let mut grading = match template.grading.take() {
        Some(g) => g,
        None => {
            let title = if table.name.trim().is_empty() {
                "Sizes".to_string()
            } else {
                table.name.clone()
            };
            let sizes: Vec<&str> = names.iter().map(String::as_str).collect();
            Grading::new(&title, &sizes, sample)?
        }
    };
    let same = |g: &SizeGroup| {
        g.sizes.len() == names.len()
            && g.sizes
                .iter()
                .zip(&names)
                .all(|(a, b)| rul::size_word(&a.name).eq_ignore_ascii_case(b))
            && g.rank(&g.base) == Some(sample)
    };
    let group_id = match grading.groups.iter().find(|g| same(g)) {
        Some(g) => g.id.clone(),
        None => {
            let sizes: Vec<&str> = names.iter().map(String::as_str).collect();
            let title = if table.name.trim().is_empty() {
                "Imported sizes"
            } else {
                table.name.as_str()
            };
            grading.add_group(title, &sizes, sample)?
        }
    };
    let group = grading.group(&group_id).expect("just made").clone();
    let size_ids: Vec<String> = group.sizes.iter().map(|s| s.id.clone()).collect();
    let mut missing = std::collections::BTreeSet::new();
    for (&index, panel_id) in selected.iter().zip(panels) {
        let piece = file.pieces.get(index).ok_or("Stale piece selection")?;
        let poly = template
            .panels
            .iter()
            .find(|p| &p.id == panel_id)
            .and_then(|p| p.polygon.clone())
            .ok_or("An imported panel has no outline")?;
        grading.assign(std::slice::from_ref(panel_id), &group_id)?;
        if piece.grade_points.is_empty() {
            continue;
        }
        let sew = piece.sew_line();
        let (lo, hi) = sew.points.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        let to_material = |p: DVec2| DVec2::new(p.x - lo.x, hi.y - p.y);
        let angle = piece
            .grade_reference
            .map_or(0., |[a, b]| (b - a).y.atan2((b - a).x));
        let (sin, cos) = angle.sin_cos();
        // A rule's growths in material frame, by size id.
        let growths = |rule: i64, missing: &mut std::collections::BTreeSet<i64>| {
            let Some(list) = table.rules.get(&rule) else {
                missing.insert(rule);
                return None;
            };
            Some(
                list.iter()
                    .zip(&size_ids)
                    .map(|(g, id)| {
                        let d = DVec2::new(g.x * cos - g.y * sin, g.x * sin + g.y * cos);
                        (id.clone(), DVec2::new(d.x, -d.y))
                    })
                    .collect::<BTreeMap<String, DVec2>>(),
            )
        };
        let n = poly.points.len();
        let edge_samples: Vec<Vec<DVec2>> = (0..n)
            .map(|i| poly.edge_points(i))
            .collect::<Result<_, _>>()?;
        let mut corners: BTreeMap<usize, BTreeMap<String, DVec2>> = BTreeMap::new();
        let mut on_cut: Vec<(DVec2, BTreeMap<String, DVec2>)> = vec![];
        type CurvePoint = (f64, DVec2, BTreeMap<String, DVec2>);
        let mut on_curves: BTreeMap<usize, Vec<CurvePoint>> = BTreeMap::new();
        let mut notch_points = vec![];
        let mut unused = 0;
        for gp in &piece.grade_points {
            let at = to_material(gp.at);
            if gp.notch {
                notch_points.push((at, gp.rule));
                continue;
            }
            let Some(growth) = growths(gp.rule, &mut missing) else {
                continue;
            };
            if let Some(i) = (0..n).find(|i| poly.points[*i].distance(at) <= MATCH_M) {
                corners.insert(i, growth);
                continue;
            }
            let on_edge = (0..n)
                .map(|i| (i, nearest_share(&edge_samples[i], at).0))
                .filter(|(i, d)| {
                    let curved = poly.curves.contains_key(&poly.edges[*i]);
                    *d <= if curved { CURVE_MATCH_M } else { MATCH_M }
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match on_edge {
                Some((i, _)) if poly.curves.contains_key(&poly.edges[i]) => {
                    let t = parameter(controls(&poly, i), at);
                    on_curves.entry(i).or_default().push((t, at, growth));
                }
                Some(_) => unused += 1,
                None if piece.sew.is_some()
                    && ring_distance(&piece.cut.points, gp.at) <= MATCH_M =>
                {
                    on_cut.push((at, growth));
                }
                None => unused += 1,
            }
        }
        // Corners graded only on the cut line take the rule of the cut corner nearest them.
        if corners.is_empty() && !on_cut.is_empty() {
            for (at, growth) in &on_cut {
                let i = (0..n)
                    .min_by(|a, b| {
                        poly.points[*a]
                            .distance(*at)
                            .total_cmp(&poly.points[*b].distance(*at))
                    })
                    .expect("an outline has points");
                corners.entry(i).or_insert_with(|| growth.clone());
            }
            notes.push(format!(
                "{}: grade rules read from the cut line's corners",
                piece.name
            ));
        }
        for (i, edge) in poly.edges.iter().enumerate().take(n) {
            match corners.get(&i) {
                Some(deltas) => {
                    let deltas = deltas
                        .iter()
                        .filter(|(_, d)| **d != DVec2::ZERO)
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    grading.set_rule(
                        &Target::Point {
                            panel: panel_id.clone(),
                            edge: edge.clone(),
                        },
                        Rule { deltas, step: None },
                    )?;
                }
                None if !corners.is_empty() => grading.set_grade_point(panel_id, edge, false),
                None => {}
            }
        }
        // Curve rules: the middle's growth that puts the size's curve nearest its graded
        // points. A curve grows by (1-t)·start + t·end + 4t(1-t)·middle; the fit starts from
        // each point's parameter on the base curve, then follows the nearest point of the
        // grown curve (Gauss-Newton on the distance), since a curve refitted on import is
        // parameterised a little differently from the one written.
        for (i, samples) in &on_curves {
            let j = (i + 1) % n;
            let edge = poly.edges[*i].clone();
            let c = controls(&poly, *i);
            let mut deltas = BTreeMap::new();
            for size in &size_ids {
                let ends = apply::point_deltas(&poly, grading.panels.get(panel_id), &group, size)?;
                let target = |(_, at, growth): &CurvePoint| {
                    *at + growth.get(size).copied().unwrap_or(DVec2::ZERO)
                };
                let grown = |delta: DVec2| {
                    let bump = delta * (4. / 3.);
                    [
                        c[0] + ends[*i],
                        c[1] + ends[*i] * (2. / 3.) + ends[j] * (1. / 3.) + bump,
                        c[2] + ends[*i] * (1. / 3.) + ends[j] * (2. / 3.) + bump,
                        c[3] + ends[j],
                    ]
                };
                let mut delta = DVec2::ZERO;
                for round in 0..8 {
                    let curve = grown(delta);
                    let (mut num, mut den) = (DVec2::ZERO, 0.);
                    for sample in samples {
                        let x = target(sample);
                        let t = if round == 0 {
                            sample.0
                        } else {
                            parameter(curve, x)
                        };
                        let w = 4. * t * (1. - t);
                        num += (x - curves::cubic(curve, t)) * w;
                        den += w * w;
                    }
                    if den <= 0. {
                        break;
                    }
                    let step = num / den;
                    delta += step;
                    if step.length() < 1e-9 {
                        break;
                    }
                }
                if delta.length() > TOLERANCE {
                    deltas.insert(size.clone(), delta);
                }
            }
            if !deltas.is_empty() {
                grading.set_rule(
                    &Target::Curve {
                        panel: panel_id.clone(),
                        edge,
                    },
                    Rule { deltas, step: None },
                )?;
            }
        }
        // Notch rules: where the growth puts the notch along its boundary in each size.
        for (at, rule) in notch_points {
            let Some(growth) = growths(rule, &mut missing) else {
                continue;
            };
            let found = poly.notches.iter().find_map(|(edge, list)| {
                let boundary = poly.closed_boundary(edge)?;
                let width = poly.allowance_of(edge);
                list.iter().find_map(|notch| {
                    let p = curves::arclength_point(&boundary, notch.at);
                    let (_, dir) = direction_at(&boundary, notch.at);
                    let outward = DVec2::new(dir.y, -dir.x);
                    ((p + outward * width).distance(at) <= width + 0.5e-3)
                        .then(|| (edge.clone(), notch.at, boundary.clone()))
                })
            });
            let Some((edge, base_at, boundary)) = found else {
                unused += 1;
                continue;
            };
            let base_len = polyline_length(&boundary);
            let base_point = curves::arclength_point(&boundary, base_at);
            let mut deltas = BTreeMap::new();
            for size in &size_ids {
                if group.is_base(size) {
                    continue;
                }
                let graded =
                    apply::grade_polygon(&poly, grading.panels.get(panel_id), &group, size)?;
                let Some(grown) = graded.polygon.closed_boundary(&edge) else {
                    continue;
                };
                let target = base_point + growth.get(size).copied().unwrap_or(DVec2::ZERO);
                let (_, share) = nearest_share(&grown, target);
                let extra = share * polyline_length(&grown) - base_at * base_len;
                if extra.abs() > TOLERANCE {
                    deltas.insert(size.clone(), extra);
                }
            }
            grading.set_notch_rule(
                panel_id,
                NotchRule {
                    edge,
                    at: base_at,
                    mode: NotchMode::FromStart,
                    deltas,
                },
            )?;
        }
        if unused > 0 {
            notes.push(format!(
                "{}: {unused} grade rule number(s) on internal lines, cutouts or straight boundaries were not needed",
                piece.name
            ));
        }
    }
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(i64::to_string).collect();
        notes.push(format!(
            "Rules {} are not in the rule table: those points do not grow",
            list.join(", ")
        ));
    }
    grading.validate()?;
    template.grading = Some(grading);
    Ok(notes)
}

/// The unit direction of a polyline at `share` of its length.
fn direction_at(points: &[DVec2], share: f64) -> (DVec2, DVec2) {
    let total = polyline_length(points);
    let mut left = total * share;
    for pair in points.windows(2) {
        let len = pair[0].distance(pair[1]);
        if len <= 1e-12 {
            continue;
        }
        if left <= len {
            let d = (pair[1] - pair[0]) / len;
            return (pair[0] + d * left, d);
        }
        left -= len;
    }
    (points[points.len() - 1], DVec2::X)
}

/// Distance from `x` to a closed ring.
fn ring_distance(ring: &[DVec2], x: DVec2) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let len2 = a.distance_squared(b);
            let t = if len2 > 0. {
                ((x - a).dot(b - a) / len2).clamp(0., 1.)
            } else {
                0.
            };
            (a + (b - a) * t).distance(x)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Move every point of a piece by `by`.
fn shift_piece(piece: &mut Piece, by: DVec2) {
    if by == DVec2::ZERO {
        return;
    }
    let rings = std::iter::once(&mut piece.cut)
        .chain(piece.sew.as_mut())
        .chain(piece.cutouts.iter_mut())
        .chain(piece.cutout_sews.iter_mut());
    for ring in rings {
        for p in &mut ring.points {
            *p += by;
        }
    }
    for line in &mut piece.internal {
        for p in &mut line.points {
            *p += by;
        }
    }
    for notch in &mut piece.notches {
        notch.base += by;
        notch.tip += by;
    }
    for p in piece
        .grain
        .iter_mut()
        .flatten()
        .chain(piece.drill_holes.iter_mut())
    {
        *p += by;
    }
}
