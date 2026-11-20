//! Torn seams: stretches of a seam left unsewn, as a rip; set, merged and mended as edits.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        CompiledGarment,
        presets::{self, GarmentKind},
    },
};

fn stitched_length(g: &CompiledGarment, id: &str) -> f64 {
    let seam = g.seams.iter().find(|s| s.id == id).unwrap();
    seam.samples.iter().map(|s| s.weight_m).sum()
}

#[test]
fn a_torn_stretch_of_a_seam_has_no_stitches() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let id = base.seams[0].id.clone();
    let whole = stitched_length(&base.clone().compile().unwrap(), &id);
    let torn = authoring::apply(
        &base,
        Command::SetSeamTears {
            seam: id.clone(),
            tears: vec![[0.3, 0.7]],
        },
    )
    .unwrap()
    .template;
    let g = torn.clone().compile().unwrap();
    let left = stitched_length(&g, &id);
    assert!((left / whole - 0.6).abs() < 1e-6, "{left} of {whole}");
    // Other seams are untouched.
    let other = &base.seams[1].id;
    let plain = base.clone().compile().unwrap();
    assert_eq!(stitched_length(&g, other), stitched_length(&plain, other));
}

#[test]
fn tears_merge_and_mend() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let id = base.seams[0].id.clone();
    let t = authoring::apply(
        &base,
        Command::SetSeamTears {
            seam: id.clone(),
            tears: vec![[0.5, 0.8], [0.2, 0.55], [0.9, 1.0]],
        },
    )
    .unwrap()
    .template;
    assert_eq!(t.seam_tears[&id], vec![[0.2, 0.8], [0.9, 1.0]]);
    t.clone().compile().unwrap();
    let mended = authoring::apply(
        &t,
        Command::SetSeamTears {
            seam: id.clone(),
            tears: vec![],
        },
    )
    .unwrap()
    .template;
    assert!(mended.seam_tears.is_empty());
    assert!(
        authoring::apply(
            &mended,
            Command::SetSeamTears {
                seam: id.clone(),
                tears: vec![]
            }
        )
        .is_err()
    );
    assert!(
        authoring::apply(
            &base,
            Command::SetSeamTears {
                seam: id.clone(),
                tears: vec![[0.6, 0.4]]
            }
        )
        .is_err()
    );
    // Deleting the seam takes its tears with it.
    let gone = authoring::apply(&t, Command::RemoveSeam { seam: id.clone() })
        .unwrap()
        .template;
    assert!(gone.seam_tears.is_empty());
}

#[test]
fn a_torn_seam_stays_open_where_it_is_torn_and_closes_elsewhere() {
    use fashioncraft::{
        avatar::SurfaceMesh,
        garment::GarmentTemplate,
        vbd::{VbdSettings, VbdSolver},
    };
    use glam::DVec3;
    use std::sync::Arc;
    let base = GarmentTemplate::two_panel_demo();
    let id = base.seams[0].id.clone();
    let plain = base.clone().compile().unwrap();
    let guides = plain.seams[0].guides.clone();
    let gap = |t: &GarmentTemplate| -> [f64; 2] {
        let g = Arc::new(t.clone().compile().unwrap());
        let far = SurfaceMesh {
            vertices: vec![
                DVec3::new(50., 0.5, 50.),
                DVec3::new(50.1, 0.5, 50.),
                DVec3::new(50., 0.6, 50.),
            ],
            triangles: vec![[0, 1, 2]],
        };
        let mut s = g.initial_state.clone();
        // Start the second piece 5 cm away, so the seam has to pull it in.
        for v in g.panels[1].vertices.clone() {
            s.x[v].z += 0.05;
        }
        let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
        let settings = VbdSettings {
            dt: 1. / 240.,
            iterations: 16,
            damping: 3.,
            gravity: DVec3::ZERO,
            ..Default::default()
        };
        for _ in 0..240 {
            solver.step(&mut s, &settings, None).unwrap();
        }
        let at = |side: [(usize, f64); 2]| side.iter().map(|(v, w)| s.x[*v] * *w).sum::<DVec3>();
        // Near the seam's start and at its middle.
        [1, 4].map(|i| at(guides[i][0]).distance(at(guides[i][1])))
    };
    let [start0, middle0] = gap(&base);
    let torn = authoring::apply(
        &base,
        Command::SetSeamTears {
            seam: id,
            tears: vec![[0.3, 0.7]],
        },
    )
    .unwrap()
    .template;
    let [start1, middle1] = gap(&torn);
    // Sewn whole, both close; torn in the middle, the start still closes and the middle not.
    assert!(start0 < 0.01 && middle0 < 0.01, "{start0} {middle0}");
    assert!(start1 < 0.02, "{start1}");
    assert!(middle1 > 0.02, "{middle1} vs {middle0}");
}
