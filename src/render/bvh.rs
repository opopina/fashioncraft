//! A bounding-volume hierarchy over triangles, for the photoreal render's rays.
//!
//! Built on the CPU with the surface-area heuristic over binned centroids, and flattened
//! depth first: an inner node's first child is the next node, its second child is named by
//! the node. The GPU walks the same nodes; [`Bvh::intersect`] walks them here, for tests and
//! for picking the focus distance.
use glam::Vec3;

/// A node of the flattened tree, as the GPU reads it (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Node {
    pub min: [f32; 3],
    /// A leaf's first triangle in [`Bvh::order`]; an inner node's second child.
    pub start: u32,
    pub max: [f32; 3],
    /// A leaf's triangle count; zero for an inner node.
    pub count: u32,
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.count > 0
    }
}

/// The tree and the order its leaves list the triangles in.
#[derive(Clone, Debug, Default)]
pub struct Bvh {
    pub nodes: Vec<Node>,
    /// Original index of each triangle, in leaf order: leaves name ranges of this list.
    pub order: Vec<u32>,
}

/// Most triangles a leaf keeps.
const LEAF: usize = 4;
/// Centroid bins per split.
const BINS: usize = 16;
/// Deepest the tree may go; the GPU's traversal stack holds this many nodes.
pub const MAX_DEPTH: usize = 60;

#[derive(Clone, Copy)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };
    fn grow(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    fn join(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
    fn area(&self) -> f32 {
        let d = (self.max - self.min).max(Vec3::ZERO);
        2. * (d.x * d.y + d.y * d.z + d.z * d.x)
    }
}

/// A ray against a triangle (Möller–Trumbore): distance and barycentric (u, v).
pub fn ray_triangle(origin: Vec3, dir: Vec3, t: &[Vec3; 3]) -> Option<(f32, f32, f32)> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1. / det;
    let s = origin - t[0];
    let u = s.dot(p) * inv;
    if !(0. ..=1.).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0. || u + v > 1. {
        return None;
    }
    let d = e2.dot(q) * inv;
    (d > 0.).then_some((d, u, v))
}

fn slab(node: &Node, origin: Vec3, inv: Vec3, t_max: f32) -> bool {
    let t0 = (Vec3::from(node.min) - origin) * inv;
    let t1 = (Vec3::from(node.max) - origin) * inv;
    let near = t0.min(t1).max_element().max(0.);
    let far = t0.max(t1).min_element().min(t_max);
    near <= far
}

impl Bvh {
    /// The tree over `triangles` (degenerate ones included: they are simply never hit).
    pub fn build(triangles: &[[Vec3; 3]]) -> Self {
        let n = triangles.len();
        if n == 0 {
            return Self::default();
        }
        let bounds: Vec<Bounds> = triangles
            .iter()
            .map(|t| {
                let mut b = Bounds::EMPTY;
                for p in t {
                    b.grow(*p);
                }
                b
            })
            .collect();
        let centroids: Vec<Vec3> = bounds.iter().map(|b| (b.min + b.max) * 0.5).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes: Vec<Node> = Vec::with_capacity(2 * n / LEAF + 1);
        // Pending ranges: (start, end, depth, parent to point at this node when it is a second
        // child). First children are always built next, right after their parent.
        let mut stack: Vec<(usize, usize, usize, Option<usize>)> = vec![(0, n, 0, None)];
        while let Some((start, end, depth, parent)) = stack.pop() {
            let index = nodes.len();
            if let Some(parent) = parent {
                nodes[parent].start = index as u32;
            }
            let mut b = Bounds::EMPTY;
            let mut c = Bounds::EMPTY;
            for &t in &order[start..end] {
                b = b.join(bounds[t as usize]);
                c.grow(centroids[t as usize]);
            }
            nodes.push(Node {
                min: b.min.to_array(),
                start: start as u32,
                max: b.max.to_array(),
                count: (end - start) as u32,
            });
            let count = end - start;
            if count <= LEAF || depth + 1 >= MAX_DEPTH {
                continue;
            }
            let Some(mid) = split(&mut order[start..end], &bounds, &centroids, c, b) else {
                continue;
            };
            nodes[index].count = 0;
            nodes[index].start = 0;
            // The second child waits; the first is built next and lands right after.
            stack.push((start + mid, end, depth + 1, Some(index)));
            stack.push((start, start + mid, depth + 1, None));
        }
        Self { nodes, order }
    }

