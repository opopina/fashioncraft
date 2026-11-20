//! Superimpose (side): a piece laid flat beside its partner, seam edge to seam edge.
use fashioncraft::garment::transfer::lay_beside;
use glam::{DVec2, DVec3};

#[test]
fn a_piece_lies_flat_in_its_partners_plane_on_the_far_side_of_the_seam() {
    // A 10 × 20 cm piece whose left edge (x = 0) is sewn to a partner standing in the
    // plane z = 0 to the left of the seam line x = 0.5 (y from 1.0 to 1.2).
    let uv = [
        DVec2::new(0., 0.),
        DVec2::new(0.1, 0.),
        DVec2::new(0.1, 0.2),
        DVec2::new(0., 0.2),
    ];
    let seam = [DVec2::new(0., 0.), DVec2::new(0., 0.2)];
    let (a, b) = (DVec3::new(0.5, 1.0, 0.), DVec3::new(0.5, 1.2, 0.));
    let partner = DVec3::new(0.3, 1.1, 0.);
    let placed = lay_beside(&uv, seam, [a, b], DVec3::Z, partner, 0.005).unwrap();
    for p in &placed {
        assert!(p.z.abs() < 1e-12, "in the partner's plane");
        assert!(
            p.x >= 0.5 + 0.005 - 1e-12,
            "beside it, away from the partner: {p}"
        );
    }
    // The seam edge meets the partner's seam, a gap away, and the shape is kept.
    assert!(placed[0].distance(DVec3::new(0.505, 1.0, 0.)) < 1e-12);
    assert!(placed[3].distance(DVec3::new(0.505, 1.2, 0.)) < 1e-12);
    assert!((placed[1].distance(placed[0]) - 0.1).abs() < 1e-12);
    assert!((placed[2].distance(placed[0]) - (0.1f64.hypot(0.2))).abs() < 1e-12);
    // A partner on the other side sends the piece the other way.
    let flipped = lay_beside(&uv, seam, [a, b], DVec3::Z, DVec3::new(0.7, 1.1, 0.), 0.005).unwrap();
    assert!(flipped.iter().all(|p| p.x <= 0.5 - 0.005 + 1e-12));
    assert!(lay_beside(&uv, [seam[0], seam[0]], [a, b], DVec3::Z, partner, 0.).is_err());
}

#[test]
fn a_t_shirt_piece_laid_beside_its_partner_is_a_valid_start() {
    use fashioncraft::{
        avatar::SurfaceMesh,
        fitting::FittingProject,
        garment::{presets::GarmentKind, transfer::superimpose_side},
        vbd::VbdSolver,
    };
    use std::path::PathBuf;
    let avatar = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar.clone());
    let scene = project.compile().unwrap();
    let body = SurfaceMesh::load_obj(&avatar).unwrap();
    let panel = project.template.panels[1].id.clone();
    let placed = superimpose_side(
        &project.template,
        &scene.garment,
        &scene.state.x,
        &panel,
        0.008,
    )
    .unwrap();
    let range = scene.garment.panels.iter().find(|r| r.id == panel).unwrap();
    assert_eq!(placed.len(), range.vertices.len());
    let mut state = scene.state.clone();
    for (v, p) in &placed {
        state.x[*v] = *p;
    }
    // Flat: every point within a hair of one plane.
    let points: Vec<_> = placed.iter().map(|(_, p)| *p).collect();
    let c = points.iter().sum::<glam::DVec3>() / points.len() as f64;
    let n = (points[1] - points[0])
        .cross(points[points.len() / 2] - points[0])
        .normalize();
    assert!(points.iter().all(|p| (*p - c).dot(n).abs() < 1e-6));
    // The solver accepts it as a start: nothing crosses the avatar or the other piece.
    VbdSolver::new(scene.garment.clone(), Some(&body), &state).unwrap();
}
