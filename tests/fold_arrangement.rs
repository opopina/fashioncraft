//! Fold arrangement: one side of an internal line turned about it as the piece lies, so the
//! simulation starts folded at the line's angle; toward the piece's front below 180°.
#[allow(dead_code)]
#[path = "support/folds.rs"]
mod support;
use fashioncraft::garment::transfer::fold_about_line;
use glam::{DVec2, DVec3};
use support::{creased, halves};

const CREASE: [DVec2; 2] = [DVec2::new(0.2, 0.002), DVec2::new(0.2, 0.298)];

fn folded(angle: f64, other_side: bool) -> (fashioncraft::garment::CompiledGarment, Vec<DVec3>) {
    let g = creased().compile().unwrap();
    let mut x = g.initial_state.x.clone();
    let id = g.panels[0].id.clone();
    for (v, p) in fold_about_line(&g, &x, &id, CREASE, angle, other_side).unwrap() {
        x[v] = p;
    }
    (g, x)
}

#[test]
fn ninety_degrees_turns_one_half_square_toward_the_front() {
    let (g, x) = folded(90., false);
    let mut s = g.initial_state.clone();
    s.x = x.clone();
    let (l, r) = halves(&g, &s);
    let angle = l.angle_between(r).to_degrees();
    assert!((angle - 90.).abs() < 1e-6, "{angle}°");
    // Rigid: every vertex keeps its distance to the crease.
    let x0 = &g.initial_state.x;
    let axis_x = |p: DVec3| DVec2::new(p.x, p.z);
    let crease_at = axis_x(x0[0]) + DVec2::new(0.2, 0.);
    for (p, q) in x0.iter().zip(&x) {
        let (d0, d1) = (
            axis_x(*p).distance(crease_at),
            axis_x(*q).distance(crease_at),
        );
        assert!((d0 - d1).abs() < 1e-9);
    }
    // Toward the side the piece's normal faces.
    let [a, b, c] = g.mesh.triangles[0].vertices;
    let normal = (x0[b] - x0[a]).cross(x0[c] - x0[a]).normalize();
    let moved: DVec3 = x.iter().zip(x0).map(|(p, q)| *p - *q).sum();
    assert!(moved.dot(normal) > 0.);
}

#[test]
fn the_other_angle_and_the_other_side_turn_the_other_way() {
    let side = |angle: f64, other: bool| {
        let (g, x) = folded(angle, other);
        let mut s = g.initial_state.clone();
        s.x = x;
        let (l, r) = halves(&g, &s);
        l.cross(r).dot(DVec3::NEG_Y).signum()
    };
    assert_eq!(side(90., false), -side(270., false));
    // Moving the other half to the same fold angle gives the same crease direction.
    assert_eq!(side(90., false), side(90., true));
    let (g, x) = folded(180., false);
    assert!(
        x.iter()
            .zip(&g.initial_state.x)
            .all(|(p, q)| p.distance(*q) < 1e-12)
    );
}

#[test]
fn a_line_off_the_piece_or_a_bad_angle_is_refused() {
    let g = creased().compile().unwrap();
    let x = g.initial_state.x.clone();
    let id = g.panels[0].id.clone();
    let off = [DVec2::new(0.9, 0.1), DVec2::new(0.9, 0.2)];
    assert!(fold_about_line(&g, &x, &id, off, 90., false).is_err());
    assert!(fold_about_line(&g, &x, &id, CREASE, 400., false).is_err());
    assert!(fold_about_line(&g, &x, "nobody", CREASE, 90., false).is_err());
}
