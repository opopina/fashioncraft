//! Avatar looks: the body's texture coordinates follow the mannequin's triangles, and the
//! bundled parts fit any made body where they belong.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_look::{self, Fitting, Look},
    avatar_maker::{MacroBody, MacroTargets},
};

const PARTS: &[&str] = &[
    "eyes/low-poly",
    "eyebrows/eyebrow001",
    "eyebrows/eyebrow010",
    "eyelashes/eyelashes01",
    "eyelashes/eyelashes02",
    "hair/short02",
    "hair/short04",
    "hair/ponytail01",
    "hair/bob01",
    "hair/bob02",
    "hair/long01",
    "hair/braid01",
    "hair/afro01",
];

#[test]
fn the_body_has_a_texture_coordinate_for_every_mannequin_corner() {
    let uvs = avatar_look::body_uvs().unwrap();
    let made = MacroTargets::bundled()
        .unwrap()
        .make(&MacroBody::default())
        .unwrap();
    assert_eq!(uvs.len(), made.surface.triangles.len());
    assert!(
        uvs.iter()
            .flatten()
            .flatten()
            .all(|c| (0.0..=1.0).contains(c))
    );
}

#[test]
fn every_bundled_part_fits_bodies_young_and_old() {
    let targets = MacroTargets::bundled().unwrap();
    for body in [
        MacroBody::default(),
        MacroBody {
            gender: 0.,
            age_years: 8.,
            ..MacroBody::default()
        },
        MacroBody {
            gender: 1.,
            weight: 0.9,
            height: 0.9,
            ..MacroBody::default()
        },
    ] {
        let made = targets.make(&body).unwrap();
        let top = made.surface.vertices.iter().map(|v| v.y).fold(0., f64::max);
        let eyes = (made.joints["joint-l-eye"] + made.joints["joint-r-eye"]) / 2.;
        for part in PARTS {
            let fitting = Fitting::bundled(part).unwrap();
            let vertices = made.fit(&fitting).unwrap();
            assert!(!vertices.is_empty() && !fitting.triangles.is_empty());
            let centre = vertices.iter().sum::<glam::DVec3>() / vertices.len() as f64;
            let highest = vertices
                .iter()
                .map(|v| v.y)
                .fold(f64::NEG_INFINITY, f64::max);
            // Every part hangs from the head: its top between the eyes and just over the
            // crown, centred, and nothing below the waist.
            assert!(
                highest > eyes.y - 0.02
                    && highest < top + 0.08
                    && centre.x.abs() < 0.03
                    && centre.y > 0.6 * top,
                "{part} on {body:?} is at {centre}, top {highest} (eyes {eyes}, crown {top})"
            );
        }
    }
}

#[test]
fn the_eyes_sit_on_the_eye_joints() {
    let made = MacroTargets::bundled()
        .unwrap()
        .make(&MacroBody::default())
        .unwrap();
    let eyes = made
        .fit(&Fitting::bundled("eyes/low-poly").unwrap())
        .unwrap();
    for joint in ["joint-l-eye", "joint-r-eye"] {
        let at = made.joints[joint];
        let nearest = eyes
            .iter()
            .map(|v| v.distance(at))
            .fold(f64::INFINITY, f64::min);
        assert!(nearest < 0.02, "no eye within 2 cm of {joint}: {nearest}");
    }
}

#[test]
fn a_look_survives_its_file_and_the_avatar_its_texture_coordinates() {
    let made = MacroTargets::bundled()
        .unwrap()
        .make(&MacroBody::default())
        .unwrap();
    let fitting = Fitting::bundled("hair/short02").unwrap();
    let mut look = Look::new(Some("builtin:skins/young_african_male.jpg".into()));
    look.parts.push(avatar_look::fitted_part(
        "hair/short02",
        "builtin:parts/hair/short02/image.png",
        true,
        &made.fit(&fitting).unwrap(),
        &fitting,
    ));
    look.validate().unwrap();
    let text = serde_json::to_string(&look).unwrap();
    assert_eq!(serde_json::from_str::<Look>(&text).unwrap(), look);
    for reference in [look.skin.as_deref().unwrap(), &look.parts[0].image] {
        assert!(
            !avatar_look::image_bytes(reference, std::path::Path::new("."))
                .unwrap()
                .is_empty()
        );
    }

    let uvs = avatar_look::body_uvs().unwrap();
    let obj = avatar_look::surface_obj_with_uvs(&made.surface, uvs, "test");
    let back = SurfaceMesh::parse_obj(&obj).unwrap();
    assert_eq!(back.triangles, made.surface.triangles);
    let (_, triangles, read) = avatar_look::obj_with_uvs(&obj).unwrap();
    assert_eq!(triangles, made.surface.triangles);
    for (a, b) in read.iter().flatten().zip(uvs.iter().flatten()) {
        assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5);
    }
}

#[test]
fn parts_bind_to_the_nearest_body_vertex() {
    let body = [glam::DVec3::ZERO, glam::DVec3::X, glam::DVec3::Y];
    let points = [
        glam::DVec3::new(0.9, 0.05, 0.),
        glam::DVec3::new(0.02, 0.6, 0.),
    ];
    assert_eq!(
        avatar_look::nearest_body_vertices(&body, &points),
        vec![1, 2]
    );
}

