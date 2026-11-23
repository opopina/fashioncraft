//! Unit checks of the interpolation that carries a panel's inside along with its outline.
use super::apply::Field;
use glam::DVec2;

fn field(base: &[DVec2], f: impl Fn(DVec2) -> DVec2) -> Field {
    Field::from_samples(base.to_vec(), base.iter().map(|p| f(*p)).collect())
}

#[test]
fn an_affine_movement_of_the_outline_moves_the_inside_the_same_way() {
    // An L-shaped (concave) outline.
    let outline = [
        DVec2::new(0., 0.),
        DVec2::new(0.4, 0.),
        DVec2::new(0.4, 0.1),
        DVec2::new(0.1, 0.1),
        DVec2::new(0.1, 0.5),
        DVec2::new(0., 0.5),
    ];
    let affine = |p: DVec2| DVec2::new(1.05 * p.x + 0.02 * p.y + 0.003, 0.98 * p.y - 0.01 * p.x);
    let f = field(&outline, affine);
    for x in [
        DVec2::new(0.05, 0.05),
        DVec2::new(0.3, 0.05),
        DVec2::new(0.05, 0.45),
        DVec2::new(0.2, 0.1),
        DVec2::new(0., 0.25),
    ] {
        assert!((f.map(x) - affine(x)).length() < 1e-12, "{x:?}");
    }
}

#[test]
fn outline_samples_keep_their_own_movement() {
    let outline = [DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
    let moved = [
        DVec2::ZERO,
        DVec2::new(1.1, 0.),
        DVec2::new(1.1, 1.2),
        DVec2::Y,
    ];
    let f = Field::from_samples(outline.to_vec(), moved.to_vec());
    for (a, b) in outline.iter().zip(&moved) {
        assert!((f.map(*a) - *b).length() < 1e-12);
    }
    // Halfway along the right side: halfway between its ends' movements.
    assert!((f.map(DVec2::new(1., 0.5)) - DVec2::new(1.1, 0.6)).length() < 1e-12);
    // An empty field moves nothing.
    assert_eq!(Field::default().map(DVec2::ONE), DVec2::ONE);
}

#[test]
fn notch_modes_place_the_notch() {
    use super::{NotchMode, apply::notch_at};
    assert_eq!(
        notch_at(0.25, 1., 2., NotchMode::Proportional, 0.),
        Some(0.25)
    );
    assert_eq!(
        notch_at(0.25, 1., 2., NotchMode::FromStart, 0.),
        Some(0.125)
    );
    assert_eq!(notch_at(0.75, 1., 2., NotchMode::FromEnd, 0.), Some(0.875));
    assert_eq!(notch_at(0.75, 1., 2., NotchMode::FromEnd, 0.25), Some(0.75));
    assert_eq!(notch_at(0.5, 1., 0.4, NotchMode::FromStart, 0.), None);
}
