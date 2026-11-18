//! Folds along internal lines: the hinges on the line get a rest angle and a stiffer crease.
//! A weightless rectangle creased down the middle at 90° settles with its halves at right
//! angles; at 180° it stays flat; at 270° it folds the other way.
#[path = "support/folds.rs"]
mod support;
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    garment::template::LineFold,
};
use support::{creased, folded, halves, settle};

#[test]
fn a_crease_at_ninety_degrees_folds_the_halves_square() {
    let (g, s) = settle(&folded(&creased(), 90.));
    let (l, r) = halves(&g, &s);
    let angle = l.angle_between(r).to_degrees();
    assert!((angle - 90.).abs() < 10., "{angle}°");
    // Below 180° the halves fold toward the side the piece's normal faces: both halves end up
    // on that side of the crease, seen from the start.
    let x0 = &g.initial_state.x;
    let [a, b, c] = g.mesh.triangles[0].vertices;
    let normal = (x0[b] - x0[a]).cross(x0[c] - x0[a]).normalize();
    let crease: Vec<_> = (0..s.x.len())
        .filter(|&i| (g.mesh.uv[i].x - 0.2).abs() < 1e-9)
        .collect();
    let centre = |x: &[DVec3]| crease.iter().map(|&i| x[i]).sum::<DVec3>() / crease.len() as f64;
    let towards = (s.x.iter().sum::<DVec3>() / s.x.len() as f64 - centre(&s.x)).dot(normal);
    assert!(towards > 0.02, "folds toward the front: {towards}");
}

#[test]
fn a_crease_at_180_stays_flat_and_270_folds_the_other_way() {
    let base = creased();
    let (g, s) = settle(&folded(&base, 180.));
    let (l, r) = halves(&g, &s);
    assert!(l.angle_between(r).to_degrees() < 2., "flat");
    let side = |angle: f64| {
        let (g, s) = settle(&folded(&base, angle));
        let (l, r) = halves(&g, &s);
        // Which way the right half turned, about the crease (along v, world −y).
        l.cross(r).dot(DVec3::NEG_Y).signum()
    };
    assert_eq!(side(90.), -side(270.), "mirror folds");
}

#[test]
fn folds_are_validated_and_flat_hinges_unchanged() {
    let base = creased();
    let plain = base.clone().compile().unwrap();
    assert!(plain.mesh.hinges.iter().all(|h| h.rest_angle == 0.));
    let g = folded(&base, 90.).compile().unwrap();
    let creased_hinges = g.mesh.hinges.iter().filter(|h| h.rest_angle != 0.).count();
    assert!(creased_hinges >= 8, "{creased_hinges}");
    let id = base.panels[0].id.clone();
    let mut wrong = base.clone();
    wrong.line_folds.push(LineFold::new(&id, "nowhere"));
    assert!(wrong.compile().is_err());
    let mut steep = LineFold::new(&id, "crease");
    steep.angle = 400.;
    assert!(
        authoring::apply(
            &base,
            Command::SetLineFold {
                panel: id,
                line: "crease".into(),
                fold: Some(steep)
            }
        )
        .is_err()
    );
}
