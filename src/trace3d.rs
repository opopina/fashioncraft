//! Patterns traced on the body: points picked on the avatar are joined along its surface into
//! a closed loop, the smaller side of the body the loop encloses is cut out, and that patch is
//! flattened into the plane keeping its lengths as well as it can — a least-squares conformal
//! map to start, then as-rigid-as-possible steps. The loop's flat image is the outline of a
//! new piece, and every point of the piece has a place on the body to start from.
use crate::avatar::SurfaceMesh;
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, BinaryHeap, HashMap};

/// A patch of the body flattened.
#[derive(Clone, Debug)]
pub struct Flattened {
    /// The patch's vertices (indices into the avatar), their flat positions (metres, as seen
    /// from outside the body, up the body along +y) and its triangles in local indices, wound
    /// like the avatar's.
    pub vertices: Vec<usize>,
    pub flat: Vec<DVec2>,
    pub triangles: Vec<[usize; 3]>,
    /// Each patch vertex's outward normal on the body (area-weighted over all its triangles).
    pub normals: Vec<DVec3>,
    /// The loop's vertices in local indices, in order around the patch, counter-clockwise in
    /// the flat plane.
    pub outline: Vec<usize>,
    /// How far edge lengths changed in flattening, as a share of their length on the body:
    /// root mean square and largest.
    pub stretch_rms: f64,
    pub stretch_max: f64,
}

impl Flattened {
    /// The outline's flat points.
    pub fn outline_points(&self) -> Vec<DVec2> {
        self.outline.iter().map(|k| self.flat[*k]).collect()
    }

    /// The outline as pattern points: pattern v grows down the body, as in drawn pieces.
    pub fn pattern_points(&self) -> Vec<DVec2> {
        self.outline
            .iter()
            .map(|k| DVec2::new(self.flat[*k].x, -self.flat[*k].y))
            .collect()
    }

    /// Starting positions on the body for a piece drawn from [`Flattened::pattern_points`]
    /// moved by `-shift` (as drawing a piece moves its points to start at zero): each pattern
    /// point `uv` at its place on the body, `clearance` out from it.
    pub fn body_positions(
        &self,
        mesh: &SurfaceMesh,
        uv: &[DVec2],
        shift: DVec2,
        clearance: f64,
    ) -> Vec<DVec3> {
        uv.iter()
            .map(|q| {
                let flat = DVec2::new(q.x + shift.x, -(q.y + shift.y));
                let (at, normal) = self.on_body_with_normal(mesh, flat);
                at + normal * clearance
            })
            .collect()
    }

    /// Where a flat point lies on the body: the patch triangle under it (or the nearest one)
    /// and that triangle's point on the avatar.
    pub fn on_body(&self, mesh: &SurfaceMesh, p: DVec2) -> DVec3 {
        self.on_body_with_normal(mesh, p).0
    }

    /// [`Flattened::on_body`], with the outward normal of the body's triangle there.
    pub fn on_body_with_normal(&self, mesh: &SurfaceMesh, p: DVec2) -> (DVec3, DVec3) {
        let mut best = (f64::NEG_INFINITY, DVec3::ZERO, DVec3::Y);
        for t in &self.triangles {
            let [a, b, c] = t.map(|k| self.flat[k]);
            let area = (b - a).perp_dot(c - a);
            if area.abs() < 1e-18 {
                continue;
            }
            let w1 = (p - a).perp_dot(c - a) / area;
            let w2 = (b - a).perp_dot(p - a) / area;
            let w = [1. - w1 - w2, w1, w2];
            let low = w[0].min(w[1]).min(w[2]);
            if low > best.0 {
                let w = w.map(|x| x.max(0.));
                let sum = w[0] + w[1] + w[2];
                let at = t
                    .iter()
                    .zip(w)
                    .map(|(k, x)| mesh.vertices[self.vertices[*k]] * (x / sum))
                    .sum();
                let normal = t
                    .iter()
                    .zip(w)
                    .map(|(k, x)| self.normals[*k] * x)
                    .sum::<DVec3>()
                    .normalize_or(DVec3::Y);
                best = (low, at, normal);
                if low >= 0. {
                    break;
                }
            }
        }
        (best.1, best.2)
    }
}

