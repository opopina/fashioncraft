//! Subdividing a selected zone of the mesh: finer triangles there only, the rest of the piece
//! (and every other piece) meshed as before, and the garment still simulates.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        CompiledGarment, GarmentTemplate,
        meshing::RefineZone,
        presets::{self, GarmentKind},
    },
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;
use std::sync::Arc;

fn longest(t: [DVec2; 3]) -> f64 {
    (0..3)
        .map(|k| t[k].distance(t[(k + 1) % 3]))
        .fold(0., f64::max)
}

fn smallest_angle(t: [DVec2; 3]) -> f64 {
    (0..3)
        .map(|k| {
            let (a, b) = (t[(k + 1) % 3] - t[k], t[(k + 2) % 3] - t[k]);
            a.angle_to(b).abs().to_degrees()
        })
        .fold(180., f64::min)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn inside(p: DVec2, t: &[DVec2; 3]) -> bool {
    let d = (0..3).map(|k| (t[(k + 1) % 3] - t[k]).perp_dot(p - t[k]));
    let d: Vec<f64> = d.collect();
    d.iter().all(|x| *x >= -1e-12) || d.iter().all(|x| *x <= 1e-12)
}

/// Corners (material coordinates) of each triangle of `panel`, and its vertices as local
/// indices.
fn piece(g: &CompiledGarment, panel: usize) -> (Vec<[DVec2; 3]>, Vec<[usize; 3]>, Vec<DVec2>) {
    let range = &g.panels[panel];
    let triangles = &g.mesh.triangles[range.triangles.clone()];
    (
        triangles
            .iter()
            .map(|t| t.vertices.map(|v| g.mesh.uv[v]))
            .collect(),
        triangles
            .iter()
            .map(|t| t.vertices.map(|v| v - range.vertices.start))
            .collect(),
        g.mesh.uv[range.vertices.clone()].to_vec(),
    )
}

/// The points of the front within `radius` of its middle: a selection, as Select mesh makes.
fn selection(g: &CompiledGarment, radius: f64) -> Vec<usize> {
    let range = g.panels[0].vertices.clone();
    let middle = range.clone().map(|v| g.mesh.uv[v]).sum::<DVec2>() / range.len() as f64;
    range
        .filter(|v| g.mesh.uv[*v].distance(middle) <= radius)
        .collect()
}

#[test]
fn a_subdivided_zone_has_finer_edges_and_the_rest_is_unchanged() {
    let base = presets::basic(GarmentKind::Tshirt, 2);
    let before = base.clone().compile().unwrap();
    let selected = selection(&before, 0.09);
    assert!(selected.len() > 6, "{}", selected.len());
    let zones = authoring::subdivide_zones(&base, &before, &selected).unwrap();
    assert_eq!(zones.keys().collect::<Vec<_>>(), vec![&before.panels[0].id]);
    let zone = zones[&before.panels[0].id][0].clone();
    let edit = authoring::apply(&base, Command::SetMeshRefinement { zones }).unwrap();
    assert!(edit.reset_simulation);
    let after = edit.garment.clone();

    let (old_corners, old_faces, old_uv) = piece(&before, 0);
    let (new_corners, new_faces, new_uv) = piece(&after, 0);
    let in_zone = |t: &[DVec2; 3]| {
        let c = (t[0] + t[1] + t[2]) / 3.;
        zone.triangles.iter().any(|z| inside(c, z))
    };
    // Finer in the zone: every triangle there within the asked edge, and about half as long.
    let old_zone: Vec<_> = old_corners.iter().filter(|t| in_zone(t)).copied().collect();
    let new_zone: Vec<_> = new_corners.iter().filter(|t| in_zone(t)).copied().collect();
    assert!(
        new_zone.len() >= 3 * old_zone.len(),
        "{} {}",
        new_zone.len(),
        old_zone.len()
    );
    assert!(
        new_zone
            .iter()
            .all(|t| longest(*t) <= zone.edge_m * (1. + 1e-9))
    );
    let old_edge = median(old_zone.iter().map(|t| longest(*t)).collect());
    let new_edge = median(new_zone.iter().map(|t| longest(*t)).collect());
    assert!(new_edge < 0.6 * old_edge, "{new_edge} vs {old_edge}");
    // Its angles stay reasonable: at least half the smallest the piece had.
    let old_min = old_corners
        .iter()
        .map(|t| smallest_angle(*t))
        .fold(180., f64::min);
    let new_min = new_corners
        .iter()
        .map(|t| smallest_angle(*t))
        .fold(180., f64::min);
    assert!(new_min >= 0.5 * old_min - 1e-6, "{new_min} vs {old_min}");

    // Unchanged elsewhere: the old points are where they were, first and in order, and every
    // triangle more than a few edges from the zone is the same triangle.
    assert_eq!(&new_uv[..old_uv.len()], &old_uv[..]);
    let target = base.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .meshing_settings()
        .target_edge_m;
    let near = |t: &[usize; 3]| {
        t.iter().any(|v| {
            zone.triangles
                .iter()
                .flatten()
                .any(|z| z.distance(old_uv[*v]) < 3. * target)
        })
    };
    let far: Vec<_> = old_faces.iter().filter(|t| !near(t)).collect();
    assert!(
        far.len() > old_faces.len() / 3,
        "{} of {}",
        far.len(),
        old_faces.len()
    );
    let kept: std::collections::HashSet<_> = new_faces.iter().collect();
    assert!(far.iter().all(|t| kept.contains(t)));
    // The piece's outline is the same line, only with more points where the zone meets it.
    for (name, chain) in &before.panels[0].boundaries {
        let new_chain = &after.panels[0].boundaries[name];
        assert!(new_chain.len() >= chain.len());
        assert_eq!(after.mesh.uv[new_chain[0]], before.mesh.uv[chain[0]]);
        assert_eq!(
            after.mesh.uv[*new_chain.last().unwrap()],
            before.mesh.uv[*chain.last().unwrap()]
        );
    }
    // Every other piece is meshed exactly as before.
    for k in 1..before.panels.len() {
        let (a, b) = (piece(&before, k), piece(&after, k));
        assert_eq!(a.1, b.1);
        assert_eq!(a.2, b.2);
    }

    // Subdividing the same cloth again halves it again.
    let picked: Vec<usize> = after.panels[0]
        .vertices
        .clone()
        .filter(|v| zone.triangles.iter().any(|z| inside(after.mesh.uv[*v], z)))
        .collect();
    let again = authoring::subdivide_zones(&edit.template, &after, &picked).unwrap();
    let zones2 = &again[&after.panels[0].id];
    assert_eq!(zones2.len(), 2);
    assert!(zones2[1].edge_m < 0.7 * zone.edge_m);
    let twice = authoring::apply(
        &edit.template,
        Command::SetMeshRefinement {
            zones: again.clone(),
        },
    )
    .unwrap();
    assert!(twice.garment.mesh.triangles.len() > after.mesh.triangles.len());

    // Saved and opened again, the zone meshes the same.
    let text = serde_json::to_string(&edit.template).unwrap();
    let reopened: GarmentTemplate = serde_json::from_str(&text).unwrap();
    let rebuilt = reopened.compile().unwrap();
    assert_eq!(rebuilt.mesh.uv, after.mesh.uv);

    // Removing the subdivision meshes the piece evenly again, as it was.
    let undo = authoring::unsubdivide_zones(&edit.template, &after, &selected_in(&after));
    let even = authoring::apply(&edit.template, Command::SetMeshRefinement { zones: undo })
        .unwrap()
        .garment;
    assert_eq!(even.mesh.uv, before.mesh.uv);
}

fn selected_in(g: &CompiledGarment) -> Vec<usize> {
    g.panels[0].vertices.clone().take(1).collect()
}

#[test]
fn a_subdivided_garment_keeps_its_drape_and_simulates() {
    let base = presets::basic(GarmentKind::Tshirt, 2);
    let before = Arc::new(base.clone().compile().unwrap());
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 12,
        damping: 3.,
        ..Default::default()
    };
    // Sewn a little first, so it has a drape to keep.
    let mut s = before.initial_state.clone();
    let mut solver = VbdSolver::new(before.clone(), None, &s).unwrap();
    for _ in 0..20 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    let zones = authoring::subdivide_zones(&base, &before, &selection(&before, 0.09)).unwrap();
    let edit = authoring::apply(&base, Command::SetMeshRefinement { zones }).unwrap();
    let mut next = edit.state_keeping_drape(&before, &s);
    let after = Arc::new(edit.garment.clone());
    assert!(after.mesh.uv.len() > before.mesh.uv.len());
    // The old points start where the drape had them; the new ones on the cloth between them.
    let range = before.panels[0].vertices.clone();
    let moved = range
        .clone()
        .map(|v| next.x[v].distance(s.x[v]))
        .fold(0., f64::max);
    assert!(moved < 1e-6, "{moved}");
    let mut solver = VbdSolver::new(after.clone(), None, &next).unwrap();
    let gap0 = after.max_seam_gap(&next.x);
    for _ in 0..40 {
        solver.step(&mut next, &settings, None).unwrap();
    }
    assert!(next.x.iter().all(|p| p.is_finite()));
    assert!(next.x.iter().any(|p| p.distance(DVec3::ZERO) > 0.));
    let gap1 = after.max_seam_gap(&next.x);
    assert!(
        gap1 < gap0.max(0.01) + 1e-9,
        "the seams keep closing: {gap0} → {gap1}"
    );
}

