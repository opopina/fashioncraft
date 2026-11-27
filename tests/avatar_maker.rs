//! Bodies made from MakeHuman's targets, with Anny's settings and calibration.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_maker::{BODY_VERTICES, MEASURES, MacroBody, MacroTargets},
    avatar_measurements::{AvatarFrame, Landmark},
    avatar_shape,
    garment::arrangement::ArrangementSet,
};
use glam::DVec3;
use std::path::Path;

fn targets() -> &'static MacroTargets {
    MacroTargets::bundled().expect("the bundled targets read")
}

#[test]
fn the_bundled_targets_are_makehumans_own() {
    let t = targets();
    // 348 macro targets, 216 bust size and firmness targets, 40 measurement targets.
    assert_eq!(t.target_names().count(), 604);
    assert_eq!(
        t.target_names()
            .filter(|n| n.starts_with("measure/"))
            .count(),
        2 * MEASURES.len()
    );
    assert_eq!(t.joint_names().count(), 125);
    // The base body is the mannequin's base mesh, vertex for vertex.
    let base = SurfaceMesh::load_obj(Path::new("assets/makehuman/base.obj")).unwrap();
    assert!(
        t.base_body()
            .iter()
            .zip(&base.vertices)
            .all(|(a, b)| a.distance(*b) < 1e-5),
        "base body differs from base.obj"
    );
    assert_eq!(t.base_body().len(), BODY_VERTICES);
    // Offsets read back exactly as the CC0 file writes them ("0 -.097 6.886 .41").
    let o = t
        .offset("height/male-young-averagemuscle-averageweight-maxheight", 0)
        .unwrap();
    assert!(o.distance(DVec3::new(-0.097, 6.886, 0.41)) < 1e-12, "{o}");
    // Young average adults are the base mesh itself.
    assert!(
        t.offset("universal-female-young-averagemuscle-averageweight", 0)
            .is_none()
    );
}

#[test]
fn settings_weigh_the_targets_drawn_nearest_them() {
    let weight = |body: &MacroBody, name: &str| {
        targets()
            .weights(body)
            .into_iter()
            .find(|(n, _)| *n == name)
            .map_or(0., |(_, w)| w)
    };
    let default = MacroBody::default();
    let v = default.variation_weights();
    // Anny's gender runs from male to female; a body halfway is half of each.
    assert!((v["male"] - 0.5).abs() < 1e-12 && (v["female"] - 0.5).abs() < 1e-12);
    // 25 years, through Anny's calibrated mapping, is a young adult a third of the way to old.
    assert!((v["young"] + v["old"] - 1.).abs() < 1e-12 && (0.25..0.45).contains(&v["old"]));
    let expected = v["caucasian"] * v["female"] * v["young"];
    assert!((weight(&default, "caucasian-female-young") - expected).abs() < 1e-12);
    // A target weighs the product of the variations it is named after.
    let expected =
        v["female"] * v["young"] * v["averagemuscle"] * v["averageweight"] * v["maxheight"];
    assert!(
        (weight(
            &default,
            "height/female-young-averagemuscle-averageweight-maxheight"
        ) - expected)
            .abs()
            < 1e-12
    );
    // The universal weights sum to one over age, gender, muscle and weight.
    let strong = MacroBody {
        muscle: 0.8,
        weight: 0.3,
        ..default
    };
    let universal: f64 = targets()
        .weights(&strong)
        .iter()
        .filter(|(n, _)| n.starts_with("universal-"))
        .map(|(_, w)| w)
        .sum();
    assert!((universal - 1.).abs() < 1e-9, "{universal}");
    // A measurement counts on its own: its larger target one way, its smaller one the other.
    let mut wide = default;
    let bust = MEASURES
        .iter()
        .position(|(m, _)| *m == "bust-circ")
        .unwrap();
    wide.measures[bust] = 0.6;
    assert!((weight(&wide, "measure/measure-bust-circ-incr") - 0.6).abs() < 1e-12);
    assert_eq!(weight(&wide, "measure/measure-bust-circ-decr"), 0.);
    assert!(
        MacroBody {
            age_years: 120.,
            ..default
        }
        .validate()
        .is_err()
    );
}

#[test]
fn typical_and_random_bodies_follow_annys_calibration() {
    // Typical adults and a ten-year-old near real heights (WHO: men 1.76 m, women 1.63 m at 19).
    let man = height(&MacroBody::typical(1., 25.));
    let woman = height(&MacroBody::typical(0., 25.));
    let boy = height(&MacroBody::typical(1., 10.));
    assert!((1.70..1.85).contains(&man), "man {man}");
    assert!((1.53..1.68).contains(&woman), "woman {woman}");
    assert!((1.20..1.45).contains(&boy), "boy {boy}");
    // Drawn bodies: the same seed draws the same body, others differ, all within range.
    let base = MacroBody::typical(1., 30.);
    let a = base.realistic(7).unwrap();
    assert_eq!(a, base.realistic(7).unwrap());
    let heights: Vec<f64> = (0..12).map(|s| base.realistic(s).unwrap().height).collect();
    assert!(heights.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-6));
    assert!(a.validate().is_ok() && a.gender == 1. && a.age_years == 30.);
}

