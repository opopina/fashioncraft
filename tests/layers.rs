//! Simulation layers: where pieces of different layers touch, only the outer one gives way.
//! Weightless sheets thrown at each other show it plainly: without layers they shove each
//! other, with layers the inner one ignores the outer, which yields, and they never cross.
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        GarmentTemplate,
        template::{Fabric, FabricProperties},
    },
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

fn floor() -> SurfaceMesh {
    SurfaceMesh {
        vertices: vec![
            DVec3::new(-40., 0., -20.),
            DVec3::new(40., 0., -20.),
            DVec3::new(0., 0., 40.),
        ],
        triangles: vec![[0, 1, 2]],
    }
}

/// Flat 12 cm squares, one per (height, areal density kg/m²), each with its own fabric.
fn squares(layers: &[(f64, f64)]) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.seams.clear();
    let base = t.panels[0].clone();
    t.panels.clear();
    for (k, (height, density)) in layers.iter().enumerate() {
        let mut p = base.clone();
        p.id = format!("square{k}");
        p.name = p.id.clone();
        p.pins.clear();
        p.pattern = marvelous_cloth::model::Pattern {
            width: 0.12,
            height: 0.12,
            nx: 3,
            ny: 3,
        };
        p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
        p.placement.translation = DVec3::new(-0.06, *height, 0.06);
        p.fabric = format!("fabric{k}");
        t.fabrics.push(Fabric {
            id: p.fabric.clone(),
            name: p.fabric.clone(),
            properties: FabricProperties {
                density: *density,
                ..Default::default()
            },
        });
        t.panels.push(p);
    }
    t
}

/// Two upright sheets 1 cm apart, weightless; `thrown` (0 or 1) flies at the other at
/// 0.5 m/s. Returns each sheet's mean z velocity after 0.2 s, and whether every vertex of the
/// first sheet is still behind its counterpart in the second.
fn collide(layer_of_second: u32, thrown: usize) -> ([f64; 2], bool) {
    let mut t = squares(&[(0.5, 0.2), (0.5, 0.2)]);
    for (k, p) in t.panels.iter_mut().enumerate() {
        p.placement.rotation_degrees = DVec3::ZERO;
        p.placement.translation = DVec3::new(-0.06, 0.5, 0.01 * k as f64);
    }
    if layer_of_second > 0 {
        t = authoring::apply(
            &t,
            Command::SetLayer {
                panels: vec!["square1".into()],
                layer: layer_of_second,
            },
        )
        .unwrap()
        .template;
    }
    let g = Arc::new(t.compile().unwrap());
    let mut s = g.initial_state.clone();
    let per = s.x.len() / 2;
    let towards = if thrown == 0 { 0.5 } else { -0.5 };
    for v in &mut s.v[thrown * per..(thrown + 1) * per] {
        v.z = towards;
    }
    let mut solver = VbdSolver::new(g, Some(&floor()), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        gravity: DVec3::ZERO,
        damping: 0.,
        ..Default::default()
    };
    for _ in 0..48 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    let mean = |k: usize| s.v[k * per..(k + 1) * per].iter().map(|v| v.z).sum::<f64>() / per as f64;
    let ordered = (0..per).all(|i| s.x[i].z < s.x[i + per].z);
    ([mean(0), mean(1)], ordered)
}

#[test]
fn only_the_outer_layer_gives_way_in_a_collision() {
    // Without layers the thrown outer sheet shoves the inner one along.
    let (plain, ordered) = collide(0, 1);
    assert!(ordered);
    assert!(plain[0] < -0.1, "the inner sheet is pushed: {plain:?}");
    // With the outer sheet on a higher layer the inner one does not move at all, and the
    // outer one is stopped on its side.
    let (layered, ordered) = collide(1, 1);
    assert!(ordered, "the layers never cross");
    assert_eq!(layered[0], 0., "the inner sheet keeps still");
    assert!(
        layered[1] > -0.05,
        "the outer sheet is stopped: {layered:?}"
    );
    // The inner sheet thrown at the outer one keeps its speed and pushes it away.
    let (pushed, ordered) = collide(1, 0);
    assert!(ordered, "the layers never cross");
    assert!(
        (pushed[0] - 0.5).abs() < 1e-9,
        "the inner sheet does not slow: {pushed:?}"
    );
    assert!(pushed[1] > 0.4, "the outer sheet gives way: {pushed:?}");
}

#[test]
fn layers_are_set_reset_and_validated_per_piece() {
    let t = squares(&[(0.003, 0.2), (0.01, 0.2)]);
    assert!(t.clone().compile().unwrap().layers.is_empty());
    let set = authoring::apply(
        &t,
        Command::SetLayer {
            panels: vec!["square1".into()],
            layer: 2,
        },
    )
    .unwrap()
    .template;
    let g = set.clone().compile().unwrap();
    let per = g.layers.len() / 2;
    assert!(g.layers[..per].iter().all(|l| *l == 0));
    assert!(g.layers[per..].iter().all(|l| *l == 2));
    let json = serde_json::to_string(&set).unwrap();
    let back: GarmentTemplate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.panel_layers, set.panel_layers);

    let reset = authoring::apply(
        &set,
        Command::SetLayer {
            panels: vec!["square1".into()],
            layer: 0,
        },
    )
    .unwrap()
    .template;
    assert!(reset.panel_layers.is_empty(), "0 clears the layer");
    assert!(reset.compile().unwrap().layers.is_empty());
    assert!(!serde_json::to_string(&t).unwrap().contains("panel_layers"));

    for (panels, layer) in [(vec!["nope".to_string()], 1), (vec!["square0".into()], 10)] {
        assert!(authoring::apply(&t, Command::SetLayer { panels, layer }).is_err());
    }
    assert!(
        authoring::apply(
            &set,
            Command::SetLayer {
                panels: vec!["square1".into()],
                layer: 2
            }
        )
        .is_err(),
        "no empty undo steps"
    );
}