#[test]
fn a_rigid_motion_is_found_back_from_its_points() {
    use glam::{DQuat, DVec3};
    let q = DQuat::from_euler(glam::EulerRot::YXZ, 0.7, -0.3, 0.2);
    let t = DVec3::new(0.1, -0.4, 2.);
    let from: Vec<DVec3> = (0..40)
        .map(|k| {
            let k = k as f64;
            DVec3::new((k * 1.3).sin(), (k * 0.7).cos(), (k * 0.31).sin() * 0.5)
        })
        .collect();
    let to: Vec<DVec3> = from.iter().map(|p| q * *p + t).collect();
    let (found, shift) = avatar_look::rigid_motion(&from, &to).unwrap();
    for (a, b) in from.iter().zip(&to) {
        assert!((found * *a + shift).distance(*b) < 1e-9);
    }
    let (same, none) = avatar_look::rigid_motion(&from, &from).unwrap();
    assert!(same.angle_between(DQuat::IDENTITY) < 1e-9 && none.length() < 1e-9);
}

#[test]
fn every_preset_makes_its_body_and_look() {
    let targets = MacroTargets::bundled().unwrap();
    for preset in avatar_look::PRESETS {
        let made = targets.make(&(preset.body)()).unwrap();
        let look = avatar_look::make_look(&made, &preset.look).unwrap();
        look.validate().unwrap();
        assert_eq!(look.parts.len(), 4, "{}", preset.name);
        for reference in look.skin.iter().chain(look.parts.iter().map(|p| &p.image)) {
            assert!(avatar_look::image_bytes(reference, std::path::Path::new(".")).is_ok());
        }
    }
}

#[test]
fn underwear_covers_the_torso_and_never_the_arms() {
    let targets = MacroTargets::bundled().unwrap();
    for (gender, age, top) in [
        (0., 28., true),
        (1., 30., false),
        (0., 8., true),
        (1., 8., false),
    ] {
        let made = targets.make(&MacroBody::typical(gender, age)).unwrap();
        let v = &made.surface.vertices;
        let coverage = avatar_look::underwear_coverage(&made.surface, top);
        assert_eq!(coverage.len(), v.len());
        let height = v.iter().map(|p| p.y).fold(0., f64::max);
        let covered = |i: usize| coverage[i] > 0.;
        // The crotch and the hips are covered; hands and elbows are not.
        let crotch = (0..v.len())
            .filter(|i| v[*i].x.abs() < 0.004 && v[*i].y > 0.25 * height)
            .min_by(|a, b| v[*a].y.total_cmp(&v[*b].y))
            .unwrap();
        assert!(covered(crotch));
        for joint in [
            "joint-l-hand",
            "joint-r-hand",
            "joint-l-elbow",
            "joint-r-elbow",
        ] {
            let at = made.joints[joint];
            let near: Vec<usize> = (0..v.len()).filter(|i| v[*i].distance(at) < 0.08).collect();
            assert!(
                !near.is_empty() && near.iter().all(|i| !covered(*i)),
                "{joint} covered at {gender} {age}"
            );
        }
        // A top only when asked: something covered well above the crotch.
        let chest = (0..v.len()).any(|i| covered(i) && v[i].y > v[crotch].y + 0.3 * height / 1.75);
        assert_eq!(chest, top, "top at gender {gender}, age {age}");
    }
}

#[test]
fn thumbnails_show_the_body_on_a_clear_background() {
    let made = MacroTargets::bundled()
        .unwrap()
        .make(&MacroBody::default())
        .unwrap();
    let look = avatar_look::make_look(&made, &avatar_look::PRESETS[0].look).unwrap();
    let images = std::collections::BTreeMap::new();
    let rgba = avatar_look::thumbnail(&made.surface, None, &look, &images, [60, 90]);
    assert_eq!(rgba.len(), 4 * 60 * 90);
    let alpha = |x: usize, y: usize| rgba[4 * (y * 60 + x) + 3];
    assert_eq!(alpha(0, 0), 0, "corners are clear");
    assert_eq!(alpha(30, 45), 255, "the middle is the body");
}

#[test]
fn only_adults_may_go_without_underwear() {
    let targets = MacroTargets::bundled().unwrap();
    let choice = avatar_look::LookChoice {
        without_underwear: true,
        ..avatar_look::PRESETS[0].look
    };
    for (age, bare) in [(30., true), (18., true), (17., false), (8., false)] {
        let made = targets.make(&MacroBody::typical(0., age)).unwrap();
        let look = avatar_look::make_look(&made, &choice).unwrap();
        assert_eq!(look.underwear.is_none(), bare, "age {age}");
    }
    let made = targets.make(&MacroBody::typical(1., 30.)).unwrap();
    let dressed = avatar_look::make_look(&made, &avatar_look::PRESETS[5].look).unwrap();
    assert!(dressed.underwear.is_some(), "on unless asked");
}

#[test]
fn the_library_avatars_have_human_heights() {
    let targets = MacroTargets::bundled().unwrap();
    for preset in avatar_look::PRESETS {
        let body = (preset.body)();
        let made = targets.make(&body).unwrap();
        let height = fashioncraft::avatar_shape::measure(&made.surface, &made.joints)
            .unwrap()
            .height_m;
        let range = if body.age_years < 18. {
            1.0..1.6
        } else {
            1.48..1.95
        };
        assert!(
            range.contains(&height),
            "{} is {:.2} m tall",
            preset.name,
            height
        );
    }
}