    /// The nearest triangle `dir` meets from `origin` within `t_max`: its original index,
    /// distance and barycentric (u, v).
    pub fn intersect(
        &self,
        triangles: &[[Vec3; 3]],
        origin: Vec3,
        dir: Vec3,
        mut t_max: f32,
    ) -> Option<(usize, f32, f32, f32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = dir.recip();
        let mut best = None;
        let mut stack = vec![0usize];
        while let Some(k) = stack.pop() {
            let node = &self.nodes[k];
            if !slab(node, origin, inv, t_max) {
                continue;
            }
            if node.is_leaf() {
                for &t in &self.order[node.start as usize..(node.start + node.count) as usize] {
                    if let Some((d, u, v)) = ray_triangle(origin, dir, &triangles[t as usize])
                        && d < t_max
                    {
                        t_max = d;
                        best = Some((t as usize, d, u, v));
                    }
                }
            } else {
                stack.push(node.start as usize);
                stack.push(k + 1);
            }
        }
        best
    }

    /// How deep the tree goes (a lone leaf is depth 1).
    pub fn depth(&self) -> usize {
        fn walk(nodes: &[Node], k: usize) -> usize {
            if nodes[k].is_leaf() {
                1
            } else {
                1 + walk(nodes, k + 1).max(walk(nodes, nodes[k].start as usize))
            }
        }
        if self.nodes.is_empty() {
            0
        } else {
            walk(&self.nodes, 0)
        }
    }
}

