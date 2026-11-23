//! A garment in another size: the base panels moved by their grade rules.
//!
//! Grade points move by their growth. A proportional point moves between the grade points
//! before and after it in proportion to the base contour's length, as the rule-table formats
//! describe for points without a rule. A curve's control points follow its ends (at a third
//! and two thirds) and add the curve rule's growth, scaled so the curve's middle moves by
//! exactly that growth. Notches follow their boundary as their rule says. Everything inside
//! the outline (holes, internal lines, graphics, trims, pins, steam) follows the outline's
//! movement by mean value coordinates (Hormann and Floater, "Mean value coordinates for
//! arbitrary planar polygons", ACM TOG 2006), which reproduce the outline's movement on the
//! outline and blend it smoothly inside.
use super::{Grading, NotchMode, PanelRules, SizeGroup};
use crate::garment::{
    GarmentTemplate, curves, markings::polyline_length, polygon::PolygonPattern, seam::BoundaryRef,
};
use glam::DVec2;
use std::collections::BTreeMap;

/// Samples taken along a curved boundary to carry its movement inward.
const CURVE_SAMPLES: usize = 12;

/// How the inside of a panel follows its outline in one size: the base outline sampled, and
/// where each sample goes.
#[derive(Clone, Debug, Default)]
pub struct Field {
    base: Vec<DVec2>,
    moved: Vec<DVec2>,
}

impl Field {
    /// A field from outline samples and where each goes, in order round the outline.
    pub fn from_samples(base: Vec<DVec2>, moved: Vec<DVec2>) -> Self {
        assert_eq!(base.len(), moved.len());
        Self { base, moved }
    }
    /// The movement of material point `x`.
    pub fn displacement(&self, x: DVec2) -> DVec2 {
        let n = self.base.len();
        if n == 0 {
            return DVec2::ZERO;
        }
        let d: Vec<DVec2> = self
            .base
            .iter()
            .zip(&self.moved)
            .map(|(a, b)| *b - *a)
            .collect();
        if d.iter().all(|v| *v == d[0]) {
            return d[0];
        }
        let s: Vec<DVec2> = self.base.iter().map(|v| *v - x).collect();
        let r: Vec<f64> = s.iter().map(|v| v.length()).collect();
        if let Some(i) = r.iter().position(|r| *r < 1e-12) {
            return d[i];
        }
        let mut tan = vec![0.; n];
        for i in 0..n {
            let j = (i + 1) % n;
            let area = s[i].perp_dot(s[j]);
            let dot = s[i].dot(s[j]);
            if area.abs() <= 1e-13 * r[i] * r[j] {
                if dot < 0. {
                    // On the outline: between its two samples.
                    let t = r[i] / (r[i] + r[j]);
                    return d[i] * (1. - t) + d[j] * t;
                }
                continue;
            }
            tan[i] = (r[i] * r[j] - dot) / area;
        }
        let mut weights = 0.;
        let mut sum = DVec2::ZERO;
        for i in 0..n {
            let w = (tan[(i + n - 1) % n] + tan[i]) / r[i];
            weights += w;
            sum += d[i] * w;
        }
        if weights.abs() < 1e-12 || !sum.is_finite() {
            let nearest = (0..n).min_by(|a, b| r[*a].total_cmp(&r[*b])).unwrap_or(0);
            return d[nearest];
        }
        sum / weights
    }
    pub fn map(&self, x: DVec2) -> DVec2 {
        x + self.displacement(x)
    }
    /// The same movement followed by a shift of the whole panel.
    fn shifted(mut self, by: DVec2) -> Self {
        if self.base.is_empty() {
            // An empty field moves nothing; a lone sample carries the shift everywhere.
            self.base.push(DVec2::ZERO);
            self.moved.push(DVec2::ZERO);
        }
        for p in &mut self.moved {
            *p += by;
        }
        self
    }
    fn between(base: &PolygonPattern, graded: &PolygonPattern) -> Self {
        let n = base.points.len();
        let mut field = Self::default();
        for i in 0..n {
            let j = (i + 1) % n;
            let curve = |p: &PolygonPattern| p.curves.get(&p.edges[i]).copied();
            match (curve(base), curve(graded)) {
                (Some([a, b]), Some([c, d])) => {
                    let from = [base.points[i], a, b, base.points[j]];
                    let to = [graded.points[i], c, d, graded.points[j]];
                    for k in 0..CURVE_SAMPLES {
                        let t = k as f64 / CURVE_SAMPLES as f64;
                        field.base.push(curves::cubic(from, t));
                        field.moved.push(curves::cubic(to, t));
                    }
                }
                _ => {
                    field.base.push(base.points[i]);
                    field.moved.push(graded.points[i]);
                }
            }
        }
        field
    }
}

