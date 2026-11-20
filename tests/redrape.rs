//! Re-drape: cloth draped on one body carried onto another, keeping its height off the skin.
use fashioncraft::{
    avatar::{MeshCollider, SurfaceMesh},
    avatar_landmarks, avatar_shape, redrape,
};
use glam::DVec3;
use std::{path::PathBuf, sync::Arc};

fn mannequin() -> SurfaceMesh {
    SurfaceMesh::load_obj(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .unwrap()
}

/// Points a centimetre off the skin, all over the body.
fn cloth(body: &SurfaceMesh) -> Vec<DVec3> {
    let collider = MeshCollider::new(Arc::new(body.clone()), 1.).unwrap();
    body.vertices
        .iter()
        .step_by(37)
        .map(|v| collider.lift_outside(*v, 0.01))
        .collect()
}

/// Share of the points whose height off the skin changed by less than `tolerance` from `x` on
/// `from` to `carried` on `to`.
fn share_kept_off_the_skin(
    from: &SurfaceMesh,
    x: &[DVec3],
    to: &SurfaceMesh,
    carried: &[DVec3],
    tolerance: f64,
) -> f64 {
    let (a, b) = (
        MeshCollider::new(Arc::new(from.clone()), 1.).unwrap(),
        MeshCollider::new(Arc::new(to.clone()), 1.).unwrap(),
    );
    let kept = x
        .iter()
        .zip(carried)
        .filter(|(p, q)| (a.signed_distance(**p) - b.signed_distance(**q)).abs() < tolerance)
        .count();
    kept as f64 / x.len() as f64
}

#[test]
fn a_reshaped_body_takes_the_cloth_at_the_same_spots() {
    let body = mannequin();
    let landmarks = avatar_landmarks::estimate(&body).unwrap();
    let mut target = avatar_shape::measure(&body, &landmarks).unwrap();
    target.bust_m *= 1.12;
    target.waist_m *= 1.15;
    target.hip_m *= 1.1;
    let (bigger, _) = avatar_shape::reshape(&body, &landmarks, &target).unwrap();
    assert_eq!(bigger.triangles, body.triangles);
    let x = cloth(&body);
    let carried = redrape::carry(&body, None, &bigger, None, &x).unwrap();
    assert_eq!(carried.len(), x.len());
    let share = share_kept_off_the_skin(&body, &x, &bigger, &carried, 0.001);
    assert!(share > 0.99, "{share}");
}

#[test]
fn another_mesh_takes_the_cloth_by_its_bones() {
    let body = mannequin();
    // A taller copy with its triangles listed differently: not the same mesh any more.
    let mut other = body.clone();
    for v in &mut other.vertices {
        *v *= 1.08;
    }
    other.triangles.rotate_left(1);
    let x = cloth(&body);
    let carried = redrape::carry(&body, None, &other, None, &x).unwrap();
    // Off the new skin as far as off the old, but for a few points in folds (fingers, toes,
    // the face) where another part of the body is nearer.
    let share = share_kept_off_the_skin(&body, &x, &other, &carried, 0.005);
    assert!(share > 0.8, "{share}");
    // And where the taller body puts it.
    let mut errors: Vec<f64> = x
        .iter()
        .zip(&carried)
        .map(|(p, q)| (*p * 1.08).distance(*q))
        .collect();
    errors.sort_by(f64::total_cmp);
    let median = errors[errors.len() / 2];
    let ninetieth = errors[errors.len() * 9 / 10];
    assert!(median < 0.005 && ninetieth < 0.03, "{median} {ninetieth}");
}

#[test]
fn auto_fit_grows_the_pieces_as_the_cloth_was_stretched() {
    use fashioncraft::{
        authoring::{self, Command},
        garment::presets::{self, GarmentKind},
    };
    let template = presets::basic(GarmentKind::Tshirt, 0);
    let garment = template.clone().compile().unwrap();
    let x = &garment.initial_state.x;
    // A body a tenth larger every way stretches the carried cloth a tenth more.
    let carried: Vec<DVec3> = x.iter().map(|p| *p * 1.1).collect();
    let scales = redrape::fit_scales(&garment, x, &carried);
    assert_eq!(scales.len(), template.panels.len());
    for (_, s) in &scales {
        assert!(
            (*s - glam::DVec2::splat(1.1)).abs().max_element() < 1e-9,
            "{s}"
        );
    }
    // Unchanged cloth asks for nothing.
    assert!(redrape::fit_scales(&garment, x, x).is_empty());
    // One edit resizes every piece; the garment still builds.
    let width = |p: &fashioncraft::garment::template::Panel| {
        let xs = p.polygon.as_ref().unwrap().points.iter().map(|q| q.x);
        xs.clone().fold(f64::NEG_INFINITY, f64::max) - xs.fold(f64::INFINITY, f64::min)
    };
    let widths: Vec<f64> = template.panels.iter().map(width).collect();
    let edit = authoring::apply(&template, Command::FitPieces { scales }).unwrap();
    for (p, w) in edit.template.panels.iter().zip(widths) {
        assert!((width(p) / w - 1.1).abs() < 1e-6, "{}", width(p) / w);
    }
    edit.template.compile().unwrap();
}