/// Reorder `order` around the cheapest binned surface-area split; the first part's length,
/// or none when no split beats a leaf.
fn split(
    order: &mut [u32],
    bounds: &[Bounds],
    centroids: &[Vec3],
    centre_bounds: Bounds,
    node_bounds: Bounds,
) -> Option<usize> {
    let extent = centre_bounds.max - centre_bounds.min;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    let lo = centre_bounds.min[axis];
    let span = extent[axis];
    let count = order.len();
    if span <= 0. || !span.is_finite() {
        // Every centroid in one place: halve the list.
        return Some(count / 2);
    }
    let bin_of = |t: u32| -> usize {
        (((centroids[t as usize][axis] - lo) / span * BINS as f32) as usize).min(BINS - 1)
    };
    let mut bin_bounds = [Bounds::EMPTY; BINS];
    let mut bin_count = [0usize; BINS];
    for &t in order.iter() {
        let b = bin_of(t);
        bin_bounds[b] = bin_bounds[b].join(bounds[t as usize]);
        bin_count[b] += 1;
    }
    // Costs of splitting after each bin, from the left and right sweeps.
    let mut left_area = [0f32; BINS - 1];
    let mut left_count = [0usize; BINS - 1];
    let (mut acc, mut n) = (Bounds::EMPTY, 0);
    for k in 0..BINS - 1 {
        acc = acc.join(bin_bounds[k]);
        n += bin_count[k];
        left_area[k] = acc.area();
        left_count[k] = n;
    }
    let (mut acc, mut n) = (Bounds::EMPTY, 0);
    let mut best = (f32::INFINITY, 0usize);
    for k in (1..BINS).rev() {
        acc = acc.join(bin_bounds[k]);
        n += bin_count[k];
        let left = left_count[k - 1];
        if left == 0 || n == 0 {
            continue;
        }
        let cost = left_area[k - 1] * left as f32 + acc.area() * n as f32;
        if cost < best.0 {
            best = (cost, k);
        }
    }
    let leaf_cost = node_bounds.area() * count as f32;
    if !best.0.is_finite() || (best.0 >= leaf_cost && count <= 2 * LEAF) {
        return (count > LEAF)
            .then_some(count / 2)
            .filter(|_| best.0.is_finite());
    }
    // Partition in place: bins before the split first.
    let mut i = 0;
    for j in 0..count {
        if bin_of(order[j]) < best.1 {
            order.swap(i, j);
            i += 1;
        }
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small pseudo-random generator, so the test is the same every run.
    fn random(state: &mut u64) -> f32 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*state >> 40) as f32) / (1u64 << 24) as f32
    }

    fn soup(n: usize, seed: u64) -> Vec<[Vec3; 3]> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                let c = Vec3::new(random(&mut s), random(&mut s), random(&mut s)) * 2. - 1.;
                let mut corner =
                    || c + (Vec3::new(random(&mut s), random(&mut s), random(&mut s)) - 0.5) * 0.2;
                [corner(), corner(), corner()]
            })
            .collect()
    }

    #[test]
    fn every_triangle_is_in_one_leaf_and_inside_its_bounds() {
        let triangles = soup(3000, 7);
        let bvh = Bvh::build(&triangles);
        let mut seen = vec![0; triangles.len()];
        for (k, node) in bvh.nodes.iter().enumerate() {
            if node.is_leaf() {
                assert!(node.count as usize <= LEAF * 2 || bvh.depth() >= MAX_DEPTH);
                for &t in &bvh.order[node.start as usize..(node.start + node.count) as usize] {
                    seen[t as usize] += 1;
                    for p in triangles[t as usize] {
                        assert!(
                            p.cmpge(Vec3::from(node.min) - 1e-6).all()
                                && p.cmple(Vec3::from(node.max) + 1e-6).all()
                        );
                    }
                }
            } else {
                // Children lie inside their parent.
                for child in [k + 1, node.start as usize] {
                    let c = &bvh.nodes[child];
                    assert!(Vec3::from(c.min).cmpge(Vec3::from(node.min) - 1e-6).all());
                    assert!(Vec3::from(c.max).cmple(Vec3::from(node.max) + 1e-6).all());
                }
            }
        }
        assert!(seen.iter().all(|&c| c == 1));
        assert!(bvh.depth() < 40, "depth {}", bvh.depth());
    }

    #[test]
    fn the_tree_finds_the_same_hits_as_testing_every_triangle() {
        let triangles = soup(2000, 11);
        let bvh = Bvh::build(&triangles);
        let mut s = 99;
        let mut hits = 0;
        for _ in 0..500 {
            let origin = Vec3::new(random(&mut s), random(&mut s), random(&mut s)) * 6. - 3.;
            let target = Vec3::new(random(&mut s), random(&mut s), random(&mut s)) - 0.5;
            let dir = (target - origin).normalize();
            let brute = triangles
                .iter()
                .enumerate()
                .filter_map(|(k, t)| ray_triangle(origin, dir, t).map(|(d, ..)| (k, d)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let tree = bvh
                .intersect(&triangles, origin, dir, f32::INFINITY)
                .map(|(k, d, ..)| (k, d));
            assert_eq!(tree.map(|h| h.0), brute.map(|h| h.0));
            if let (Some(a), Some(b)) = (tree, brute) {
                assert!((a.1 - b.1).abs() < 1e-5);
                hits += 1;
            }
        }
        assert!(hits > 100, "{hits} hits");
    }

    #[test]
    fn stacked_triangles_with_one_centroid_still_split() {
        let t = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let bvh = Bvh::build(&vec![t; 100]);
        assert!(bvh.nodes.len() > 1);
        assert!(
            bvh.intersect(&vec![t; 100], Vec3::new(0.2, 0.2, 1.), -Vec3::Z, 10.)
                .is_some()
        );
        assert!(Bvh::build(&[]).nodes.is_empty());
    }
}
