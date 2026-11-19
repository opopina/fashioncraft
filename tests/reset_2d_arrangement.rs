//! Reset 2D arrangement: every piece laid flat in 3D as on the board, on an upright plane in
//! front of the avatar, which the solver accepts as a start.
use fashioncraft::{
    authoring::arrange2d::board_point,
    avatar::SurfaceMesh,
    garment::{
        presets::{self, GarmentKind},
        transfer::lay_out_as_board,
    },
    vbd::VbdSolver,
};
use std::{path::PathBuf, sync::Arc};

#[test]
fn pieces_lie_flat_in_front_of_the_avatar_as_on_the_board() {
    let template = presets::basic(GarmentKind::Tshirt, 0);
    let garment = Arc::new(template.clone().compile().unwrap());
    let body = SurfaceMesh::load_obj(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .unwrap();
    let laid = lay_out_as_board(&template, &garment, &body.vertices);
    assert_eq!(laid.len(), garment.mesh.uv.len());
    let front = body
        .vertices
        .iter()
        .map(|p| p.z)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut x = garment.initial_state.x.clone();
    for (v, p) in &laid {
        assert!((p.z - front - 0.25).abs() < 1e-12);
        x[*v] = *p;
    }
    // Same shapes and the same offsets as on the board (its y runs down, 3D y up).
    let index = |id: &str| template.panels.iter().position(|p| p.id == id).unwrap();
    let (a, b) = (&garment.panels[0], &garment.panels[1]);
    let (va, vb) = (a.vertices.start, b.vertices.start + 3);
    let board = |range: &fashioncraft::garment::template::PanelRange, v: usize| {
        board_point(&template, index(&range.id), garment.mesh.uv[v])
    };
    let d_board = board(b, vb) - board(a, va);
    let d_3d = x[vb] - x[va];
    assert!((d_3d.x - d_board.x).abs() < 1e-12 && (d_3d.y + d_board.y).abs() < 1e-12);
    // The solver accepts it as a start: nothing crosses the body or itself.
    let mut state = garment.initial_state.clone();
    state.x = x;
    VbdSolver::new(garment.clone(), Some(&body), &state).unwrap();
    // Centred on the avatar across.
    let mean_x = laid.iter().map(|(_, p)| p.x).sum::<f64>() / laid.len() as f64;
    let body_mid = {
        let (lo, hi) = body
            .vertices
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.x), hi.max(p.x))
            });
        (lo + hi) * 0.5
    };
    assert!((mean_x - body_mid).abs() < 0.2, "{mean_x} vs {body_mid}");
}
