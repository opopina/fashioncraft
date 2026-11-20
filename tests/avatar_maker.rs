//! Bodies made from MakeHuman's macro targets.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_maker::{BODY_VERTICES, MacroBody, MacroTargets},
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
    assert_eq!(t.target_names().count(), 348);
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
        body.weights()
            .into_iter()
            .find(|(n, _)| n == name)
            .map_or(0., |(_, w)| w)
    };
    let default = MacroBody::default();
    assert!((weight(&default, "caucasian-female-young") - 1. / 6.).abs() < 1e-12);
    assert!(
        (weight(&default, "universal-male-young-averagemuscle-averageweight") - 0.5).abs() < 1e-12
    );
    assert!(
        default
            .weights()
            .iter()
            .all(|(n, _)| !n.starts_with("height/") && !n.starts_with("proportions/")),
        "average height and regular proportions move nothing"
    );
    let teen = MacroBody {
        age_years: 18.,
        gender: 1.,
        height: 1.,
        ..default
    };
    assert!(
        (weight(&teen, "universal-male-child-averagemuscle-averageweight") - 0.5).abs() < 1e-12
    );
    assert!(
        (weight(
            &teen,
            "height/male-young-averagemuscle-averageweight-maxheight"
        ) - 0.5)
            .abs()
            < 1e-12
    );
    // Every universal weight sums to one over the muscle and weight categories.
    let strong = MacroBody {
        muscle: 0.8,
        weight: 0.3,
        ..default
    };
    let universal: f64 = strong
        .weights()
        .iter()
        .filter(|(n, _)| n.starts_with("universal-"))
        .map(|(_, w)| w)
        .sum();
    assert!((universal - 1.).abs() < 1e-12);
    assert!(
        MacroBody {
            age_years: 120.,
            ..default
        }
        .validate()
        .is_err()
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