/// One panel in one size.
#[derive(Clone, Debug)]
pub struct GradedPanel {
    pub polygon: PolygonPattern,
    pub field: Field,
    /// Why the size's outline is not usable (a crossing, a notch off its boundary), when so.
    pub problem: Option<String>,
}

/// Length of each outline boundary of the base polygon, as flattened.
fn edge_lengths(poly: &PolygonPattern) -> Result<Vec<f64>, String> {
    (0..poly.points.len())
        .map(|i| poly.edge_points(i).map(|p| polyline_length(&p)))
        .collect()
}

/// The growth of every contour point of `poly` in `size`.
pub fn point_deltas(
    poly: &PolygonPattern,
    rules: Option<&PanelRules>,
    group: &SizeGroup,
    size: &str,
) -> Result<Vec<DVec2>, String> {
    let n = poly.points.len();
    if group.is_base(size) {
        return Ok(vec![DVec2::ZERO; n]);
    }
    let grade: Vec<bool> = poly
        .edges
        .iter()
        .map(|e| rules.is_none_or(|r| !r.proportional.contains(e)))
        .collect();
    let own: Vec<DVec2> = poly
        .edges
        .iter()
        .map(|e| {
            rules
                .and_then(|r| r.points.get(e))
                .map_or(DVec2::ZERO, |rule| rule.delta(group, size))
        })
        .collect();
    if !grade.iter().any(|g| *g) {
        return Ok(vec![DVec2::ZERO; n]);
    }
    let lengths = edge_lengths(poly)?;
    Ok((0..n)
        .map(|i| {
            if grade[i] {
                return own[i];
            }
            let (mut p, mut back) = (i, 0.);
            loop {
                p = (p + n - 1) % n;
                back += lengths[p];
                if grade[p] {
                    break;
                }
            }
            let (mut q, mut ahead) = (i, 0.);
            loop {
                ahead += lengths[q];
                q = (q + 1) % n;
                if grade[q] {
                    break;
                }
            }
            let t = if back + ahead > 0. {
                back / (back + ahead)
            } else {
                0.5
            };
            own[p] * (1. - t) + own[q] * t
        })
        .collect())
}

/// Where a notch at share `at` of a boundary `base_len` long goes on the same boundary
/// `len` long in another size.
pub fn notch_at(at: f64, base_len: f64, len: f64, mode: NotchMode, extra: f64) -> Option<f64> {
    if len <= 0. || !len.is_finite() {
        return None;
    }
    let value = match mode {
        NotchMode::Proportional => at + extra / len,
        NotchMode::FromStart => (at * base_len + extra) / len,
        NotchMode::FromEnd => 1. - ((1. - at) * base_len + extra) / len,
    };
    (value.is_finite() && value > 0. && value < 1.).then_some(value)
}

/// `poly` in `size`: its rules applied, the inside carried along, and the outline checked.
pub fn grade_polygon(
    poly: &PolygonPattern,
    rules: Option<&PanelRules>,
    group: &SizeGroup,
    size: &str,
) -> Result<GradedPanel, String> {
    poly.validate_outer_structure()?;
    let n = poly.points.len();
    let deltas = point_deltas(poly, rules, group, size)?;
    let mut out = poly.clone();
    for (p, d) in out.points.iter_mut().zip(&deltas) {
        *p += *d;
    }
    for i in 0..n {
        let edge = &poly.edges[i];
        let Some([c, d]) = poly.curves.get(edge) else {
            continue;
        };
        let j = (i + 1) % n;
        let bump = rules
            .and_then(|r| r.curves.get(edge))
            .map_or(DVec2::ZERO, |r| r.delta(group, size))
            * (4. / 3.);
        out.curves.insert(
            edge.clone(),
            [
                *c + deltas[i] * (2. / 3.) + deltas[j] * (1. / 3.) + bump,
                *d + deltas[i] * (1. / 3.) + deltas[j] * (2. / 3.) + bump,
            ],
        );
    }
    let field = Field::between(poly, &out);
    for hole in &mut out.holes {
        for p in &mut hole.points {
            *p = field.map(*p);
        }
        for edge in &hole.edges {
            if let Some(controls) = out.curves.get_mut(edge) {
                for c in controls {
                    *c = field.map(*c);
                }
            }
        }
    }
    for line in &mut out.lines {
        for p in &mut line.points {
            *p = field.map(*p);
        }
    }
    // A required length is a base-size specification.
    out.target_lengths.clear();
    let mut problem = None;
    let shape = out_shape(&out);
    for (edge, list) in &mut out.notches {
        let (Some(from), Some(to)) = (poly.closed_boundary(edge), shape.closed_boundary(edge))
        else {
            continue;
        };
        let (base_len, len) = (polyline_length(&from), polyline_length(&to));
        for notch in list.iter_mut() {
            let rule = rules.and_then(|r| r.notch(edge, notch.at));
            let mode = rule.map_or(NotchMode::Proportional, |r| r.mode);
            let extra = rule
                .and_then(|r| r.deltas.get(size))
                .copied()
                .filter(|_| !group.is_base(size))
                .unwrap_or(0.);
            match notch_at(notch.at, base_len, len, mode, extra) {
                Some(at) => notch.at = at,
                None => {
                    problem
                        .get_or_insert_with(|| format!("a notch on {edge} falls off its boundary"));
                }
            }
        }
    }
    if problem.is_none() {
        problem = check(&out).err();
    }
    Ok(GradedPanel {
        polygon: out,
        field,
        problem,
    })
}

