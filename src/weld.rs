//! Welding a garment's seams for export: the two edges of every seam become one, vertex to
//! vertex. ClothCraft sews edges whose meshes need not match, so each point where either side
//! has a vertex is first added to the other side's edge (the triangle there is split in two),
//! and then the points sewn together are merged into one. Every piece keeps its own vertices,
//! so texture coordinates stay per piece; merged vertices share one position. A piece whose
//! front faces the other way from the pieces it is sewn to is turned over, so the welded
//! surface has one outside.
use crate::garment::CompiledGarment;
use glam::DVec3;
use std::collections::{BTreeMap, HashMap};

/// A point this close to an end of its edge (as a share of the edge) is that end.
const SNAP: f64 = 1e-6;

/// A vertex added on an edge of a piece: `edge[0]` moved `t` of the way to `edge[1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Added {
    pub edge: [usize; 2],
    pub t: f64,
}

/// The pieces with their seams welded.
#[derive(Clone, Debug)]
pub struct Welded {
    /// The garment's vertex count: added vertices are numbered from here.
    pub vertices: usize,
    /// Vertices added on seam edges.
    pub added: Vec<Added>,
    /// Each piece's front-wound triangles, split where vertices were added; triangles whose
    /// corners were merged together are dropped.
    pub triangles: Vec<Vec<[usize; 3]>>,
    /// Each piece's added vertices.
    pub piece_added: Vec<Vec<usize>>,
    /// Each vertex's weld group (garment vertices, then added ones), numbered in vertex order:
    /// the vertices of a group are one point.
    pub group: Vec<usize>,
    pub groups: usize,
    /// Pieces turned over to face the same way as the pieces they are sewn to: their
    /// triangles are wound the other way.
    pub flipped: Vec<bool>,
    /// Where each sewn point (an edge and how far along it, from the lower vertex) ended up.
    at: HashMap<(usize, usize), Vec<(f64, usize)>>,
}

impl Welded {
    /// Nothing welded: each piece's triangles as they are, every vertex a group of its own.
    pub fn none(garment: &CompiledGarment, triangles: &[[usize; 3]]) -> Self {
        let n = garment.mesh.uv.len();
        Self {
            vertices: n,
            added: vec![],
            triangles: garment
                .panels
                .iter()
                .map(|p| triangles[p.triangles.clone()].to_vec())
                .collect(),
            piece_added: vec![vec![]; garment.panels.len()],
            group: (0..n).collect(),
            groups: n,
            flipped: vec![false; garment.panels.len()],
            at: HashMap::new(),
        }
    }

    /// `values` of the garment's vertices, extended to the added ones along their edges.
    pub fn extend<T: Copy>(&self, values: &[T], mix: impl Fn(T, T, f64) -> T) -> Vec<T> {
        let mut out = values.to_vec();
        out.extend(
            self.added
                .iter()
                .map(|a| mix(values[a.edge[0]], values[a.edge[1]], a.t)),
        );
        out
    }

    /// Each group's position: the mean of its vertices' (`x` of every vertex, added ones too).
    pub fn positions(&self, x: &[DVec3]) -> Vec<DVec3> {
        let mut sum = vec![DVec3::ZERO; self.groups];
        let mut count = vec![0usize; self.groups];
        for (i, p) in x.iter().enumerate() {
            let g = self.group[i];
            // A group of one keeps its vertex exactly.
            sum[g] = if count[g] == 0 { *p } else { sum[g] + *p };
            count[g] += 1;
        }
        sum.iter()
            .zip(&count)
            .map(|(s, c)| if *c > 1 { *s / *c as f64 } else { *s })
            .collect()
    }

