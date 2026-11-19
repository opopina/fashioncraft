//! A skirt drafted from measurements: its outline follows the waist, hip, depth, length and
//! flare asked for, it is sewn down both sides and starts clear of the mannequin.
use fashioncraft::{
    avatar::{SurfaceMesh, diagnostics},
    garment::presets::{SkirtMeasurements, skirt},
};
use glam::DVec2;
use std::{path::Path, sync::Arc};

fn width_at(points: &[DVec2], y: f64) -> f64 {
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
    let m = SkirtMeasurements {
        waist_m: 0.70,
        hip_m: 1.00,
        ease_m: 0.04,
        hip_depth_m: 0.20,
        length_m: 0.60,
        flare_m: 0.40,
        ..Default::default()
    };
    let t = skirt(&m, 2).unwrap();
    assert_eq!(t.panels.len(), 2);
    let sewn: Vec<_> = t.seams.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(sewn, ["hip-right", "side-right", "side-left", "hip-left"]);
    let points = &t.panels[0].polygon.as_ref().unwrap().points;
    // Front and back together go round the waist and the hip with the ease, and the hem
    // round the hip and the flare.
    let waist = width_at(points, 1e-6);
    let hip = width_at(points, 0.20 + 1e-9);
    let hem = width_at(points, 0.60 - 1e-6);
    assert!((2. * waist - 0.74).abs() < 1e-5, "waist {waist}");
    assert!((2. * hip - 1.04).abs() < 1e-6, "hip {hip}");
    assert!((2. * hem - 1.44).abs() < 1e-5, "hem {hem}");
    let height = points.iter().map(|p| p.y).fold(0., f64::max);
    assert!((height - 0.60).abs() < 1e-12);
}

#[test]
fn the_default_skirt_starts_clear_of_the_mannequin() {
    let avatar = SurfaceMesh::load_obj(Path::new("assets/makehuman/mannequin.obj")).unwrap();
    let t = skirt(&SkirtMeasurements::default(), 2).unwrap();
    let garment = Arc::new(t.compile().unwrap());
    let state = garment.initial_state.clone();
    diagnostics::check_vbd_initialization(garment, &avatar, &state).unwrap();
}

#[test]
fn impossible_skirts_are_refused() {
    let base = SkirtMeasurements::default();
    for bad in [
        SkirtMeasurements {
            waist_m: 1.2,
            ..base
        },
        SkirtMeasurements {
            length_m: f64::NAN,
            ..base
        },
        SkirtMeasurements {
            length_m: 0.15,
            hip_depth_m: 0.2,
            ..base
        },
        SkirtMeasurements {
            length_m: 1.2,
            waist_height_m: 1.0,
            ..base
        },
    ] {
        assert!(skirt(&bad, 2).is_err(), "{bad:?}");
    }
}