/// The polygon without its notches, to measure boundaries while the notches are rewritten.
fn out_shape(p: &PolygonPattern) -> PolygonPattern {
    PolygonPattern {
        notches: BTreeMap::new(),
        ..p.clone()
    }
}

/// The checks a compiled panel would fail: a crossing or reversed outline, holes and lines
/// leaving the fabric.
pub fn check(poly: &PolygonPattern) -> Result<(), String> {
    let outer = poly.outer_points()?;
    if crate::garment::domain::area(&outer) <= 0. {
        return Err("the outline turns inside out".into());
    }
    crate::garment::domain::simple(&outer, true)?;
    crate::garment::domain::validate(poly, &outer)
}

/// The panel `id` in `size` of its group, or `None` when it has no polygon. The base size
/// gives the panel as it is.
pub fn panel_in_size(
    template: &GarmentTemplate,
    id: &str,
    size: &str,
) -> Result<Option<GradedPanel>, String> {
    let panel = template
        .panels
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("No panel {id}"))?;
    let Some(poly) = &panel.polygon else {
        return Ok(None);
    };
    let grading = template.grading.as_ref();
    let group = grading.and_then(|g| g.group_of(id));
    match group {
        Some(group) if group.rank(size).is_some() && !group.is_base(size) => {
            grade_polygon(poly, grading.and_then(|g| g.panels.get(id)), group, size).map(Some)
        }
        _ => Ok(Some(GradedPanel {
            polygon: poly.clone(),
            field: Field::default(),
            problem: None,
        })),
    }
}

/// Rest length of one seam side reference.
fn reference_length(template: &GarmentTemplate, r: &BoundaryRef) -> Option<f64> {
    let poly = template
        .panels
        .iter()
        .find(|p| p.id == r.panel)?
        .polygon
        .as_ref()?;
    let boundary = poly.named_boundary(r.edge.as_deref()?)?;
    Some(polyline_length(&boundary) * (r.range[1] - r.range[0]).abs())
}

/// Both sides' lengths of every seam, by seam id.
pub fn seam_lengths(template: &GarmentTemplate) -> BTreeMap<String, (f64, f64)> {
    template
        .seams
        .iter()
        .filter_map(|s| {
            let side = |b| {
                s.side(b)
                    .map(|r| reference_length(template, r))
                    .sum::<Option<f64>>()
            };
            Some((s.id.clone(), (side(false)?, side(true)?)))
        })
        .collect()
}

