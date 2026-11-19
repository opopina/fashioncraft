//! Measuring on meshes: a level plane cuts closed loops around tubes and open chains across
//! sheets, the girth through a point is the loop nearest to it, and paths add up their legs.
use fashioncraft::measure3d::{girth, level_cuts, path_length};
use glam::DVec3;
use std::f64::consts::TAU;

/// An open tube along y: `n` sides of radius `r`, from y 0 to `h`, centred at `x`.
fn tube(x: f64, r: f64, h: f64, n: usize) -> (Vec<DVec3>, Vec<[usize; 3]>) {
    let mut v = vec![];
    for ring in 0..2 {
        for k in 0..n {
            let a = TAU * k as f64 / n as f64;
            v.push(DVec3::new(x + r * a.cos(), h * ring as f64, r * a.sin()));
        }
    }
    let mut t = vec![];
    for k in 0..n {
        let (a, b) = (k, (k + 1) % n);
        t.push([a, b, n + b]);
        t.push([a, n + b, n + a]);
    }
    (v, t)
}

fn merge(
    (mut v, mut t): (Vec<DVec3>, Vec<[usize; 3]>),
    (w, s): (Vec<DVec3>, Vec<[usize; 3]>),
) -> (Vec<DVec3>, Vec<[usize; 3]>) {
    let base = v.len();
    v.extend(w);
    t.extend(s.into_iter().map(|f| f.map(|i| i + base)));
    (v, t)
}

#[test]
fn a_level_plane_cuts_a_closed_loop_around_a_tube() {
    let (n, r) = (32, 0.15);
    let (v, t) = tube(0., r, 1., n);
    let cuts = level_cuts(&v, &t, 0.4);
    assert_eq!(cuts.len(), 1);
    assert!(cuts[0].closed);
    // A point on every side edge and on every face's diagonal, all on the faces' chords.
    assert_eq!(cuts[0].points.len(), 2 * n);
    assert!(cuts[0].points.iter().all(|p| (p.y - 0.4).abs() < 1e-12));
    // The cut is the n-gon's perimeter, a hair under the circle's.
    let polygon = 2. * n as f64 * r * (std::f64::consts::PI / n as f64).sin();
    assert!(
        (cuts[0].length() - polygon).abs() < 1e-9,
        "{}",
        cuts[0].length()
    );
    assert!(cuts[0].length() < TAU * r);
    // Above or below the mesh, nothing.
    assert!(level_cuts(&v, &t, 1.5).is_empty());
    assert!(girth(&v, &t, DVec3::new(0., -0.2, 0.)).is_none());
}

#[test]
fn the_girth_through_a_point_is_the_loop_nearest_to_it() {
    // Two legs: the girth taken on the right one measures the right one only.
    let legs = merge(tube(-0.12, 0.06, 0.8, 24), tube(0.12, 0.08, 0.8, 24));
    let (v, t) = (&legs.0, &legs.1);
    assert_eq!(level_cuts(v, t, 0.3).len(), 2);
    let right = girth(v, t, DVec3::new(0.2, 0.3, 0.)).unwrap();
    let left = girth(v, t, DVec3::new(-0.18, 0.3, 0.)).unwrap();
    assert!(right.closed && left.closed);
    assert!((right.length() - 2. * 24. * 0.08 * (std::f64::consts::PI / 24.).sin()).abs() < 1e-9);
    assert!((left.length() - 2. * 24. * 0.06 * (std::f64::consts::PI / 24.).sin()).abs() < 1e-9);
}

#[test]
fn a_sheet_gives_an_open_chain_and_paths_add_their_legs() {
    // A vertical sheet 0.5 m wide in five strips: the cut runs across it, open.
    let mut v = vec![];
    for row in 0..2 {
        for col in 0..=5 {
            v.push(DVec3::new(0.1 * col as f64, row as f64, 0.));
        }
    }
    let mut t = vec![];
    for col in 0..5 {
        t.push([col, col + 1, 7 + col]);
        t.push([col, 7 + col, 6 + col]);
    }
    let cuts = level_cuts(&v, &t, 0.25);
    assert_eq!(cuts.len(), 1);
    assert!(!cuts[0].closed);
    assert!((cuts[0].length() - 0.5).abs() < 1e-12);
    // A vertex exactly on the plane is not counted twice.
    let on = level_cuts(&v, &t, 0.);
    assert!(on.iter().all(|c| c.points.windows(2).all(|w| w[0] != w[1])));
    let square = [
        DVec3::ZERO,
        DVec3::X,
        DVec3::new(1., 1., 0.),
        DVec3::new(0., 1., 0.),
    ];
    assert_eq!(path_length(&square, false), 3.);
    assert_eq!(path_length(&square, true), 4.);
    assert_eq!(path_length(&square[..2], true), 1.);
    assert_eq!(path_length(&[], true), 0.);
}
