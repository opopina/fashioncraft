//! Constrained Delaunay refinement in a constant material-space metric.
//! Contours are constraints; named edges are recovered after Steiner insertion.
use glam::DVec2;
use serde::{Deserialize, Serialize};
use spade::{
    AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshingSettings {
    /// Nominal equilateral edge size. The enforced bound is on triangle area.
    pub target_edge_m: f64,
    /// Ratio of spacing along axis_degrees to spacing perpendicular to it.
    #[serde(default = "one")]
    pub anisotropy: f64,
    #[serde(default)]
    pub axis_degrees: f64,
    #[serde(default = "angle")]
    pub min_angle_degrees: f64,
    #[serde(default = "budget")]
    pub max_vertices: usize,
    /// Quad mesh: inside, an even grid of squares one target edge apart along `axis_degrees`
    /// (each square two triangles); free triangles only where the grid meets the outline,
    /// holes and internal lines.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quads: bool,
    /// Zones meshed finer than the rest (3D Garment › Subdivide selected mesh).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refine: Vec<RefineZone>,
}

/// A zone of a piece meshed finer than the rest: after the piece is meshed, every triangle
/// whose centre lies in one of `triangles` (material coordinates, metres) is split along its
/// longest edge until no edge of it is longer than `edge_m`. Triangles elsewhere keep their
/// shape, except next to the zone, where splits spread just enough to keep the mesh whole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefineZone {
    pub triangles: Vec<[DVec2; 3]>,
    pub edge_m: f64,
}

/// Most refined zones a piece may have.
pub const MAX_REFINE_ZONES: usize = 16;
/// Most triangles that may outline one zone.
pub const MAX_ZONE_TRIANGLES: usize = 50_000;
/// Shortest longest-edge a zone may ask for, metres.
pub const MIN_ZONE_EDGE_M: f64 = 0.002;

impl RefineZone {
    pub fn validate(&self) -> Result<(), String> {
        if self.triangles.is_empty()
            || self.triangles.len() > MAX_ZONE_TRIANGLES
            || self.triangles.iter().flatten().any(|p| !p.is_finite())
            || !self.edge_m.is_finite()
            || !(MIN_ZONE_EDGE_M..=0.5).contains(&self.edge_m)
        {
            return Err(format!(
                "A subdivided zone needs 1 to {MAX_ZONE_TRIANGLES} triangles and an edge of 2 to 500 mm"
            ));
        }
        Ok(())
    }
}
fn one() -> f64 {
    1.0
}
fn angle() -> f64 {
    25.0
}
fn budget() -> usize {
    20_000
}
impl Default for MeshingSettings {
    fn default() -> Self {
        Self {
            target_edge_m: 0.025,
            anisotropy: one(),
            axis_degrees: 0.0,
            min_angle_degrees: angle(),
            quads: false,
            max_vertices: budget(),
            refine: vec![],
        }
    }
}
impl MeshingSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.target_edge_m.is_finite()
            || !(0.003..=0.5).contains(&self.target_edge_m)
            || !self.anisotropy.is_finite()
            || !(1.0..=4.0).contains(&self.anisotropy)
            || !self.axis_degrees.is_finite()
            || !self.min_angle_degrees.is_finite()
            || !(15.0..=30.0).contains(&self.min_angle_degrees)
            || !(64..=100_000).contains(&self.max_vertices)
        {
            return Err("Invalid meshing settings: size 3..500 mm, anisotropy 1..4, angle 15..30 degrees, budget 64..100000".into());
        }
        if self.refine.len() > MAX_REFINE_ZONES {
            return Err(format!(
                "A piece may have {MAX_REFINE_ZONES} subdivided zones at most"
            ));
        }
        for zone in &self.refine {
            zone.validate()?;
        }
        Ok(())
    }
    pub fn to_metric(&self, p: DVec2) -> DVec2 {
        let (s, c) = self.axis_degrees.to_radians().sin_cos();
        DVec2::new(
            (c * p.x + s * p.y) / (self.target_edge_m * self.anisotropy),
            (-s * p.x + c * p.y) / self.target_edge_m,
        )
    }
    fn material_point(&self, q: DVec2) -> DVec2 {
        let (s, c) = self.axis_degrees.to_radians().sin_cos();
        let p = q * DVec2::new(self.target_edge_m * self.anisotropy, self.target_edge_m);
        DVec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
    }
}
/// Least distance, in target edges, between a grid point and the outline or a constraint: a
/// closer grid point would leave a sliver against it.
const GRID_CLEARANCE: f64 = 0.45;

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let t = if d.length_squared() > 0. {
        ((p - a).dot(d) / d.length_squared()).clamp(0., 1.)
    } else {
        0.
    };
    p.distance(a + d * t)
}