/// The garment with each group in the size given by group id (the base for groups not
/// listed), and its grading removed: the garment as it is cut and simulated in those sizes.
pub fn template_in_sizes(
    template: &GarmentTemplate,
    sizes: &BTreeMap<String, String>,
) -> Result<GarmentTemplate, String> {
    let mut out = template.clone();
    let Some(grading) = out.grading.take() else {
        return Ok(out);
    };
    let mut fields: BTreeMap<String, (Field, Option<String>)> = BTreeMap::new();
    for panel in &mut out.panels {
        let Some(group) = grading.group_of(&panel.id) else {
            continue;
        };
        let size = sizes
            .get(&group.id)
            .map(String::as_str)
            .filter(|s| group.rank(s).is_some())
            .unwrap_or(&group.base);
        if group.is_base(size) {
            continue;
        }
        let Some(poly) = &panel.polygon else {
            continue;
        };
        let mut graded = grade_polygon(poly, grading.panels.get(&panel.id), group, size)?;
        if let Some(problem) = &graded.problem {
            return Err(format!(
                "{} in size {}: {problem}",
                panel.name,
                group.name_of(size)
            ));
        }
        // Material coordinates start at zero: a size that grows past the base's left or top
        // edge moves over, and its placement moves back, so the cloth stays where it was.
        let low = graded
            .polygon
            .points
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |m, p| m.min(*p));
        let shift = DVec2::new((-low.x).max(0.), (-low.y).max(0.));
        if shift != DVec2::ZERO {
            let poly = &mut graded.polygon;
            for p in poly
                .points
                .iter_mut()
                .chain(poly.curves.values_mut().flatten())
            {
                *p += shift;
            }
            for p in poly
                .holes
                .iter_mut()
                .flat_map(|h| h.points.iter_mut())
                .chain(poly.lines.iter_mut().flat_map(|l| l.points.iter_mut()))
            {
                *p += shift;
            }
            graded.field = std::mem::take(&mut graded.field).shifted(shift);
            let origin = panel.placement.transform(glam::DVec3::ZERO);
            let along = panel
                .placement
                .transform(glam::DVec3::new(shift.x, -shift.y, 0.))
                - origin;
            panel.placement.translation -= along;
        }
        let extent = |p: &PolygonPattern| {
            p.outer_points().ok().map(|pts| {
                let (lo, hi) = pts.iter().fold(
                    (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                    |(lo, hi), p| (lo.min(*p), hi.max(*p)),
                );
                hi - lo
            })
        };
        if let (Some(a), Some(b)) = (extent(poly), extent(&graded.polygon))
            && a.x > 0.
            && a.y > 0.
        {
            panel.pattern.width *= b.x / a.x;
            panel.pattern.height *= b.y / a.y;
        }
        let high = graded
            .polygon
            .points
            .iter()
            .fold(DVec2::ZERO, |m, p| m.max(*p));
        panel.pattern.width = panel.pattern.width.max(high.x);
        panel.pattern.height = panel.pattern.height.max(high.y);
        for p in &mut panel.pin_points {
            *p = graded.field.map(*p);
        }
        panel.polygon = Some(graded.polygon);
        fields.insert(panel.id.clone(), (graded.field, Some(size.to_string())));
    }
    if fields.is_empty() {
        return Ok(out);
    }
    let field = |panel: &str| fields.get(panel).map(|(f, _)| f);
    for g in &mut out.graphics {
        let Some((f, Some(size))) = fields.get(&g.panel_id) else {
            continue;
        };
        g.center_m = f.map(g.center_m);
        if let Some(rule) = grading.graphics.get(&g.id) {
            g.center_m += rule.offsets.get(size).copied().unwrap_or(DVec2::ZERO);
            g.size_m *= rule.scales.get(size).copied().unwrap_or(1.);
        }
    }
    for trim in &mut out.trims {
        let Some(f) = field(&trim.panel_id) else {
            continue;
        };
        use crate::garment::trims::TrimKind as K;
        match &mut trim.kind {
            K::Button { center_m, .. } | K::Object { center_m, .. } => *center_m = f.map(*center_m),
            K::Buttonhole { start_m, end_m, .. } => {
                *start_m = f.map(*start_m);
                *end_m = f.map(*end_m);
            }
            K::Zipper { path_m, .. } | K::Topstitch { path_m, .. } | K::Piping { path_m, .. } => {
                for p in path_m {
                    *p = f.map(*p);
                }
            }
        }
    }
    for pin in &mut out.pin_targets {
        if let Some(f) = field(&pin.panel) {
            pin.uv = f.map(pin.uv);
        }
    }
    for tack in &mut out.tacks {
        for end in [&mut tack.a, &mut tack.b] {
            if let Some(f) = field(&end.panel) {
                end.uv = f.map(end.uv);
            }
        }
    }
    for stamp in &mut out.steam {
        if let Some(f) = field(&stamp.panel) {
            stamp.center = f.map(stamp.center);
        }
    }
    // Captured 3D shapes belong to the base size's mesh.
    out.frozen_panels.retain(|p| !fields.contains_key(&p.panel));
    out.solidified_panels
        .retain(|p| !fields.contains_key(&p.panel));
    out.deactivated_panels
        .retain(|p| !fields.contains_key(&p.panel));
    // Seams sewn at equal length in the base may differ by a little in another size: they
    // ease there instead of failing (the grade table lists the difference).
    let lengths = seam_lengths(&out);
    for seam in &mut out.seams {
        if seam.allow_ease {
            continue;
        }
        let touched = seam.references().any(|r| fields.contains_key(&r.panel));
        if !touched {
            continue;
        }
        let differs = lengths
            .get(&seam.id)
            .is_none_or(|(a, b)| (a - b).abs() > 1e-9 * a.max(*b));
        if differs || !seam.notches.is_empty() {
            seam.allow_ease = true;
        }
    }
    Ok(out)
}

