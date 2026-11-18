//! Retopology for export: each piece remeshed as quads of a chosen size, laid along its grain
//! in the pattern and carried onto the draped cloth, with triangles only in the strip where
//! the grid meets the outline (paired into quads where two make a good one). A new vertex is
//! a point of one of the garment's triangles, so the new mesh lies on the drape; like a
//! piece's own mesh it ends at the piece's outline, and pieces are not joined to each other.
use crate::garment::CompiledGarment;
use glam::{DVec2, DVec3};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::{Add, Mul},
};

/// A new vertex: a point of one of the garment's triangles, as its corners and their weights.
pub type Point = [(usize, f64); 3];

/// A piece remeshed.
#[derive(Clone, Debug, Default)]
pub struct Piece {
    /// Its vertices, as indices into [`Retopology::points`].
    pub vertices: Vec<usize>,
    /// Its faces, wound like the piece's triangles: quads, and triangles near the outline.
    pub faces: Vec<Vec<usize>>,
}

/// The remeshed pieces: one per piece of the garment, empty for pieces left out.
#[derive(Clone, Debug)]
pub struct Retopology {
    pub points: Vec<Point>,
    pub pieces: Vec<Piece>,
}

impl Retopology {
    /// A quantity given at the garment's vertices, at the new vertices.
    pub fn sample<T: Copy + Mul<f64, Output = T> + Add<Output = T>>(&self, values: &[T]) -> Vec<T> {
        self.points
            .iter()
            .map(|[(a, wa), (b, wb), (c, wc)]| {
                values[*a] * *wa + values[*b] * *wb + values[*c] * *wc
            })
            .collect()
    }
}

/// Barycentric weights of `p` in the triangle `t`.
fn weights(t: [DVec2; 3], p: DVec2) -> [f64; 3] {
    let area = (t[1] - t[0]).perp_dot(t[2] - t[0]);
    let w1 = (p - t[0]).perp_dot(t[2] - t[0]) / area;
    let w2 = (t[1] - t[0]).perp_dot(p - t[0]) / area;
    [1. - w1 - w2, w1, w2]
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-300)).clamp(0., 1.);
    p.distance(a + d * t)
}

/// Whether segments `a`–`b` and `c`–`d` cross.
fn cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    side(a, b, c) * side(a, b, d) <= 0. && side(c, d, a) * side(c, d, b) <= 0.
}