    /// The vertex at a point of a piece's edge given as two vertices and their weights (as a
    /// seam gives it): one of the two, or the vertex added there; none when nothing is there.
    pub fn vertex_at(&self, point: [(usize, f64); 2]) -> Option<usize> {
        let [(a, wa), (b, wb)] = point;
        if wb <= SNAP {
            return Some(a);
        }
        if wa <= SNAP {
            return Some(b);
        }
        let t = if a < b { wb } else { wa };
        self.at
            .get(&(a.min(b), a.max(b)))?
            .iter()
            .find(|(s, _)| (s - t).abs() <= SNAP)
            .map(|(_, v)| *v)
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Weld the seams of `garment` whose two sides are both on pieces marked in `pieces` (one flag
/// per piece). `triangles` are the garment's triangles wound to face front.
pub fn weld(garment: &CompiledGarment, triangles: &[[usize; 3]], pieces: &[bool]) -> Welded {
    let mut out = Welded::none(garment, triangles);
    let n = out.vertices;
    let mut piece_of = vec![usize::MAX; n];
    for (k, p) in garment.panels.iter().enumerate() {
        piece_of[p.vertices.clone()].fill(k);
    }
    let kept = |v: usize| pieces.get(piece_of[v]).copied().unwrap_or(false);
    // Every directed edge of the kept pieces' triangles, and the triangle that has it.
    let mut tris: Vec<[usize; 3]> = vec![];
    let mut owner: Vec<usize> = vec![];
    for (k, p) in garment.panels.iter().enumerate() {
        if pieces.get(k).copied().unwrap_or(false) {
            for t in &triangles[p.triangles.clone()] {
                tris.push(*t);
                owner.push(k);
            }
        }
    }
    let mut edge: HashMap<(usize, usize), usize> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            edge.insert((t[k], t[(k + 1) % 3]), i);
        }
    }
    // Each sewn point as a vertex, adding vertices inside edges where needed.
    let mut splits: BTreeMap<(usize, usize), Vec<(f64, usize)>> = BTreeMap::new();
    let mut added: Vec<Added> = vec![];
    // A point that can be welded: on a kept piece, at a vertex or inside a triangle's edge.
    let weldable = |p: [(usize, f64); 2]| {
        let [(a, wa), (b, wb)] = p;
        kept(a)
            && kept(b)
            && piece_of[a] == piece_of[b]
            && (wa <= SNAP
                || wb <= SNAP
                || edge.contains_key(&(a, b))
                || edge.contains_key(&(b, a)))
    };
    let mut locate = |p: [(usize, f64); 2]| -> usize {
        let [(a, wa), (b, wb)] = p;
        if wb <= SNAP {
            return a;
        }
        if wa <= SNAP {
            return b;
        }
        let key = (a.min(b), a.max(b));
        let t = if a < b { wb } else { wa };
        let list = splits.entry(key).or_default();
        if let Some((_, v)) = list.iter().find(|(s, _)| (s - t).abs() <= SNAP) {
            return *v;
        }
        let v = n + added.len();
        added.push(Added {
            edge: [key.0, key.1],
            t,
        });
        list.push((t, v));
        v
    };
    let mut links = vec![];
    for seam in &garment.seams {
        for [a, b] in &seam.pairs {
            if weldable(*a) && weldable(*b) {
                links.push((locate(*a), locate(*b)));
            }
        }
    }
    // Split the triangles at the added vertices, edge by edge, from each edge's lower end.
    for ((lo, hi), list) in &mut splits {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut from = *lo;
        for (_, m) in list.iter() {
            for (u, v) in [(from, *hi), (*hi, from)] {
                let Some(i) = edge.remove(&(u, v)) else {
                    continue;
                };
                let t = tris[i];
                let k = (0..3).find(|k| t[*k] == u).unwrap();
                let c = t[(k + 2) % 3];
                let j = tris.len();
                tris[i] = [u, *m, c];
                tris.push([*m, v, c]);
                owner.push(owner[i]);
                for (e, at) in [
                    ((u, *m), i),
                    ((*m, c), i),
                    ((*m, v), j),
                    ((v, c), j),
                    ((c, *m), j),
                ] {
                    edge.insert(e, at);
                }
            }
            from = *m;
        }
    }
    // Merge what is sewn together.
    let total = n + added.len();
    let mut parent: Vec<usize> = (0..total).collect();
    for (a, b) in links {
        let (a, b) = (find(&mut parent, a), find(&mut parent, b));
        if a != b {
            parent[a.max(b)] = a.min(b);
        }
    }
    let mut number = vec![usize::MAX; total];
    let mut groups = 0;
    let group: Vec<usize> = (0..total)
        .map(|i| {
            let root = find(&mut parent, i);
            if number[root] == usize::MAX {
                number[root] = groups;
                groups += 1;
            }
            number[root]
        })
        .collect();
    let mut triangles = vec![vec![]; garment.panels.len()];
    for (t, k) in tris.iter().zip(&owner) {
        let [a, b, c] = t.map(|i| group[i]);
        if a != b && b != c && c != a {
            triangles[*k].push(*t);
        }
    }
    // One outside: a piece sewn to one whose front faces the other way is turned over, so
    // every welded edge runs opposite ways in its two triangles. The first piece of each
    // sewn set keeps its side.
    let mut runs: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (k, list) in triangles.iter().enumerate() {
        for t in list {
            for i in 0..3 {
                runs.entry((group[t[i]], group[t[(i + 1) % 3]]))
                    .or_default()
                    .push(k);
            }
        }
    }
    // Per pair of pieces: edges they share running opposite ways, less those running alike.
    let mut agree: BTreeMap<(usize, usize), i64> = BTreeMap::new();
    for ((a, b), pieces) in &runs {
        for k in pieces {
            for j in runs.get(&(*b, *a)).into_iter().flatten() {
                if j != k {
                    *agree.entry((*k, *j)).or_default() += 1;
                }
            }
            for j in pieces {
                if j != k {
                    *agree.entry((*k, *j)).or_default() -= 1;
                }
            }
        }
    }
    let mut flipped = vec![false; garment.panels.len()];
    let mut seen = vec![false; garment.panels.len()];
    for start in 0..garment.panels.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        while let Some(k) = stack.pop() {
            for (&(_, j), &votes) in agree.range((k, 0)..=(k, usize::MAX)) {
                if votes != 0 && !seen[j] {
                    seen[j] = true;
                    flipped[j] = flipped[k] != (votes < 0);
                    stack.push(j);
                }
            }
        }
    }
    for (list, flip) in triangles.iter_mut().zip(&flipped) {
        if *flip {
            for t in list {
                t.swap(1, 2);
            }
        }
    }
    let mut piece_added = vec![vec![]; garment.panels.len()];
    for (k, a) in added.iter().enumerate() {
        piece_added[piece_of[a.edge[0]]].push(n + k);
    }
    out.added = added;
    out.triangles = triangles;
    out.piece_added = piece_added;
    out.group = group;
    out.groups = groups;
    out.flipped = flipped;
    out.at = splits.into_iter().collect();
    out
}