#[test]
fn measurements_and_bust_change_the_body_where_they_say() {
    let measure = |body: &MacroBody| {
        let made = targets().make(body).unwrap();
        avatar_shape::measure(&made.surface, &made.joints).unwrap()
    };
    let woman = MacroBody::typical(0., 30.);
    let base = measure(&woman);
    let mut hips = woman;
    hips.measures[MEASURES
        .iter()
        .position(|(m, _)| *m == "hips-circ")
        .unwrap()] = 1.;
    let wider = measure(&hips);
    assert!(
        wider.hip_m > base.hip_m + 0.02,
        "{} → {}",
        base.hip_m,
        wider.hip_m
    );
    assert!((wider.height_m - base.height_m).abs() < 0.01);
    let small = measure(&MacroBody {
        cupsize: 0.,
        ..woman
    });
    let large = measure(&MacroBody {
        cupsize: 1.,
        ..woman
    });
    assert!(
        large.bust_m > small.bust_m + 0.02,
        "{} {}",
        small.bust_m,
        large.bust_m
    );
}

fn height(body: &MacroBody) -> f64 {
    let made = targets().make(body).unwrap();
    let (lo, hi) = made
        .surface
        .vertices
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
    assert!(lo.abs() < 1e-12, "stands on the floor");
    hi - lo
}

#[test]
fn bodies_follow_their_settings() {
    let default = MacroBody::default();
    let average = height(&default);
    assert!((1.55..1.85).contains(&average), "average adult {average} m");
    let short = height(&MacroBody {
        height: 0.,
        ..default
    });
    let tall = height(&MacroBody {
        height: 1.,
        ..default
    });
    assert!(
        short + 0.1 < average && average + 0.1 < tall,
        "{short} {average} {tall}"
    );
    let baby = height(&MacroBody {
        age_years: 1.,
        ..default
    });
    assert!(baby < 1.0, "a one-year-old is {baby} m tall");

    // A man has broader shoulders than a woman, and less hip for his waist.
    let shape = |gender: f64| {
        let made = targets().make(&MacroBody { gender, ..default }).unwrap();
        let m = avatar_shape::measure(&made.surface, &made.joints).unwrap();
        let shoulders = made.joints["joint-l-shoulder"].distance(made.joints["joint-r-shoulder"]);
        (shoulders, m.hip_m / m.waist_m)
    };
    let (woman, man) = (shape(0.), shape(1.));
    assert!(man.0 > woman.0 + 0.02, "shoulders {woman:?} {man:?}");
    assert!(man.1 < woman.1, "hip to waist {woman:?} {man:?}");
}

#[test]
fn a_made_body_has_the_joints_the_anchors_are_built_on() {
    let made = targets()
        .make(&MacroBody {
            gender: 0.8,
            muscle: 0.7,
            ..MacroBody::default()
        })
        .unwrap();
    assert_eq!(made.joints.len(), 125);
    let j = |name: &str| made.joints[name];
    assert!(j("joint-l-shoulder").x > 0. && j("joint-r-shoulder").x < 0.);
    assert!(j("joint-neck").y > j("joint-l-shoulder").y);
    assert!(j("joint-l-shoulder").y > j("joint-l-elbow").y);
    let landmarks: Vec<Landmark> = made
        .joints
        .iter()
        .map(|(id, p)| Landmark {
            id: id.clone(),
            position_m: *p,
        })
        .collect();
    let set =
        ArrangementSet::from_json(include_str!("../assets/makehuman/arrangement.json")).unwrap();
    let frame = AvatarFrame {
        avatar_id: "makehuman-mannequin".into(),
        revision: 1,
    };
    set.validate(&frame, &landmarks)
        .expect("every anchor finds its joints");
    for anchor in &set.anchors {
        set.evaluate(&frame, &landmarks, &anchor.id).unwrap();
    }
}

#[test]
fn a_height_in_centimetres_sets_the_height_slider() {
    let t = targets();
    let woman = MacroBody::typical(0., 24.);
    let h = t.height_for(&woman, 175.).unwrap();
    let mut made = woman;
    made.height = h;
    let cm = t.height_cm(&made).unwrap();
    assert!((cm - 175.).abs() < 0.2, "{cm} cm");
    // Out of reach: it says the range.
    let error = t.height_for(&woman, 260.).unwrap_err();
    assert!(error.contains("out of reach"), "{error}");
}
