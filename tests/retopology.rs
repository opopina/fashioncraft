//! Retopology: each piece remeshed as quads of a chosen size along its grain, with triangles
//! only where the grid meets the outline, lying on the drape.
use fashioncraft::{
    fitting::FittingProject,
    garment::presets::GarmentKind,
    gltf_export::front_triangles,
    obj_export::{self, ObjOptions, ObjScene},
    retopology::retopologize,
};
use glam::{DVec2, DVec3};
use std::{collections::HashMap, path::PathBuf};

fn shirt() -> FittingProject {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    FittingProject::basic(GarmentKind::Tshirt, 1, path)
}

/// Twice the signed area of a polygon.
fn area(p: &[DVec2]) -> f64 {
    (0..p.len())
        .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
        .sum()
}

#[test]
fn pieces_become_grain_aligned_quads_that_keep_their_outline_and_lie_on_the_drape() {
    let project = shirt();
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let triangles = front_triangles(&project.template, garment);
    let all = vec![true; garment.panels.len()];
    let size = 0.02;
    let retopo = retopologize(garment, &triangles, size, &all).unwrap();
    let uv = retopo.sample(&garment.mesh.uv);
    for (k, panel) in garment.panels.iter().enumerate() {
        let piece = &retopo.pieces[k];
        let before: f64 = triangles[panel.triangles.clone()]
            .iter()
            .map(|t| area(&t.map(|v| garment.mesh.uv[v])))
            .sum();
        // Quads, and a few triangles near the outline.
        let quads = piece.faces.iter().filter(|f| f.len() == 4).count();
        assert!(piece.faces.iter().all(|f| (3..=4).contains(&f.len())));
        assert!(quads as f64 > 0.85 * piece.faces.len() as f64, "{quads}");
        // Every face turns like the piece's triangles, none is flat, and together they
        // cover the piece (the outline is followed by chords about `size` long).
        let mut after = 0.;
        for f in &piece.faces {
            let a = area(&f.iter().map(|v| uv[*v]).collect::<Vec<_>>());
            assert!(a * before.signum() > 1e-8, "{a}");
            after += a;
        }
        assert!(
            (after - before).abs() < 0.01 * before.abs(),
            "{before} {after}"
        );
        // Inside, the quads are squares of the chosen size along the grain (here 0°).
        let mut sides: Vec<f64> = piece
            .faces
            .iter()
            .filter(|f| f.len() == 4)
            .flat_map(|f| {
                (0..4)
                    .map(|i| uv[f[(i + 1) % 4]] - uv[f[i]])
                    .collect::<Vec<_>>()
            })
            .filter(|d| d.x.abs() < 1e-9 || d.y.abs() < 1e-9)
            .map(|d| d.length())
            .collect();
        sides.sort_by(f64::total_cmp);
        assert!(sides.len() > 1000);
        assert!((sides[sides.len() / 2] - size).abs() < 1e-9);
        // An edge used by one face is on the outline, and the outline is one closed loop
        // about as long as the piece's.
        let mut uses: HashMap<(usize, usize), usize> = HashMap::new();
        for f in &piece.faces {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(uses.values().all(|u| *u <= 2));
        let open: Vec<_> = uses
            .iter()
            .filter(|(_, u)| **u == 1)
            .map(|(e, _)| *e)
            .collect();
        let length: f64 = open.iter().map(|(a, b)| uv[*a].distance(uv[*b])).sum();
        let mut outline_uses: HashMap<(usize, usize), usize> = HashMap::new();
        for t in &triangles[panel.triangles.clone()] {
            for i in 0..3 {
                let (a, b) = (t[i], t[(i + 1) % 3]);
                *outline_uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let outline: f64 = outline_uses
            .iter()
            .filter(|(_, u)| **u == 1)
            .map(|((a, b), _)| garment.mesh.uv[*a].distance(garment.mesh.uv[*b]))
            .sum();
        assert!(
            (length - outline).abs() < 0.01 * outline,
            "{length} {outline}"
        );
    }
    // Each new vertex is a point of a garment triangle, so it lies on the cloth.
    for point in &retopo.points {
        let sum: f64 = point.iter().map(|(_, w)| w).sum();
        assert!((sum - 1.).abs() < 1e-9 && point.iter().all(|(_, w)| *w >= 0.));
    }
    let x = retopo.sample(&scene.state.x);
    assert_eq!(x.len(), retopo.points.len());
    assert!(x.iter().all(|p: &DVec3| p.is_finite()));

    // Sizes out of range are refused.
    for bad in [0.001, 0.6, f64::NAN] {
        assert!(retopologize(garment, &triangles, bad, &all).is_err());
    }
}

#[test]
fn an_obj_can_be_written_retopologized() {
    let project = shirt();
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let triangles = front_triangles(&project.template, garment);
    let retopo =
        retopologize(garment, &triangles, 0.03, &vec![true; garment.panels.len()]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retopo.obj");
    let export = ObjScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: None,
    };
    let options = ObjOptions {
        retopology: Some(0.03),
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    let faces: usize = retopo.pieces.iter().map(|p| p.faces.len()).sum();
    assert_eq!(summary.vertices, retopo.points.len());
    assert_eq!(summary.faces, faces);
    let text = std::fs::read_to_string(&path).unwrap();
    let corners: Vec<usize> = text
        .lines()
        .filter_map(|l| l.strip_prefix("f "))
        .map(|f| f.split_whitespace().count())
        .collect();
    assert!(corners.iter().filter(|c| **c == 4).count() > corners.len() / 2);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("vt ")).count(),
        retopo.points.len()
    );
}

#[test]
fn the_grid_follows_the_grain() {
    let mut project = shirt();
    project.template.panels[0].grain_degrees = 30.;
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let triangles = front_triangles(&project.template, garment);
    let retopo = retopologize(garment, &triangles, 0.025, &[true, false]).unwrap();
    assert!(retopo.pieces[1].faces.is_empty());
    let uv = retopo.sample(&garment.mesh.uv);
    let grain = DVec2::from_angle(30f64.to_radians());
    let mut along = 0;
    let quads: Vec<_> = retopo.pieces[0]
        .faces
        .iter()
        .filter(|f| f.len() == 4)
        .collect();
    for f in &quads {
        let d = (uv[f[1]] - uv[f[0]]).normalize();
        if d.dot(grain).abs() > 1. - 1e-9 || d.perp_dot(grain).abs() > 1. - 1e-9 {
            along += 1;
        }
    }
    assert!(
        along as f64 > 0.8 * quads.len() as f64,
        "{along} of {}",
        quads.len()
    );
}