fn neighbours(mesh: &SurfaceMesh) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]; mesh.vertices.len()];
    for t in &mesh.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            out[a].push(b);
            out[b].push(a);
        }
    }
    for list in &mut out {
        list.sort_unstable();
        list.dedup();
    }
    out
}

/// The shortest path along the mesh's edges from `from` to `to`, both ends included.
fn path(mesh: &SurfaceMesh, around: &[Vec<usize>], from: usize, to: usize) -> Option<Vec<usize>> {
    #[derive(PartialEq)]
    struct Item(f64, usize);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Item {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other.0.total_cmp(&self.0)
        }
    }
    let mut distance: HashMap<usize, f64> = HashMap::from([(from, 0.)]);
    let mut previous: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::from([Item(0., from)]);
    while let Some(Item(d, v)) = heap.pop() {
        if v == to {
            let mut out = vec![to];
            let mut at = to;
            while at != from {
                at = previous[&at];
                out.push(at);
            }
            out.reverse();
            return Some(out);
        }
        if d > distance[&v] {
            continue;
        }
        for &u in &around[v] {
            let e = d + mesh.vertices[v].distance(mesh.vertices[u]);
            if distance.get(&u).is_none_or(|x| e < *x) {
                distance.insert(u, e);
                previous.insert(u, v);
                heap.push(Item(e, u));
            }
        }
    }
    None
}

/// The avatar vertex nearest to where a ray from `origin` along `direction` first meets the
/// body.
pub fn pick(mesh: &SurfaceMesh, origin: DVec3, direction: DVec3) -> Option<usize> {
    let mut best = (f64::INFINITY, None);
    for t in &mesh.triangles {
        if let Some((d, w)) =
            crate::picking::ray_triangle(origin, direction, t.map(|v| mesh.vertices[v]))
            && d < best.0
        {
            let k = (0..3).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
            best = (d, Some(t[k]));
        }
    }
    best.1
}

/// The path along the surface through `points` in order, open (for drawing a loop being
/// picked); legs between points on separate parts of the body are left out.
pub fn surface_chain(mesh: &SurfaceMesh, points: &[usize]) -> Vec<usize> {
    let around = neighbours(mesh);
    let mut out: Vec<usize> = points.first().copied().into_iter().collect();
    for pair in points.windows(2) {
        if let Some(leg) = path(mesh, &around, pair[0], pair[1]) {
            out.extend_from_slice(&leg[1..]);
        }
    }
    out
}