/// Remesh the pieces marked in `pieces` (one flag per piece) as quads of `size` metres.
/// `triangles` are the garment's triangles wound to face front.
pub fn retopologize(
    garment: &CompiledGarment,
    triangles: &[[usize; 3]],
    size: f64,
    pieces: &[bool],
) -> Result<Retopology, String> {
    if !(0.002..=0.5).contains(&size) {
        return Err("Retopologize with quads of 2 to 500 mm".into());
    }
    let h = size;
    let uv = &garment.mesh.uv;
    let mut points: Vec<Point> = vec![];
    let mut out = vec![Piece::default(); garment.panels.len()];
    for (k, panel) in garment.panels.iter().enumerate() {
        if !pieces.get(k).copied().unwrap_or(false) {
            continue;
        }
        let tris = &triangles[panel.triangles.clone()];
        if tris.is_empty() {
            continue;
        }
        let area = |t: &[usize; 3]| (uv[t[1]] - uv[t[0]]).perp_dot(uv[t[2]] - uv[t[0]]);
        // The side the piece's triangles turn in the pattern: new faces turn the same way.
        let turn = tris.iter().map(area).sum::<f64>().signum();
        let (sin, cos) = garment.materials[panel.material]
            .grain_degrees
            .to_radians()
            .sin_cos();
        let to_grid = |p: DVec2| DVec2::new(p.x * cos + p.y * sin, -p.x * sin + p.y * cos);
        let from_grid = |q: DVec2| DVec2::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos);
        // The outline: edges of one triangle, directed as in it, with its third corner.
        let mut uses: HashMap<(usize, usize), usize> = HashMap::new();
        for t in tris {
            for i in 0..3 {
                let (a, b) = (t[i], t[(i + 1) % 3]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let mut next: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        let mut segments: Vec<(DVec2, DVec2)> = vec![];
        for t in tris {
            for i in 0..3 {
                let (a, b, c) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                if uses[&(a.min(b), a.max(b))] == 1 {
                    next.entry(a).or_default().push((b, c));
                    segments.push((uv[a], uv[b]));
                }
            }
        }
        let inside = |p: DVec2| {
            segments
                .iter()
                .filter(|(a, b)| {
                    (a.y > p.y) != (b.y > p.y)
                        && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x)
                })
                .count()
                % 2
                == 1
        };
        let clearance = |p: DVec2| {
            segments
                .iter()
                .map(|(a, b)| segment_distance(p, *a, *b))
                .fold(f64::INFINITY, f64::min)
        };
        let mut new_point = |point: Point, piece: &mut Piece| {
            points.push(point);
            piece.vertices.push(points.len() - 1);
            points.len() - 1
        };
        let piece = &mut out[k];
        let mut position: HashMap<usize, DVec2> = HashMap::new();
        // The outline sampled about every `h`, keeping its corners.
        let mut loops: Vec<Vec<usize>> = vec![];
        let mut taken: HashSet<(usize, usize)> = HashSet::new();
        for (&start, list) in &next {
            for &(first, third) in list {
                if !taken.insert((start, first)) {
                    continue;
                }
                // The loop's vertices and, for each edge, the third corner of its triangle.
                let mut chain = vec![(start, third)];
                let mut at = first;
                while at != start {
                    let Some(&(b, c)) = next
                        .get(&at)
                        .and_then(|l| l.iter().find(|(b, _)| !taken.contains(&(at, *b))))
                    else {
                        break;
                    };
                    taken.insert((at, b));
                    chain.push((at, c));
                    at = b;
                }
                if at != start || chain.len() < 3 {
                    continue;
                }
                let n = chain.len();
                let p = |i: usize| uv[chain[i % n].0];
                let corner = |i: usize| {
                    let (u, v) = (p(i) - p(i + n - 1), p(i + 1) - p(i));
                    u.angle_to(v).abs() > 30f64.to_radians()
                };
                let mut corners: Vec<usize> = (0..n).filter(|i| corner(*i)).collect();
                if corners.is_empty() {
                    corners.push(0);
                }
                let mut samples = vec![];
                for (c, &from) in corners.iter().enumerate() {
                    let to = corners[(c + 1) % corners.len()];
                    let span = (to + n - from - 1) % n + 1;
                    let length: f64 = (0..span)
                        .map(|i| p(from + i).distance(p(from + i + 1)))
                        .sum();
                    let count = ((length / h).round() as usize).max(1);
                    let (mut i, mut walked) = (0, 0.);
                    for s in 0..count {
                        let goal = length * s as f64 / count as f64;
                        while i + 1 < span && walked + p(from + i).distance(p(from + i + 1)) <= goal
                        {
                            walked += p(from + i).distance(p(from + i + 1));
                            i += 1;
                        }
                        let (a, b) = ((from + i) % n, (from + i + 1) % n);
                        let edge = p(a).distance(p(b));
                        let t = if edge > 0. {
                            ((goal - walked) / edge).clamp(0., 1.)
                        } else {
                            0.
                        };
                        let point = [(chain[a].0, 1. - t), (chain[b].0, t), (chain[a].1, 0.)];
                        let v = new_point(point, piece);
                        position.insert(v, p(a).lerp(p(b), t));
                        samples.push(v);
                    }
                }
                loops.push(samples);
            }
        }
        // Grid nodes well inside the piece, on the triangle under each.
        let (lo, hi) = tris.iter().flatten().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), v| (lo.min(to_grid(uv[*v])), hi.max(to_grid(uv[*v]))),
        );
        let mut node: HashMap<(i64, i64), usize> = HashMap::new();
        for j in (lo.y / h).floor() as i64..=(hi.y / h).ceil() as i64 {
            for i in (lo.x / h).floor() as i64..=(hi.x / h).ceil() as i64 {
                let q = from_grid(DVec2::new(i as f64, j as f64) * h);
                if !inside(q) || clearance(q) < 0.5 * h {
                    continue;
                }
                let (mut best, mut best_w) = (None, f64::NEG_INFINITY);
                for t in tris {
                    let w = weights(t.map(|v| uv[v]), q);
                    let low = w[0].min(w[1]).min(w[2]);
                    if low > best_w {
                        (best, best_w) = (Some((t, w)), low);
                    }
                }
                let Some((t, w)) = best else {
                    continue;
                };
                let w = w.map(|w| w.max(0.));
                let sum = w[0] + w[1] + w[2];
                let point = [(t[0], w[0] / sum), (t[1], w[1] / sum), (t[2], w[2] / sum)];
                let v = new_point(point, piece);
                position.insert(v, q);
                node.insert((i, j), v);
            }
        }
        // Quads where all four corners are nodes and no outline passes through the cell.
        let mut cells: HashSet<(i64, i64)> = HashSet::new();
        for &(i, j) in node.keys() {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            if !corners.iter().all(|c| node.contains_key(c)) {
                continue;
            }
            let at: Vec<DVec2> = corners.iter().map(|c| position[&node[c]]).collect();
            let crossed = segments.iter().any(|(a, b)| {
                (0..4).any(|e| cross(*a, *b, at[e], at[(e + 1) % 4]))
                    || (0..4).all(|e| (at[(e + 1) % 4] - at[e]).perp_dot(*a - at[e]) > 0.)
            });
            if !crossed {
                cells.insert((i, j));
            }
        }
        let cell_of = |p: DVec2| {
            let q = to_grid(p) / h;
            (q.x.floor() as i64, q.y.floor() as i64)
        };
        let mut sorted: Vec<(i64, i64)> = cells.iter().copied().collect();
        sorted.sort_unstable();
        for &(i, j) in &sorted {
            let mut face: Vec<usize> = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                .iter()
                .map(|c| node[c])
                .collect();
            if turn < 0. {
                face.reverse();
            }
            piece.faces.push(face);
        }
        // The strip between the quads and the outline, triangulated.
        let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
        let mut vertex_of: HashMap<usize, usize> = HashMap::new();
        let mut handle = |cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>, v: usize| {
            let p = position[&v];
            let fixed = cdt.insert(Point2::new(p.x, p.y)).ok()?;
            vertex_of.entry(fixed.index()).or_insert(v);
            Some(fixed)
        };
        for samples in &loops {
            let handles: Vec<_> = samples
                .iter()
                .filter_map(|v| handle(&mut cdt, *v))
                .collect();
            for e in 0..handles.len() {
                let (a, b) = (handles[e], handles[(e + 1) % handles.len()]);
                if a != b && cdt.can_add_constraint(a, b) {
                    cdt.add_constraint(a, b);
                }
            }
        }
        let mut sorted_nodes: Vec<(&(i64, i64), &usize)> = node.iter().collect();
        sorted_nodes.sort_unstable();
        let mut border: HashMap<usize, _> = HashMap::new();
        for &(&(i, j), &v) in &sorted_nodes {
            let around = [(i, j), (i - 1, j), (i - 1, j - 1), (i, j - 1)];
            if !around.iter().all(|c| cells.contains(c))
                && let Some(h) = handle(&mut cdt, v)
            {
                border.insert(v, h);
            }
        }
        for &(i, j) in &sorted {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let across = [(i, j - 1), (i + 1, j), (i, j + 1), (i - 1, j)];
            for e in 0..4 {
                if cells.contains(&across[e]) {
                    continue;
                }
                let (a, b) = (node[&corners[e]], node[&corners[(e + 1) % 4]]);
                if let (Some(&a), Some(&b)) = (border.get(&a), border.get(&b))
                    && cdt.can_add_constraint(a, b)
                {
                    cdt.add_constraint(a, b);
                }
            }
        }
        let mut strip: Vec<[usize; 3]> = vec![];
        for f in cdt.inner_faces() {
            let corners = f.vertices().map(|v| vertex_of[&v.fix().index()]);
            let at = corners.map(|v| position[&v]);
            let centre = (at[0] + at[1] + at[2]) / 3.;
            let signed = (at[1] - at[0]).perp_dot(at[2] - at[0]);
            if signed.abs() < 1e-14 || !inside(centre) || cells.contains(&cell_of(centre)) {
                continue;
            }
            let [a, b, c] = corners;
            strip.push(if signed * turn > 0. {
                [a, b, c]
            } else {
                [a, c, b]
            });
        }
        // Pair the strip's triangles into quads where they make good ones.
        let mut flat = vec![DVec3::ZERO; points.len()];
        for (v, p) in &position {
            flat[*v] = DVec3::new(p.x, p.y, 0.);
        }
        piece.faces.extend(crate::obj_export::quads(&flat, &strip));
    }
    Ok(Retopology {
        points,
        pieces: out,
    })
}
