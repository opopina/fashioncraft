//! Trousers drafted from measurements: four pieces whose waist and hip add up to the girths
//! asked for, sewn down the sides, the inside legs, the centre front and the centre back, and
//! starting clear of the mannequin.
use fashioncraft::{
    avatar::{SurfaceMesh, diagnostics},
    garment::presets::{TrousersMeasurements, trousers},
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
fn the_pieces_add_up_to_the_measurements() {
    let m = TrousersMeasurements {
        waist_m: 0.72,
        hip_m: 1.00,
        ease_m: 0.04,
        ..Default::default()
    };
    let t = trousers(&m, 2).unwrap();
    let ids: Vec<_> = t.panels.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        ["front-right", "front-left", "back-right", "back-left"]
    );
    assert_eq!(t.seams.len(), 10);
    let mut waist = 0.;
    let mut hip = 0.;
    for p in &t.panels {
        let points = &p.polygon.as_ref().unwrap().points;
        waist += width_at(points, 1e-9);
        hip += width_at(points, m.hip_depth_m + 1e-9);
        // The left leg is the right one mirrored.
        assert!(
            points
                .iter()
                .all(|q| q.x >= -1e-12 && q.x <= p.pattern.width)
        );
    }
    assert!((waist - 0.76).abs() < 1e-6, "waist {waist}");
    assert!((hip - 1.04).abs() < 1e-6, "hip {hip}");
    // Mirrored pieces keep their edge names on the matching edges.
    let edge_length = |panel: usize, name: &str| {
        let p = t.panels[panel].polygon.as_ref().unwrap();
        let k = p.edges.iter().position(|e| e == name).unwrap();
        p.points[k].distance(p.points[(k + 1) % p.points.len()])
    };
    for name in [
        "waist",
        "centre",
        "crotch",
        "inseam",
        "hem",
        "outseam-lower",
        "outseam-upper",
    ] {
        assert!(
            (edge_length(0, name) - edge_length(1, name)).abs() < 1e-9,
            "{name}"
        );
        assert!(
            (edge_length(2, name) - edge_length(3, name)).abs() < 1e-9,
            "{name}"
        );
    }
}

#[test]
fn the_default_trousers_start_clear_of_the_mannequin() {
    let avatar = SurfaceMesh::load_obj(Path::new("assets/makehuman/mannequin.obj")).unwrap();
    let t = trousers(&TrousersMeasurements::default(), 2).unwrap();
    let garment = Arc::new(t.compile().unwrap());
    let state = garment.initial_state.clone();
    diagnostics::check_vbd_initialization(garment, &avatar, &state).unwrap();
}

#[test]
fn impossible_trousers_are_refused() {
    let base = TrousersMeasurements::default();
    for bad in [
        TrousersMeasurements {
            waist_m: 1.2,
            ..base
        },
        TrousersMeasurements {
            rise_m: 0.18,
            hip_depth_m: 0.18,
            ..base
        },
        TrousersMeasurements {
            length_m: 1.1,
            waist_height_m: 1.0,
            ..base
        },
        TrousersMeasurements {
            hem_m: f64::NAN,
            ..base
        },
    ] {
        assert!(trousers(&bad, 2).is_err(), "{bad:?}");
    }
}
