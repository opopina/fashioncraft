//! Automatic sewing finds the T-shirt's eight seams again once they are removed, and leaves
//! its openings (hem, neckline, cuffs) open because the body passes through them.
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    fitting::FittingProject,
    garment::{auto_sew::auto_sew, presets::GarmentKind},
};
use std::{collections::BTreeSet, path::PathBuf};

#[test]
fn the_t_shirt_is_sewn_again_and_its_openings_stay_open() {
    let mannequin =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin);
    let scene = project.compile().unwrap();
    let body = &scene.collider.surface;
    let sewn = &project.template;
    // Already sewn: nothing to add.
    assert!(auto_sew(sewn, &scene.garment, &scene.state.x, body).is_empty());
    let expected: BTreeSet<(String, String, String, String, bool)> = sewn
        .seams
        .iter()
        .map(|s| {
            (
                s.a.panel.clone(),
                s.a.edge.clone().unwrap(),
                s.b.panel.clone(),
                s.b.edge.clone().unwrap(),
                s.reverse_b,
            )
        })
        .collect();
    let mut bare = sewn.clone();
    bare.seams.clear();
    let found = auto_sew(&bare, &scene.garment, &scene.state.x, body);
    let got: BTreeSet<_> = found
        .iter()
        .map(|s| {
            (
                s.a.panel.clone(),
                s.a.edge.clone().unwrap(),
                s.b.panel.clone(),
                s.b.edge.clone().unwrap(),
                s.reverse,
            )
        })
        .collect();
    assert_eq!(got, expected);
    assert!(
        found
            .iter()
            .all(|s| (s.lengths_m[0] - s.lengths_m[1]).abs() < 1e-9)
    );
    // The seams it finds go on as ordinary seams, in one edit (one undo step).
    let t = authoring::apply(
        &bare,
        Command::SewMany {
            seams: found
                .iter()
                .map(|s| (s.a.clone(), s.b.clone(), s.reverse, false))
                .collect(),
        },
    )
    .unwrap()
    .template;
    assert_eq!(t.seams.len(), sewn.seams.len());
    assert!(authoring::apply(&bare, Command::SewMany { seams: vec![] }).is_err());
    // Pieces laid away from the body do not pair: their openings cannot be told.
    let away: Vec<_> = scene
        .state
        .x
        .iter()
        .map(|p| *p + glam::DVec3::new(3., 0., 0.))
        .collect();
    assert!(auto_sew(&bare, &scene.garment, &away, body).is_empty());
    // Without a body to tell openings from seams, the hem pairs as well.
    let none = SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    };
    let blind = auto_sew(&bare, &scene.garment, &scene.state.x, &none);
    assert!(blind.len() > found.len());
    assert!(blind.iter().any(|s| s.a.edge.as_deref() == Some("hem")));
}
