//! Steam: stamps that shrink or stretch the cloth under them, added and removed as edits.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{GarmentTemplate, meshing::MeshingSettings, presets, steam::SteamStamp},
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

/// A 40 × 30 cm rectangle.
fn rectangle() -> (GarmentTemplate, String) {
    let points = [[0., 0.], [0.4, 0.], [0.4, 0.3], [0., 0.3]];
    let t = authoring::apply(
        &presets::basic(presets::GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.iter().map(|p| DVec2::from_array(*p)).collect(),
            tangents: vec![DVec2::ZERO; 4],
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let id = t.panels[0].id.clone();
    (t, id)
}

/// Full-strength steam at `percent` over the right half (x from 0.2 to 0.4).
fn right_half(id: &str, percent: f64) -> Vec<SteamStamp> {
    (0..=6)
        .flat_map(|i| (0..=5).map(move |j| (i, j)))
        .map(|(i, j)| SteamStamp {
            panel: id.into(),
            center: DVec2::new(0.22 + 0.03 * i as f64, 0.03 + 0.05 * j as f64),
            radius_m: 0.04,
            hardness: 1.,
            percent,
        })
        .filter(|s| s.center.x <= 0.4)
        .collect()
}

#[test]
fn steamed_cloth_takes_its_new_size_when_it_settles() {
    let (base, id) = rectangle();
    let t = authoring::apply(
        &base,
        Command::AddSteam {
            stamps: right_half(&id, 80.),
        },
    )
    .unwrap()
    .template;
    let plain = base.clone().compile().unwrap();
    let g = Arc::new(t.compile().unwrap());
    // Rest areas: right-half triangles at 0.8² of their size, left-half ones untouched.
    for (a, b) in plain.mesh.triangles.iter().zip(&g.mesh.triangles) {
        let x = a.vertices.iter().map(|&v| plain.mesh.uv[v].x).sum::<f64>() / 3.;
        let ratio = b.area / a.area;
        if x > 0.23 {
            assert!((ratio - 0.64).abs() < 1e-9, "{x}: {ratio}");
        } else if x < 0.17 {
            assert!((ratio - 1.).abs() < 1e-12, "{x}: {ratio}");
        }
    }
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    for (p, uv) in s.x.iter_mut().zip(&g.mesh.uv) {
        p.z += 0.001 * (uv.y * 40.).sin();
    }
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 16,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..160 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    let column = |x: f64| {
        let ids: Vec<usize> = (0..s.x.len())
            .filter(|&i| (g.mesh.uv[i].x - x).abs() < 1e-9)
            .collect();
        ids.iter().map(|&i| s.x[i]).sum::<DVec3>() / ids.len() as f64
    };
    let left = column(0.).distance(column(0.2));
    let right = column(0.2).distance(column(0.4));
    assert!((left - 0.2).abs() < 0.01, "left {left}");
    assert!(right < 0.18, "right {right}");
}

#[test]
fn steam_comes_off_under_the_eraser_per_piece_or_all_at_once() {
    let (base, id) = rectangle();
    let t = authoring::apply(
        &base,
        Command::AddSteam {
            stamps: right_half(&id, 120.),
        },
    )
    .unwrap()
    .template;
    let total = t.steam.len();
    let erased = authoring::apply(
        &t,
        Command::RemoveSteam {
            panels: vec![],
            eraser: Some((id.clone(), vec![DVec2::new(0.22, 0.03)], 0.01)),
        },
    )
    .unwrap()
    .template;
    assert_eq!(erased.steam.len(), total - 1);
    let none = authoring::apply(
        &t,
        Command::RemoveSteam {
            panels: vec![id.clone()],
            eraser: None,
        },
    )
    .unwrap()
    .template;
    assert!(none.steam.is_empty());
    assert!(
        authoring::apply(
            &none,
            Command::RemoveSteam {
                panels: vec![],
                eraser: None
            }
        )
        .is_err()
    );
    // Bad stamps are refused.
    let mut wild = right_half(&id, 120.);
    wild[0].percent = 400.;
    assert!(authoring::apply(&base, Command::AddSteam { stamps: wild }).is_err());
}

#[test]
fn grain_guides_still_export_through_steamed_cloth() {
    let (base, id) = rectangle();
    let t = authoring::apply(
        &base,
        Command::AddSteam {
            stamps: right_half(&id, 80.),
        },
    )
    .unwrap()
    .template;
    let g = t.clone().compile().unwrap();
    let text = fashioncraft::garment::export::grain_guides_obj(&t, &g, &g.initial_state).unwrap();
    assert!(text.starts_with("# Material grain guides"));
}