/// A closed outline with points dropped where the rest stay within `tolerance` of it
/// (Douglas–Peucker on the closed polygon, split at its two farthest points).
pub fn simplify(points: &[DVec2], tolerance: f64) -> Vec<DVec2> {
    let n = points.len();
    if n <= 4 {
        return points.to_vec();
    }
    fn keep(points: &[DVec2], from: usize, to: usize, tolerance: f64, out: &mut Vec<bool>) {
        let n = points.len();
        let (a, b) = (points[from % n], points[to % n]);
        let mut worst = (0., None);
        let mut k = from + 1;
        while k < to {
            let p = points[k % n];
            let d = b - a;
            let t = ((p - a).dot(d) / d.length_squared().max(1e-300)).clamp(0., 1.);
            let distance = p.distance(a + d * t);
            if distance > worst.0 {
                worst = (distance, Some(k));
            }
            k += 1;
        }
        if let (distance, Some(k)) = worst
            && distance > tolerance
        {
            out[k % n] = true;
            keep(points, from, k, tolerance, out);
            keep(points, k, to, tolerance, out);
        }
    }
    let far = (1..n)
        .max_by(|a, b| {
            points[*a]
                .distance(points[0])
                .total_cmp(&points[*b].distance(points[0]))
        })
        .unwrap();
    let mut kept = vec![false; n];
    kept[0] = true;
    kept[far] = true;
    keep(points, 0, far, tolerance, &mut kept);
    keep(points, far, n, tolerance, &mut kept);
    points
        .iter()
        .zip(kept)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// The closed loop through `points` (avatar vertices, at least three) along the surface: the
/// loop's vertices in order, each once.
pub fn surface_loop(mesh: &SurfaceMesh, points: &[usize]) -> Result<Vec<usize>, String> {
    if points.len() < 3 {
        return Err("Pick at least three points on the body".into());
    }
    if points.iter().any(|p| *p >= mesh.vertices.len()) {
        return Err("A picked point is not on the body".into());
    }
    let around = neighbours(mesh);
    let mut out: Vec<usize> = vec![];
    for k in 0..points.len() {
        let (a, b) = (points[k], points[(k + 1) % points.len()]);
        if a == b {
            continue;
        }
        let leg = path(mesh, &around, a, b)
            .ok_or("Two picked points are on separate parts of the body")?;
        out.extend_from_slice(&leg[..leg.len() - 1]);
    }
    let mut seen = std::collections::HashSet::new();
    if out.len() < 3 || !out.iter().all(|v| seen.insert(*v)) {
        return Err("The line crosses or touches itself; pick points further apart".into());
    }
    Ok(out)
}

/// The triangles of the smaller side of the body that `cycle` (from [`surface_loop`]) encloses.
pub fn enclosed(mesh: &SurfaceMesh, cycle: &[usize]) -> Result<Vec<usize>, String> {
    let n = cycle.len();
    let cut: std::collections::HashSet<(usize, usize)> = (0..n)
        .map(|k| {
            let (a, b) = (cycle[k], cycle[(k + 1) % n]);
            (a.min(b), a.max(b))
        })
        .collect();
    // Triangles across each edge.
    let mut by_edge: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            by_edge.entry((a.min(b), a.max(b))).or_default().push(t);
        }
    }
    // The triangles on each side of the loop's edges, grown without crossing the loop.
    let area = |t: usize| {
        let [a, b, c] = mesh.triangles[t].map(|v| mesh.vertices[v]);
        (b - a).cross(c - a).length() / 2.
    };
    let mut side = vec![usize::MAX; mesh.triangles.len()];
    let mut sides: Vec<(Vec<usize>, f64)> = vec![];
    for edge in &cut {
        for &start in by_edge.get(edge).into_iter().flatten() {
            if side[start] != usize::MAX {
                continue;
            }
            let id = sides.len();
            side[start] = id;
            let mut stack = vec![start];
            let mut members = vec![];
            let mut total = 0.;
            while let Some(t) = stack.pop() {
                members.push(t);
                total += area(t);
                let tri = mesh.triangles[t];
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    let e = (a.min(b), a.max(b));
                    if cut.contains(&e) {
                        continue;
                    }
                    for &u in &by_edge[&e] {
                        if side[u] == usize::MAX {
                            side[u] = id;
                            stack.push(u);
                        }
                    }
                }
            }
            sides.push((members, total));
        }
    }
    if sides.len() < 2 {
        return Err("The line does not close off a part of the body".into());
    }
    let (members, _) = sides
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    Ok(members)
}