/// Insert an even grid of points, one unit apart in the metric (one target edge along the
/// mesh axis), inside the material: inside the outline, outside the holes, and clear of every
/// outline and constraint segment.
fn seed_grid(
    cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>,
    outline: &[DVec2],
    metric: &[DVec2],
    holes: &[super::domain::Hole],
    paths: &[(String, Vec<DVec2>)],
    settings: &MeshingSettings,
) -> Result<(), String> {
    let mut segments: Vec<[DVec2; 2]> = (0..metric.len())
        .map(|i| [metric[i], metric[(i + 1) % metric.len()]])
        .collect();
    for (_, path) in paths {
        for pair in path.windows(2) {
            segments.push([settings.to_metric(pair[0]), settings.to_metric(pair[1])]);
        }
    }
    let lo = metric
        .iter()
        .copied()
        .fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let hi = metric
        .iter()
        .copied()
        .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
    for y in (lo.y.floor() as i64)..=(hi.y.ceil() as i64) {
        for x in (lo.x.floor() as i64)..=(hi.x.ceil() as i64) {
            let q = DVec2::new(x as f64, y as f64);
            let p = settings.material_point(q);
            if !super::domain::inside(p, outline)
                || holes.iter().any(|h| super::domain::inside(p, &h.points))
                || segments
                    .iter()
                    .any(|[a, b]| segment_distance(q, *a, *b) < GRID_CLEARANCE)
            {
                continue;
            }
            if cdt.num_vertices() + 1 > settings.max_vertices {
                return Err("The quad grid exceeds the meshing vertex budget".into());
            }
            cdt.insert(Point2::new(q.x, q.y))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// A refined zone ready for point queries: its triangles bucketed on a grid.
struct Zone {
    triangles: Vec<[DVec2; 3]>,
    edge_m: f64,
    cell: f64,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Zone {
    fn new(zone: &RefineZone) -> Self {
        let size = zone
            .triangles
            .iter()
            .map(|t| (t[1] - t[0]).abs().max(t[2] - t[0]).max_element())
            .fold(0., f64::max);
        let cell = size.max(1e-4);
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, t) in zone.triangles.iter().enumerate() {
            let lo = t[0].min(t[1]).min(t[2]);
            let hi = t[0].max(t[1]).max(t[2]);
            for x in (lo.x / cell).floor() as i64..=(hi.x / cell).floor() as i64 {
                for y in (lo.y / cell).floor() as i64..=(hi.y / cell).floor() as i64 {
                    grid.entry((x, y)).or_default().push(i);
                }
            }
        }
        Self {
            triangles: zone.triangles.clone(),
            edge_m: zone.edge_m,
            cell,
            grid,
        }
    }
    fn contains(&self, p: DVec2) -> bool {
        let key = (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        );
        self.grid.get(&key).is_some_and(|list| {
            list.iter().any(|i| {
                let [a, b, c] = self.triangles[*i];
                let (d1, d2, d3) = (
                    (b - a).perp_dot(p - a),
                    (c - b).perp_dot(p - b),
                    (a - c).perp_dot(p - c),
                );
                let tolerance = -1e-12;
                (d1 >= tolerance && d2 >= tolerance && d3 >= tolerance)
                    || (d1 <= -tolerance && d2 <= -tolerance && d3 <= -tolerance)
            })
        })
    }
}

fn edge_key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// Split the triangles `too_big` asks for along their longest edge, by longest-edge
/// propagation (Rivara): when that edge is not also the longest of the triangle across it,
/// that neighbour is split first, along its own longest edge, and so on, so every split
/// halves an edge shared by two triangles that both have it as their longest (or one on the
/// outline). The mesh stays whole, its smallest angle is at least half of the smallest it had,
/// and triangles away from the split ones keep their vertices. New vertices go after the
/// existing ones, each at the middle of the edge it splits (so on the outline when the edge
/// is). Winding is kept.
pub fn refine_longest_edge(
    uv: &mut Vec<DVec2>,
    faces: &mut Vec<[usize; 3]>,
    too_big: impl Fn([DVec2; 3]) -> bool,
    max_vertices: usize,
) -> Result<(), String> {
    let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (f, t) in faces.iter().enumerate() {
        for k in 0..3 {
            edges
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(f);
        }
    }
    // Edges in a strict order (length, then vertices), so a longest-edge path cannot cycle.
    let longest = |uv: &[DVec2], t: [usize; 3]| {
        (0..3)
            .map(|k| edge_key(t[k], t[(k + 1) % 3]))
            .max_by(|x, y| {
                uv[x.0]
                    .distance_squared(uv[x.1])
                    .total_cmp(&uv[y.0].distance_squared(uv[y.1]))
                    .then(x.cmp(y))
            })
            .unwrap()
    };
    let corners = |uv: &[DVec2], t: [usize; 3]| t.map(|i| uv[i]);
    let mut stack: Vec<usize> = (0..faces.len())
        .filter(|f| too_big(corners(uv, faces[*f])))
        .collect();
    while let Some(f) = stack.pop() {
        if !too_big(corners(uv, faces[f])) {
            continue;
        }
        // Walk the longest-edge path from f to the edge to split.
        let mut current = f;
        let mut walked = 0;
        let edge = loop {
            let e = longest(uv, faces[current]);
            let across = edges[&e].iter().copied().find(|g| *g != current);
            match across {
                Some(g) if longest(uv, faces[g]) != e => current = g,
                _ => break e,
            }
            walked += 1;
            if walked > faces.len() {
                return Err("Mesh subdivision did not converge".into());
            }
        };
        if uv.len() >= max_vertices {
            return Err(
                "Subdividing there exceeds the meshing vertex budget; select a smaller zone".into(),
            );
        }
        let middle = uv.len();
        uv.push((uv[edge.0] + uv[edge.1]) * 0.5);
        let sharing = edges.remove(&edge).unwrap_or_default();
        for g in sharing {
            let t = faces[g];
            let k = (0..3)
                .find(|k| edge_key(t[*k], t[(*k + 1) % 3]) == edge)
                .expect("the split edge is the triangle's");
            let (p, q, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let new = faces.len();
            faces[g] = [p, middle, c];
            faces.push([middle, q, c]);
            edges.entry(edge_key(p, middle)).or_default().push(g);
            edges.entry(edge_key(middle, q)).or_default().push(new);
            edges
                .entry(edge_key(middle, c))
                .or_default()
                .extend([g, new]);
            if let Some(owners) = edges.get_mut(&edge_key(q, c)) {
                for owner in owners.iter_mut().filter(|o| **o == g) {
                    *owner = new;
                }
            }
            stack.extend([g, new]);
        }
        stack.push(f);
    }
    Ok(())
}

pub struct TriangulatedContour {
    pub uv: Vec<DVec2>,
    pub faces: Vec<[usize; 3]>,
    pub boundaries: BTreeMap<String, Vec<usize>>,
}

#[derive(Debug, Serialize)]
pub struct MeshQuality {
    pub triangles: usize,
    pub min_angle_degrees: f64,
    pub min_metric_angle_degrees: f64,
    pub median_edge_m: f64,
    pub max_edge_m: f64,
    pub max_triangle_area_m2: f64,
    pub max_metric_triangle_area: f64,
}
pub fn quality(triangles: &[crate::model::Triangle], settings: &MeshingSettings) -> MeshQuality {
    let mut result = MeshQuality {
        triangles: triangles.len(),
        min_angle_degrees: 180.0,
        min_metric_angle_degrees: 180.0,
        median_edge_m: 0.0,
        max_edge_m: 0.0,
        max_triangle_area_m2: 0.0,
        max_metric_triangle_area: 0.0,
    };
    let mut lengths = vec![];
    for t in triangles {
        let metric = t.rest.map(|p| settings.to_metric(p));
        result.max_triangle_area_m2 = result.max_triangle_area_m2.max(t.area);
        result.max_metric_triangle_area = result.max_metric_triangle_area.max(
            (metric[1] - metric[0])
                .perp_dot(metric[2] - metric[0])
                .abs()
                * 0.5,
        );
        for i in 0..3 {
            let a = t.rest[(i + 1) % 3] - t.rest[i];
            let b = t.rest[(i + 2) % 3] - t.rest[i];
            lengths.push(a.length());
            result.min_angle_degrees = result
                .min_angle_degrees
                .min(a.angle_to(b).abs().to_degrees());
            let a = metric[(i + 1) % 3] - metric[i];
            let b = metric[(i + 2) % 3] - metric[i];
            result.min_metric_angle_degrees = result
                .min_metric_angle_degrees
                .min(a.angle_to(b).abs().to_degrees());
        }
    }
    lengths.sort_by(f64::total_cmp);
    if !lengths.is_empty() {
        result.median_edge_m = lengths[lengths.len() / 2];
        result.max_edge_m = *lengths.last().unwrap();
    }
    result
}

/// The caller validates the simple CCW contour before entering this routine.
pub fn triangulate(
    points: &[DVec2],
    names: &[String],
    settings: &MeshingSettings,
) -> Result<TriangulatedContour, String> {
    triangulate_domain(points, names, &[], &[], settings)
}

/// Internal constraints do not delimit material. With holes/lines, refine the
/// hull then select faces by the validated material domain, never by crossing
/// an arbitrary constraint. Boundary-only legacy inputs keep the old path.
pub fn triangulate_domain(
    points: &[DVec2],
    names: &[String],
    holes: &[super::domain::Hole],
    lines: &[super::domain::InternalLine],
    settings: &MeshingSettings,
) -> Result<TriangulatedContour, String> {
    settings.validate()?;
    let metric: Vec<_> = points.iter().map(|p| settings.to_metric(*p)).collect();
    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    let mut handles = vec![];
    for i in 0..points.len() {
        let a = metric[i];
        let b = metric[(i + 1) % points.len()];
        let count = a.distance(b).ceil().max(1.0) as usize;
        if handles.len().saturating_add(count) > settings.max_vertices {
            return Err("Contour exceeds meshing vertex budget".into());
        }
        for k in 0..count {
            let p = a.lerp(b, k as f64 / count as f64);
            handles.push(
                cdt.insert(Point2::new(p.x, p.y))
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    for i in 0..handles.len() {
        cdt.add_constraint(handles[i], handles[(i + 1) % handles.len()]);
    }
    let mut paths: Vec<(String, Vec<DVec2>)> = vec![];
    for hole in holes {
        for (i, name) in hole.edges.iter().enumerate() {
            paths.push((
                name.clone(),
                vec![hole.points[i], hole.points[(i + 1) % hole.points.len()]],
            ));
        }
    }
    for line in lines {
        paths.push((line.id.clone(), line.samples()?));
    }
    for (_, path) in &paths {
        for pair in path.windows(2) {
            let a = settings.to_metric(pair[0]);
            let b = settings.to_metric(pair[1]);
            let count = a.distance(b).ceil().max(1.) as usize;
            if cdt.num_vertices().saturating_add(count.saturating_add(1)) > settings.max_vertices {
                return Err("Internal constraints exceed meshing vertex budget".into());
            }
            let mut previous = None;
            for k in 0..=count {
                // Keep shared endpoints bit-identical; rounded lerp(1) can
                // otherwise create a near-duplicate constraint vertex.
                let q = if k == count {
                    b
                } else {
                    a.lerp(b, k as f64 / count as f64)
                };
                let next = cdt
                    .insert(Point2::new(q.x, q.y))
                    .map_err(|e| e.to_string())?;
                if let Some(prev) = previous {
                    if prev == next || !cdt.can_add_constraint(prev, next) {
                        return Err("Intersecting or degenerate material constraints".into());
                    }
                    cdt.add_constraint(prev, next);
                }
                previous = Some(next);
            }
        }
    }
    let legacy = holes.is_empty() && lines.is_empty();
    if !legacy {
        // The material boundary must not nearly coincide with the hull during
        // full-domain refinement. Rotated/rounded collinear samples otherwise
        // form vanishing angles between hull and constraints and cause runaway
        // encroachment. An external frame separates them without moving cloth.
        if cdt.num_vertices().saturating_add(4) > settings.max_vertices {
            return Err("Domain frame exceeds meshing vertex budget".into());
        }
        let lo = metric
            .iter()
            .copied()
            .fold(DVec2::splat(f64::INFINITY), DVec2::min)
            - DVec2::splat(2.);
        let hi = metric
            .iter()
            .copied()
            .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max)
            + DVec2::splat(2.);
        for p in [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)] {
            cdt.insert(Point2::new(p.x, p.y))
                .map_err(|e| e.to_string())?;
        }
    }
    if settings.quads {
        seed_grid(&mut cdt, points, &metric, holes, &paths, settings)?;
    }
    // Squares of the grid split into two triangles of half a unit each; free triangles stay
    // within the equilateral bound.
    let max_area = if settings.quads {
        0.5 * (1.0 + 1e-9)
    } else {
        3.0_f64.sqrt() / 4.0
    };
    let material_face = |p: [Point2<f64>; 3]| {
        if legacy {
            return true;
        }
        let q = settings.material_point(DVec2::new(
            (p[0].x + p[1].x + p[2].x) / 3.,
            (p[0].y + p[1].y + p[2].y) / 3.,
        ));
        super::domain::inside(q, points)
            && !holes.iter().any(|h| super::domain::inside(q, &h.points))
    };
    let mut excluded = BTreeSet::new();
    let mut complete = false;
    for _ in 0..8 {
        let parameters = RefinementParameters::new()
            .exclude_outer_faces(legacy)
            .with_max_allowed_area(max_area)
            .with_angle_limit(AngleLimit::from_deg(settings.min_angle_degrees))
            .with_max_additional_vertices(settings.max_vertices.saturating_sub(cdt.num_vertices()));
        let result = cdt.refine(parameters);
        if !result.refinement_complete {
            return Err(format!(
                "Delaunay refinement incomplete at {} / {} vertices; increase target size or budget",
                cdt.num_vertices(),
                settings.max_vertices
            ));
        }
        excluded = result.excluded_faces.iter().map(|f| f.index()).collect();
        // A refinement pass can leave newly formed triangles over the requested
        // area. Check the actual postcondition and seed their centroids explicitly.
        let oversized: Vec<_> = cdt
            .inner_faces()
            .filter(|f| {
                !excluded.contains(&f.index())
                    && material_face(f.vertices().map(|v| v.position()))
                    && f.area() > max_area * (1.0 + 1e-10)
            })
            .map(|f| {
                let p = f.vertices().map(|v| v.position());
                Point2::new(
                    (p[0].x + p[1].x + p[2].x) / 3.0,
                    (p[0].y + p[1].y + p[2].y) / 3.0,
                )
            })
            .collect();
        if oversized.is_empty() {
            complete = true;
            break;
        }
        if cdt.num_vertices() + oversized.len() > settings.max_vertices {
            return Err("Delaunay area refinement exceeds vertex budget".into());
        }
        for p in oversized {
            cdt.insert(p).map_err(|e| e.to_string())?;
        }
    }
    if !complete {
        return Err("Delaunay refinement did not satisfy the requested area bound".into());
    }
    let raw_faces: Vec<_> = cdt
        .inner_faces()
        .filter(|f| {
            !excluded.contains(&f.index()) && material_face(f.vertices().map(|v| v.position()))
        })
        .map(|f| f.vertices().map(|v| v.index()))
        .collect();
    let used: BTreeSet<_> = raw_faces.iter().flatten().copied().collect();
    let mut remap = BTreeMap::new();
    let mut uv = vec![];
    for v in cdt.vertices() {
        if used.contains(&v.index()) {
            remap.insert(v.index(), uv.len());
            let p = v.position();
            uv.push(settings.material_point(DVec2::new(p.x, p.y)));
        }
    }
    let mut faces: Vec<[usize; 3]> = raw_faces
        .into_iter()
        .map(|f| f.map(|i| remap[&i]))
        .collect();
    // Finer zones: split there, before the named edges are recovered, so their new points on
    // outlines and lines join those edges.
    if !settings.refine.is_empty() {
        let zones: Vec<Zone> = settings.refine.iter().map(Zone::new).collect();
        refine_longest_edge(
            &mut uv,
            &mut faces,
            |t| {
                let centre = (t[0] + t[1] + t[2]) / 3.;
                let longest = (0..3)
                    .map(|k| t[k].distance(t[(k + 1) % 3]))
                    .fold(0., f64::max);
                zones
                    .iter()
                    .any(|z| longest > z.edge_m * (1. + 1e-9) && z.contains(centre))
            },
            settings.max_vertices,
        )?;
    }
    let mut boundaries = BTreeMap::new();
    for i in 0..points.len() {
        let a = points[i];
        let d = points[(i + 1) % points.len()] - a;
        let mut edge: Vec<_> = uv
            .iter()
            .enumerate()
            .filter_map(|(j, p)| {
                let t = (*p - a).dot(d) / d.length_squared();
                ((-1e-9..=1.0 + 1e-9).contains(&t) && (*p - a - t * d).length() < 1e-9)
                    .then_some((t, j))
            })
            .collect();
        edge.sort_by(|a, b| a.0.total_cmp(&b.0));
        if edge.len() < 2 {
            return Err("Missing refined contour edge".into());
        }
        boundaries.insert(names[i].clone(), edge.into_iter().map(|(_, j)| j).collect());
    }
    for (name, path) in paths {
        let mut chain = vec![];
        for pair in path.windows(2) {
            let (a, d) = (pair[0], pair[1] - pair[0]);
            let mut edge: Vec<_> = uv
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let t = (*p - a).dot(d) / d.length_squared();
                    ((-1e-9..=1. + 1e-9).contains(&t) && (*p - a - t * d).length() < 1e-9)
                        .then_some((t, i))
                })
                .collect();
            edge.sort_by(|a, b| a.0.total_cmp(&b.0));
            if edge.len() < 2 {
                return Err("Missing internal constraint segment".into());
            }
            let ids: Vec<_> = edge.into_iter().map(|(_, i)| i).collect();
            if chain.is_empty() {
                chain.extend(ids);
            } else {
                if chain.last() != ids.first() {
                    return Err("Disconnected internal constraint".into());
                }
                chain.extend_from_slice(&ids[1..]);
            }
        }
        boundaries.insert(name, chain);
    }
    Ok(TriangulatedContour {
        uv,
        faces,
        boundaries,
    })
}
