//! A T-shirt drafted from measurements: its outline follows the girths, length, shoulders and
//! sleeves asked for, it sews like the bundled one and starts clear of the mannequin.
use fashioncraft::{
    avatar::{SurfaceMesh, diagnostics},
    garment::presets::{TshirtMeasurements, tshirt},
};
use glam::DVec2;
use std::{path::Path, sync::Arc};

fn width_at(points: &[DVec2], y: f64) -> f64 {
    // Crossings of the outline with the horizontal line at `y`.
    let mut xs: Vec<f64> = Vec::new();
    for k in 0..points.len() {
        let (a, b) = (points[k], points[(k + 1) % points.len()]);
        if (a.y - y) * (b.y - y) < 0. {
            xs.push(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y));
        }
    }
    xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - xs.iter().copied().fold(f64::INFINITY, f64::min)
}

#[test]
fn the_outline_follows_the_measurements() {
    let m = TshirtMeasurements {
        bust_m: 1.00,
        hip_m: 1.10,
        ease_m: 0.08,
        length_m: 0.72,
        ..Default::default()
    };
    let t = tshirt(&m, 2).unwrap();
    assert_eq!(t.panels.len(), 2);
    assert_eq!(t.seams.len(), 8);
    let points = &t.panels[0].polygon.as_ref().unwrap().points;
    let (lo, hi) = points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    assert!((hi.y - lo.y - 0.72).abs() < 1e-9, "length");
    // Front and back together go round the bust and the hip with the ease.
    let hem = width_at(points, hi.y - 1e-6);
    assert!((2. * hem - (1.10 + 0.08)).abs() < 1e-6, "hem {hem}");
    // Across the chest, underarm to underarm.
    let chest = points[7].x - points[10].x;
    assert!((2. * chest - (1.00 + 0.08)).abs() < 1e-9, "chest {chest}");
    // Wider shoulders, wider shirt at the top.
    let wide = tshirt(
        &TshirtMeasurements {
            shoulder_m: 0.48,
            ..m
        },
        2,
    )
    .unwrap();
    let shoulder = |t: &fashioncraft::garment::GarmentTemplate| {
        let p = &t.panels[0].polygon.as_ref().unwrap().points;
        p[4].x - p[13].x
    };
    assert!((shoulder(&wide) - 0.48).abs() < 1e-9 && shoulder(&t) < shoulder(&wide));
}

#[test]
fn the_default_shirt_starts_clear_of_the_mannequin() {
    let avatar = SurfaceMesh::load_obj(Path::new("assets/makehuman/mannequin.obj")).unwrap();
    let t = tshirt(&TshirtMeasurements::default(), 2).unwrap();
    let garment = Arc::new(t.compile().unwrap());
    let state = garment.initial_state.clone();
    diagnostics::check_vbd_initialization(garment, &avatar, &state).unwrap();
}

#[test]
fn impossible_shirts_are_refused() {
    let base = TshirtMeasurements::default();
    for bad in [
        TshirtMeasurements {
            bust_m: 0.1,
            ..base
        },
        TshirtMeasurements {
            sleeve_m: f64::NAN,
            ..base
        },
        TshirtMeasurements {
            shoulder_m: 0.7,
            bust_m: 0.6,
            ..base
        },
        TshirtMeasurements {
            sleeve_m: 0.08,
            cuff_m: 0.2,
            ..base
        },
    ] {
        assert!(tshirt(&bad, 2).is_err(), "{bad:?}");
    }
}