/// The garment with group `group` in `size` and every other group in its base size.
pub fn template_in_size(
    template: &GarmentTemplate,
    group: &str,
    size: &str,
) -> Result<GarmentTemplate, String> {
    template_in_sizes(
        template,
        &BTreeMap::from([(group.to_string(), size.to_string())]),
    )
}

/// The garment in the sizes on show, when some group shows a size other than its base.
pub fn view(template: &GarmentTemplate) -> Result<Option<GarmentTemplate>, String> {
    match &template.grading {
        Some(g) if g.shows_graded_size() => {
            template_in_sizes(template, &g.active_sizes()).map(Some)
        }
        _ => Ok(None),
    }
}

/// What is simulated: the garment in the sizes on show, without its grading. A grading
/// change that leaves this equal is a draft change. When a size cannot be made, the
/// template is returned as it is, so the difference shows and compiling reports it.
pub fn effective(template: &GarmentTemplate) -> GarmentTemplate {
    match view(template) {
        Ok(Some(t)) => t,
        Ok(None) => {
            let mut t = template.clone();
            t.grading = None;
            t
        }
        Err(_) => template.clone(),
    }
}

/// Keep the sizes `current` shows on `target`, a template taken back from history: picking
/// a size is not an edit, so undo does not change it.
pub fn keep_shown_sizes(current: &GarmentTemplate, target: &mut GarmentTemplate) {
    let (Some(from), Some(to)) = (current.grading.as_ref(), target.grading.as_mut()) else {
        return;
    };
    for group in &mut to.groups {
        if let Some(shown) = from.group(&group.id) {
            let active = shown.active.clone();
            if active.as_ref().is_none_or(|a| group.rank(a).is_some()) {
                group.active = active;
            }
        }
    }
}

/// Difference between the two sides of each seam touching group `group`'s panels, in `size`,
/// where it is larger than `tolerance` metres: (seam id, side A, side B).
pub fn seam_differences(
    template: &GarmentTemplate,
    group: &SizeGroup,
    size: &str,
    tolerance: f64,
) -> Result<Vec<(String, f64, f64)>, String> {
    let graded = template_in_size(template, &group.id, size)?;
    let grading: &Grading = template.grading.as_ref().ok_or("No grading")?;
    let lengths = seam_lengths(&graded);
    Ok(template
        .seams
        .iter()
        .filter(|s| {
            s.references()
                .any(|r| grading.group_of(&r.panel).is_some_and(|g| g.id == group.id))
        })
        .filter_map(|s| {
            let (a, b) = lengths.get(&s.id)?;
            ((a - b).abs() > tolerance).then(|| (s.id.clone(), *a, *b))
        })
        .collect())
}

/// The graphic placements as shown: those on panels in a size other than their base follow
/// their panel into that size, with their own offset and scale. Cheaper than [`view`] when
/// only the graphics are needed.
pub fn graphics_shown(
    template: &GarmentTemplate,
) -> Vec<crate::garment::graphics::GraphicPlacement> {
    let mut out = template.graphics.clone();
    let Some(grading) = template.grading.as_ref().filter(|g| g.shows_graded_size()) else {
        return out;
    };
    let mut fields: BTreeMap<String, Option<(Field, String)>> = BTreeMap::new();
    for g in &mut out {
        let entry = fields.entry(g.panel_id.clone()).or_insert_with(|| {
            let size = grading.active_size(&g.panel_id)?.to_string();
            let graded = panel_in_size(template, &g.panel_id, &size).ok()??;
            Some((graded.field, size))
        });
        let Some((field, size)) = entry else {
            continue;
        };
        g.center_m = field.map(g.center_m);
        if let Some(rule) = grading.graphics.get(&g.id) {
            g.center_m += rule
                .offsets
                .get(size.as_str())
                .copied()
                .unwrap_or(DVec2::ZERO);
            g.size_m *= rule.scales.get(size.as_str()).copied().unwrap_or(1.);
        }
    }
    out
}
