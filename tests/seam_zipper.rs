//! Zippers on seams: only the share the slider has closed is sewn; open, the sides are apart.
use marvelous_cloth::{
    authoring::{self, Command},
    fitting::FittingProject,
    garment::{GarmentTemplate, template::SeamZipper, trims::TrimKind},
};

fn tshirt() -> GarmentTemplate {
    FittingProject::basic(
        marvelous_cloth::garment::presets::GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .template
}

fn zipped(t: &GarmentTemplate, closed: f64) -> GarmentTemplate {
    authoring::apply(
        t,
        Command::SetSeamZipper {
            seam: t.seams[0].id.clone(),
            zipper: Some(SeamZipper {
                closed,
                width_m: 0.01,
            }),
        },
    )
    .unwrap()
    .template
}

/// The sewn length of seam `id` in the compiled garment: its samples' weights add up to the
/// mean of the two sides' lengths.
fn sewn(t: &GarmentTemplate, id: &str) -> Option<f64> {
    let garment = t.compile().unwrap();
    let seam = garment.seams.iter().find(|s| s.id == id)?;
    Some(seam.samples.iter().map(|s| s.weight_m).sum())
}

#[test]
fn only_the_closed_share_is_sewn() {
    let t = tshirt();
    let id = t.seams[0].id.clone();
    let whole = sewn(&t, &id).unwrap();
    assert!(whole > 0.05);
    let half = sewn(&zipped(&t, 0.5), &id).unwrap();
    assert!((half - 0.5 * whole).abs() < 1e-9, "{half} vs {whole}");
    let closed = sewn(&zipped(&t, 1.), &id).unwrap();
    assert!((closed - whole).abs() < 1e-12);
    // Open all along, the seam is not sewn at all; the others still are.
    let open = zipped(&t, 0.);
    assert!(sewn(&open, &id).is_none());
    assert_eq!(
        open.compile().unwrap().seams.len(),
        t.compile().unwrap().seams.len() - 1
    );
}

#[test]
fn bad_zippers_are_refused() {
    let t = tshirt();
    for zipper in [
        SeamZipper {
            closed: 1.5,
            width_m: 0.01,
        },
        SeamZipper {
            closed: 0.5,
            width_m: 0.2,
        },
        SeamZipper {
            closed: f64::NAN,
            width_m: 0.01,
        },
    ] {
        let edit = authoring::apply(
            &t,
            Command::SetSeamZipper {
                seam: t.seams[0].id.clone(),
                zipper: Some(zipper.clone()),
            },
        );
        assert!(edit.is_err(), "{zipper:?}");
    }
    assert!(
        authoring::apply(
            &t,
            Command::SetSeamZipper {
                seam: "missing".into(),
                zipper: Some(SeamZipper::default()),
            },
        )
        .is_err()
    );
}

#[test]
fn the_width_is_only_a_look_and_deleting_the_seam_drops_the_zipper() {
    let t = zipped(&tshirt(), 0.5);
    let mut wider = t.clone();
    wider.seam_zippers.values_mut().next().unwrap().width_m = 0.02;
    assert!(authoring::same_physics(&t, &wider));
    assert!(!authoring::same_physics(&t, &zipped(&tshirt(), 0.4)));
    // Closed all along it sews like a plain seam.
    assert!(authoring::same_physics(&tshirt(), &zipped(&tshirt(), 1.)));
    let without = authoring::apply(
        &t,
        Command::RemoveSeam {
            seam: t.seams[0].id.clone(),
        },
    )
    .unwrap();
    assert!(without.template.seam_zippers.is_empty());
}

#[test]
fn it_draws_a_tape_on_each_side_and_the_slider_where_it_stops() {
    let t = zipped(&tshirt(), 0.25);
    let trims = marvelous_cloth::garment::trims::zipper_trims(&t);
    let tapes = trims
        .iter()
        .filter(|t| matches!(t.kind, TrimKind::Zipper { .. }))
        .count();
    assert_eq!(tapes, 2);
    let slider = trims
        .iter()
        .find_map(|t| match &t.kind {
            TrimKind::Button { center_m, .. } => Some(*center_m),
            _ => None,
        })
        .expect("a slider");
    // The slider is a quarter of the way along the first side's tape.
    let tape = trims
        .iter()
        .find_map(|t| match &t.kind {
            TrimKind::Zipper { path_m, .. } => Some(path_m.clone()),
            _ => None,
        })
        .unwrap();
    let length = |p: &[glam::DVec2]| -> f64 { p.windows(2).map(|w| w[0].distance(w[1])).sum() };
    let total = length(&tape);
    let mut walked = 0.;
    let mut along = None;
    for w in tape.windows(2) {
        let d = w[0].distance(w[1]);
        let t = (slider - w[0]).dot(w[1] - w[0]) / (d * d);
        if (0. ..=1.).contains(&t) && (w[0].lerp(w[1], t)).distance(slider) < 1e-9 {
            along = Some(walked + t * d);
            break;
        }
        walked += d;
    }
    let along = along.expect("the slider lies on the tape");
    assert!((along - 0.25 * total).abs() < 1e-9, "{along} of {total}");
}

#[test]
fn the_zipper_shows_on_the_cloth() {
    let t = zipped(&tshirt(), 0.4);
    let garment = t.compile().unwrap();
    let trims = marvelous_cloth::garment::trims::zipper_trims(&t);
    let triangles =
        marvelous_cloth::garment::trims::trim_triangles(&trims, &garment, &garment.initial_state.x);
    assert!(triangles.len() > 50, "{} triangles", triangles.len());
}

#[test]
fn every_seam_of_the_tee_can_show_a_zipper() {
    let base = tshirt();
    for seam in &base.seams {
        let t = authoring::apply(
            &base,
            Command::SetSeamZipper {
                seam: seam.id.clone(),
                zipper: Some(SeamZipper {
                    closed: 0.4,
                    width_m: 0.02,
                }),
            },
        )
        .unwrap()
        .template;
        let garment = t.compile().unwrap();
        let trims = marvelous_cloth::garment::trims::zipper_trims(&t);
        let triangles = marvelous_cloth::garment::trims::trim_triangles(
            &trims,
            &garment,
            &garment.initial_state.x,
        );
        assert!(
            trims.len() == 3 && triangles.len() > 50,
            "{}: {} trims, {} triangles",
            seam.id,
            trims.len(),
            triangles.len()
        );
    }
}
