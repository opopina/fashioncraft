//! Measurements on the body or the draped garment: lengths along a path of points and the
//! loop a level plane cuts around the mesh (a girth). Plain geometry on a triangle soup, so
//! the posed avatar and the simulated cloth measure the same way.
use glam::DVec3;
use std::collections::HashMap;

/// Length of a polyline, closed back to its start when `closed`.
pub fn path_length(points: &[DVec3], closed: bool) -> f64 {
    let open: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    match (closed, points.first(), points.last()) {
        (true, Some(a), Some(b)) if points.len() > 2 => open + a.distance(*b),
        _ => open,
    }
}

/// A cut through a mesh: its points in order and whether it closes on itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    pub points: Vec<DVec3>,
    pub closed: bool,
}

impl Cut {
    pub fn length(&self) -> f64 {
        path_length(&self.points, self.closed)
    }
}

/// Every chain the level plane `y = height` cuts through the triangles. A vertex lying
/// exactly on the plane counts as just above it, so no crossing is counted twice.
pub fn level_cuts(vertices: &[DVec3], triangles: &[[usize; 3]], height: f64) -> Vec<Cut> {
    let above = |v: usize| vertices[v].y >= height;
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let point = |(a, b): (usize, usize)| {
        let (p, q) = (vertices[a], vertices[b]);
        let t = ((height - p.y) / (q.y - p.y)).clamp(0., 1.);
        p + (q - p) * t
    };
    // Each crossed triangle joins the two crossed edges it has.
    let mut links: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for t in triangles {
        let crossed: Vec<(usize, usize)> = (0..3)
            .map(|k| (t[k], t[(k + 1) % 3]))
            .filter(|(a, b)| above(*a) != above(*b))
            .map(|(a, b)| key(a, b))
            .collect();
        if let [e, f] = crossed[..] {
            links.entry(e).or_default().push(f);
            links.entry(f).or_default().push(e);
        }
    }
    let mut cuts = vec![];
    let mut used: std::collections::HashSet<(usize, usize)> = Default::default();
    // Open chains first, from their ends, then the loops left over.
    let mut starts: Vec<(usize, usize)> = links
        .iter()
        .filter(|(_, n)| n.len() == 1)
        .map(|(e, _)| *e)
        .collect();
    starts.sort_unstable();
    let mut rest: Vec<(usize, usize)> = links.keys().copied().collect();
    rest.sort_unstable();
    starts.extend(rest);
    for start in starts {
        if used.contains(&start) {
            continue;
        }
        let mut chain = vec![start];
        used.insert(start);
        let mut at = start;
        while let Some(e) = links[&at].iter().copied().find(|e| !used.contains(e)) {
            used.insert(e);
            chain.push(e);
            at = e;
        }
        let closed = chain.len() > 2 && links[&at].contains(&start);
        cuts.push(Cut {
            points: chain.into_iter().map(point).collect(),
            closed,
        });
    }
    cuts
}

/// The level cut through `at` around the mesh: the cut at its height that passes nearest to
/// it. `None` when the plane misses the mesh.
pub fn girth(vertices: &[DVec3], triangles: &[[usize; 3]], at: DVec3) -> Option<Cut> {
    let distance = |cut: &Cut| {
        let n = cut.points.len();
        let segments = if cut.closed { n } else { n.saturating_sub(1) };
        (0..segments)
            .map(|k| segment_distance(at, cut.points[k], cut.points[(k + 1) % n]))
            .fold(f64::INFINITY, f64::min)
    };
    level_cuts(vertices, triangles, at.y)
        .into_iter()
        .filter(|c| c.points.len() >= 2)
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

fn segment_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-30)).clamp(0., 1.);
    p.distance(a + d * t)
}