#[test]
fn zones_are_validated() {
    let base = presets::basic(GarmentKind::Tshirt, 2);
    let id = base.panels[0].id.clone();
    let zone = |edge_m: f64, triangles: Vec<[DVec2; 3]>| {
        BTreeMap::from([(id.clone(), vec![RefineZone { triangles, edge_m }])])
    };
    let t = [DVec2::ZERO, DVec2::new(0.1, 0.), DVec2::new(0., 0.1)];
    for bad in [
        zone(0.0005, vec![t]),
        zone(f64::NAN, vec![t]),
        zone(0.01, vec![]),
        zone(0.01, vec![[DVec2::NAN, t[1], t[2]]]),
    ] {
        assert!(authoring::apply(&base, Command::SetMeshRefinement { zones: bad }).is_err());
    }
    assert!(
        authoring::apply(
            &base,
            Command::SetMeshRefinement {
                zones: BTreeMap::from([("nope".to_string(), vec![])]),
            }
        )
        .is_err()
    );
    // A zone finer than the vertex budget allows is refused, not half done.
    let count = base.compile().unwrap().panels[0].vertices.len();
    let mut tight = base.clone();
    let polygon = tight.panels[0].polygon.as_mut().unwrap();
    let mut settings = polygon.meshing_settings();
    settings.max_vertices = count + 20;
    polygon.meshing = Some(settings.clone());
    assert!(tight.compile().is_ok());
    let polygon = tight.panels[0].polygon.as_mut().unwrap();
    polygon.meshing = Some(settings);
    let big = [
        DVec2::new(-1., -1.),
        DVec2::new(3., -1.),
        DVec2::new(-1., 3.),
    ];
    assert!(
        authoring::apply(
            &tight,
            Command::SetMeshRefinement {
                zones: zone(0.003, vec![big])
            }
        )
        .is_err()
    );
}
