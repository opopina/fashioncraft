//! Shrinkage per piece: below 100 % a piece draws in along that grain direction, as a fabric
//! that shrinks, keeping its mass. A weightless square released with its warp at 80 % settles
//! at 80 % of its cut size along the warp and keeps its size along the weft; turning the
//! grain a quarter turn swaps the directions. Lengths are measured along the piece's edges:
//! block descent does not conserve angular momentum exactly, so a free piece may turn.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::GarmentTemplate,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

fn far_away() -> SurfaceMesh {
    SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    }
}

/// An upright 12 cm square with `grain` degrees and shrinkage [warp, weft] %.
fn square(grain: f64, shrink: [f64; 2]) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.grain_degrees = grain;
    p.pattern = fashioncraft::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 6,
        ny: 6,
    };
    p.placement.translation = DVec3::new(-0.06, 1., 0.);
    if shrink == [100., 100.] {
        return t;
    }
    let id = t.panels[0].id.clone();
    authoring::apply(
        &t,
        Command::SetShrink {
            panels: vec![id],
            warp: shrink[0],
            weft: shrink[1],
        },
    )
    .unwrap()
    .template
}

/// Settle weightless for 1.5 s; the lengths of the bottom and left edges (corner to corner)
/// relative to the cut size. Measured along the piece, since it may turn in its plane.
fn settled_extent(t: &GarmentTemplate) -> [f64; 2] {
    let g = Arc::new(t.clone().compile().unwrap());
    let x0 = &g.initial_state.x;
    let corner = |key: &dyn Fn(DVec3) -> f64| {
        (0..x0.len())
            .min_by(|a, b| key(x0[*a]).total_cmp(&key(x0[*b])))
            .unwrap()
    };
    let bottom_left = corner(&|p| p.x + p.y);
    let bottom_right = corner(&|p| -p.x + p.y);
    let top_left = corner(&|p| p.x - p.y);
    let lengths = |x: &[DVec3]| {
        [
            x[bottom_left].distance(x[bottom_right]),
            x[bottom_left].distance(x[top_left]),
        ]
    };
    let before = lengths(x0);
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..360 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    let after = lengths(&s.x);
    [after[0] / before[0], after[1] / before[1]]
}

#[test]
fn a_piece_shrinks_along_its_warp() {
    let [x, y] = settled_extent(&square(0., [80., 100.]));
    assert!((x - 0.8).abs() < 0.02, "warp {x}");
    assert!((y - 1.).abs() < 0.02, "weft {y}");
}

#[test]
fn the_grain_decides_which_way_it_shrinks() {
    let [x, y] = settled_extent(&square(90., [80., 100.]));
    assert!((x - 1.).abs() < 0.02, "weft now along x: {x}");
    assert!((y - 0.8).abs() < 0.02, "warp now along y: {y}");
}

#[test]
fn shrinkage_keeps_the_mass_and_is_validated() {
    let plain = square(0., [100., 100.]).compile().unwrap();
    let shrunk = square(0., [80., 90.]).compile().unwrap();
    let mass = |g: &fashioncraft::garment::CompiledGarment| g.mesh.masses.iter().sum::<f64>();
    assert!((mass(&plain) - mass(&shrunk)).abs() < 1e-15);
    let area = |g: &fashioncraft::garment::CompiledGarment| {
        g.mesh.triangles.iter().map(|t| t.area).sum::<f64>()
    };
    assert!((area(&shrunk) / area(&plain) - 0.72).abs() < 1e-12);
    let t = square(0., [80., 100.]);
    let id = t.panels[0].id.clone();
    for (warp, weft) in [(40., 100.), (100., 151.), (f64::NAN, 100.)] {
        assert!(
            authoring::apply(
                &t,
                Command::SetShrink {
                    panels: vec![id.clone()],
                    warp,
                    weft
                }
            )
            .is_err()
        );
    }
    let cleared = authoring::apply(
        &t,
        Command::SetShrink {
            panels: vec![id],
            warp: 100.,
            weft: 100.,
        },
    )
    .unwrap()
    .template;
    assert!(cleared.panel_shrink.is_empty());
}