/// Solve `apply(x) = b` for a symmetric positive definite operator by conjugate gradients.
fn conjugate_gradient(apply: impl Fn(&[f64]) -> Vec<f64>, b: &[f64], x: &mut [f64]) {
    let mut r: Vec<f64> = apply(x).iter().zip(b).map(|(a, b)| b - a).collect();
    let mut p = r.clone();
    let mut rr: f64 = r.iter().map(|v| v * v).sum();
    let scale: f64 = b.iter().map(|v| v * v).sum::<f64>().max(1e-30);
    for _ in 0..4 * b.len().max(50) {
        if rr <= 1e-24 * scale {
            break;
        }
        let ap = apply(&p);
        let pap: f64 = p.iter().zip(&ap).map(|(a, b)| a * b).sum();
        if pap.abs() < 1e-300 {
            break;
        }
        let alpha = rr / pap;
        for i in 0..x.len() {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let next: f64 = r.iter().map(|v| v * v).sum();
        let beta = next / rr;
        rr = next;
        for i in 0..p.len() {
            p[i] = r[i] + beta * p[i];
        }
    }
}

/// Flatten the patch made of the avatar `triangles` (from [`enclosed`]), whose boundary is
/// `cycle`.
pub fn flatten(
    mesh: &SurfaceMesh,
    triangles: &[usize],
    cycle: &[usize],
) -> Result<Flattened, String> {
    // Local numbering.
    let mut local: BTreeMap<usize, usize> = BTreeMap::new();
    let mut vertices = vec![];
    let mut tris: Vec<[usize; 3]> = vec![];
    for &t in triangles {
        tris.push(mesh.triangles[t].map(|v| {
            *local.entry(v).or_insert_with(|| {
                vertices.push(v);
                vertices.len() - 1
            })
        }));
    }
    let n = vertices.len();
    // A disk: one boundary loop, the cycle, and Euler characteristic one.
    let mut edges: HashMap<(usize, usize), usize> = HashMap::new();
    for t in &tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let boundary = edges.values().filter(|c| **c == 1).count();
    if n as i64 - edges.len() as i64 + tris.len() as i64 != 1 || boundary != cycle.len() {
        return Err("The enclosed part of the body has holes; draw around them".into());
    }
    let outline: Vec<usize> = cycle
        .iter()
        .map(|v| {
            local
                .get(v)
                .copied()
                .ok_or("The loop is not the patch's edge")
        })
        .collect::<Result<_, _>>()?;
    let x: Vec<DVec3> = vertices.iter().map(|v| mesh.vertices[*v]).collect();
    // Each triangle in its own plane, isometrically: corners (0,0), (|e1|,0), and the third.
    let shapes: Vec<[DVec2; 3]> = tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|k| x[k]);
            let e1 = b - a;
            let u = e1.normalize_or_zero();
            let w = c - a;
            let along = w.dot(u);
            let up = (w - u * along).length();
            [
                DVec2::ZERO,
                DVec2::new(e1.length(), 0.),
                DVec2::new(along, up),
            ]
        })
        .collect();
    // Least-squares conformal map, two far-apart outline points held.
    let first = outline[0];
    let second = *outline
        .iter()
        .max_by(|a, b| {
            x[**a]
                .distance(x[first])
                .total_cmp(&x[**b].distance(x[first]))
        })
        .unwrap();
    let held = [
        (first, DVec2::ZERO),
        (second, DVec2::new(x[first].distance(x[second]), 0.)),
    ];
    // Rows: per triangle, real and imaginary parts of Σ W_j U_j / sqrt(2A).
    let mut rows: Vec<Vec<(usize, f64)>> = vec![];
    for (t, s) in tris.iter().zip(&shapes) {
        let area2 = (s[1] - s[0]).perp_dot(s[2] - s[0]).abs().max(1e-18);
        let k = 1. / area2.sqrt();
        let w: Vec<DVec2> = (0..3).map(|j| s[(j + 2) % 3] - s[(j + 1) % 3]).collect();
        // (Wr + i Wi)(u + i v) = (Wr u - Wi v) + i (Wi u + Wr v)
        let mut re = vec![];
        let mut im = vec![];
        for j in 0..3 {
            let v = t[j];
            re.push((2 * v, w[j].x * k));
            re.push((2 * v + 1, -w[j].y * k));
            im.push((2 * v, w[j].y * k));
            im.push((2 * v + 1, w[j].x * k));
        }
        rows.push(re);
        rows.push(im);
    }
    let fixed: HashMap<usize, f64> = held
        .iter()
        .flat_map(|(v, p)| [(2 * v, p.x), (2 * v + 1, p.y)])
        .collect();
    let m = 2 * n;
    // Normal equations over the free unknowns: AᵀA x = Aᵀ(-A_fixed x_fixed).
    let rhs_rows: Vec<f64> = rows
        .iter()
        .map(|r| {
            -r.iter()
                .filter_map(|(c, a)| fixed.get(c).map(|v| a * v))
                .sum::<f64>()
        })
        .collect();
    let mut b = vec![0.; m];
    for (r, rhs) in rows.iter().zip(&rhs_rows) {
        for (c, a) in r {
            if !fixed.contains_key(c) {
                b[*c] += a * rhs;
            }
        }
    }
    let apply = |x: &[f64]| {
        let mut out = vec![0.; m];
        for r in &rows {
            let s: f64 = r
                .iter()
                .filter(|(c, _)| !fixed.contains_key(c))
                .map(|(c, a)| a * x[*c])
                .sum();
            for (c, a) in r {
                if !fixed.contains_key(c) {
                    out[*c] += a * s;
                }
            }
        }
        for c in fixed.keys() {
            out[*c] = x[*c];
        }
        out
    };
    let mut uv = vec![0.; m];
    conjugate_gradient(apply, &b, &mut uv);
    for (c, v) in &fixed {
        uv[*c] = *v;
    }
    let mut flat: Vec<DVec2> = (0..n)
        .map(|i| DVec2::new(uv[2 * i], uv[2 * i + 1]))
        .collect();
    // As-rigid-as-possible: each triangle's best rotation, then the positions fitting them.
    let cot = |a: DVec2, b: DVec2, c: DVec2| {
        let (u, v) = (b - a, c - a);
        (u.dot(v) / u.perp_dot(v).abs().max(1e-18)).max(1e-3)
    };
    let mut weights: Vec<[f64; 3]> = vec![];
    for s in &shapes {
        // Weight of the edge opposite corner j.
        weights.push([0, 1, 2].map(|j| cot(s[j], s[(j + 1) % 3], s[(j + 2) % 3]) / 2.));
    }
    // The cotangent Laplacian; one vertex is held where it is, eliminated so the system
    // stays symmetric.
    let anchor = first;
    let full = |x: &[f64]| {
        let mut out = vec![0.; m];
        for (t, w) in tris.iter().zip(&weights) {
            for j in 0..3 {
                let (a, b) = (t[(j + 1) % 3], t[(j + 2) % 3]);
                for d in 0..2 {
                    let diff = x[2 * a + d] - x[2 * b + d];
                    out[2 * a + d] += w[j] * diff;
                    out[2 * b + d] -= w[j] * diff;
                }
            }
        }
        out
    };
    let laplace = |x: &[f64]| {
        let mut y = x.to_vec();
        y[2 * anchor] = 0.;
        y[2 * anchor + 1] = 0.;
        let mut out = full(&y);
        out[2 * anchor] = x[2 * anchor];
        out[2 * anchor + 1] = x[2 * anchor + 1];
        out
    };
    for _ in 0..12 {
        let mut b = vec![0.; m];
        for ((t, s), w) in tris.iter().zip(&shapes).zip(&weights) {
            // Best rotation from the triangle's own shape to its flat one.
            let (mut a, mut c) = (0., 0.);
            for j in 0..3 {
                let (p, q) = (t[(j + 1) % 3], t[(j + 2) % 3]);
                let e0 = s[(j + 1) % 3] - s[(j + 2) % 3];
                let e1 = flat[p] - flat[q];
                a += w[j] * e0.dot(e1);
                c += w[j] * e0.perp_dot(e1);
            }
            let r = DVec2::new(a, c).normalize_or(DVec2::X);
            for j in 0..3 {
                let (p, q) = (t[(j + 1) % 3], t[(j + 2) % 3]);
                let e0 = s[(j + 1) % 3] - s[(j + 2) % 3];
                let rotated = DVec2::new(r.x * e0.x - r.y * e0.y, r.y * e0.x + r.x * e0.y);
                for d in 0..2 {
                    b[2 * p + d] += w[j] * rotated[d];
                    b[2 * q + d] -= w[j] * rotated[d];
                }
            }
        }
        let held = flat[anchor];
        let mut pinned = vec![0.; m];
        pinned[2 * anchor] = held.x;
        pinned[2 * anchor + 1] = held.y;
        for (bi, l) in b.iter_mut().zip(full(&pinned)) {
            *bi -= l;
        }
        b[2 * anchor] = held.x;
        b[2 * anchor + 1] = held.y;
        let mut next: Vec<f64> = flat.iter().flat_map(|p| [p.x, p.y]).collect();
        conjugate_gradient(laplace, &b, &mut next);
        flat = (0..n)
            .map(|i| DVec2::new(next[2 * i], next[2 * i + 1]))
            .collect();
        flat[anchor] = held;
    }
    // As seen from outside the body: the avatar's triangles turn counter-clockwise seen from
    // outside, so a map that turns them the other way is mirrored back.
    let turning: f64 = tris
        .iter()
        .map(|t| (flat[t[1]] - flat[t[0]]).perp_dot(flat[t[2]] - flat[t[0]]))
        .sum();
    if turning < 0. {
        for p in &mut flat {
            p.x = -p.x;
        }
    }
    // Upright: turned so the body's height grows along +y (least-squares slope over the patch).
    let mean = flat.iter().sum::<DVec2>() / n as f64;
    let height = x.iter().map(|p| p.y).sum::<f64>() / n as f64;
    let (mut sxx, mut sxy, mut syy, mut gx, mut gy) = (0., 0., 0., 0., 0.);
    for (f, p) in flat.iter().zip(&x) {
        let (d, h) = (*f - mean, p.y - height);
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
        gx += d.x * h;
        gy += d.y * h;
    }
    let det = sxx * syy - sxy * sxy;
    if det.abs() > 1e-18 {
        let slope = DVec2::new(syy * gx - sxy * gy, sxx * gy - sxy * gx) / det;
        if slope.length() > 0.1 {
            let turn = std::f64::consts::FRAC_PI_2 - slope.y.atan2(slope.x);
            let (sin, cos) = turn.sin_cos();
            for p in &mut flat {
                *p = DVec2::new(cos * p.x - sin * p.y, sin * p.x + cos * p.y);
            }
        }
    }
    // The outline counter-clockwise in the plane.
    let signed: f64 = (0..outline.len())
        .map(|k| flat[outline[k]].perp_dot(flat[outline[(k + 1) % outline.len()]]))
        .sum();
    let mut outline = outline;
    if signed < 0. {
        outline.reverse();
    }
    let mut squares = 0.;
    let mut largest: f64 = 0.;
    for (a, b) in edges.keys() {
        let body = x[*a].distance(x[*b]);
        let change = (flat[*a].distance(flat[*b]) - body).abs() / body.max(1e-12);
        squares += change * change;
        largest = largest.max(change);
    }
    let mut normals = vec![DVec3::ZERO; n];
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|v| mesh.vertices[v]);
        let face = (b - a).cross(c - a);
        for v in t {
            if let Some(k) = local.get(v) {
                normals[*k] += face;
            }
        }
    }
    let normals = normals.iter().map(|n| n.normalize_or(DVec3::Y)).collect();
    Ok(Flattened {
        vertices,
        flat,
        triangles: tris,
        normals,
        outline,
        stretch_rms: (squares / edges.len() as f64).sqrt(),
        stretch_max: largest,
    })
}

/// Trace a piece on the body through `points`: the loop, the patch it encloses, flattened.
pub fn trace(mesh: &SurfaceMesh, points: &[usize]) -> Result<Flattened, String> {
    let cycle = surface_loop(mesh, points)?;
    let patch = enclosed(mesh, &cycle)?;
    flatten(mesh, &patch, &cycle)
}
