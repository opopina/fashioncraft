//! Garments and poses as files of their own: a saved garment opens in the template browser
//! and adds beside another garment with all its seams; a pose file carries the rotations
//! only and refuses anything else.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{
        self, Command,
        clipboard::{self, PatternClipboard},
    },
    avatar_rig::{self, POSE_FILE_FORMAT, Pose},
    garment::{
        GarmentTemplate,
        presets::{self, GarmentKind},
    },
    template_catalog::Selection,
};

fn ids(t: &GarmentTemplate) -> Vec<String> {
    t.panels.iter().map(|p| p.id.clone()).collect()
}

#[test]
fn a_saved_garment_reopens_and_adds_beside_another() {
    let shirt = presets::basic(GarmentKind::Tshirt, 0);
    // Saved as the template JSON the template browser imports.
    let text = serde_json::to_string_pretty(&shirt).unwrap();
    assert!(Selection::external(text.as_bytes(), "garment.json".into()).is_ok());
    let back: GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&shirt).unwrap()
    );
    // Added to a dress: every piece and seam comes along, laid out to the right of it.
    let dress = presets::basic(GarmentKind::Dress, 0);
    let (lo, hi) = clipboard::layout_bounds(&back, &ids(&back)).unwrap();
    let (dlo, dhi) = clipboard::layout_bounds(&dress, &ids(&dress)).unwrap();
    let corner = DVec2::new(dhi.x + 0.1, dlo.y);
    let clip = PatternClipboard::capture(&back, &ids(&back)).unwrap();
    let both = authoring::apply(
        &dress,
        Command::Paste {
            clipboard: clip,
            center: corner + (hi - lo) * 0.5,
        },
    )
    .unwrap()
    .template;
    assert_eq!(both.panels.len(), dress.panels.len() + shirt.panels.len());
    assert_eq!(both.seams.len(), dress.seams.len() + shirt.seams.len());
    let added: Vec<String> = ids(&both)[dress.panels.len()..].to_vec();
    let (alo, ahi) = clipboard::layout_bounds(&both, &added).unwrap();
    assert!((alo - corner).length() < 1e-9, "{alo} vs {corner}");
    assert!(((ahi - alo) - (hi - lo)).length() < 1e-9);
    assert!(both.compile().is_ok());
    // No pieces, no bounds.
    assert!(clipboard::layout_bounds(&dress, &[]).is_none());
}

#[test]
fn a_pose_file_round_trips_and_refuses_strangers() {
    let pose = avatar_rig::preset("Arms forward").unwrap();
    assert!(!pose.0.is_empty());
    let text = pose.to_file().unwrap();
    assert!(text.contains(POSE_FILE_FORMAT));
    assert_eq!(Pose::from_file(&text).unwrap(), pose);
    // Other JSON, another format, another version: refused.
    assert!(Pose::from_file("{}").is_err());
    assert!(Pose::from_file("not json").is_err());
    assert!(Pose::from_file(&text.replace(POSE_FILE_FORMAT, "something-else")).is_err());
    assert!(Pose::from_file(&text.replace("\"version\": 1", "\"version\": 2")).is_err());
    // Angles out of range or names no bone could have: refused.
    let bone = avatar_rig::bone_names().next().unwrap().to_string();
    let mut wild = pose.clone();
    wild.0.insert(bone, [720., 0., 0.]);
    assert!(Pose::from_file(&wild.to_file().unwrap()).is_err());
    let mut strange = Pose::default();
    strange.0.insert("no such bone!".into(), [1., 0., 0.]);
    assert!(Pose::from_file(&strange.to_file().unwrap()).is_err());
    // A bone added on another avatar travels by name; the avatar it goes on checks it has it.
    let mut added = Pose::default();
    added.0.insert("bone_1".into(), [10., 0., 0.]);
    assert_eq!(Pose::from_file(&added.to_file().unwrap()).unwrap(), added);
}

#[test]
fn a_saved_avatar_reads_back_as_the_same_mesh() {
    let mannequin =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let body = marvelous_cloth::avatar::SurfaceMesh::load_obj(&mannequin).unwrap();
    let text = body.to_obj("posed avatar\nmetres");
    assert!(text.starts_with("# posed avatar\n# metres\n"));
    let back = marvelous_cloth::avatar::SurfaceMesh::parse_obj(&text).unwrap();
    assert_eq!(back.triangles, body.triangles);
    assert_eq!(back.vertices.len(), body.vertices.len());
    // Positions print exactly: Rust writes the shortest text that reads back the same.
    assert!(
        back.vertices
            .iter()
            .zip(&body.vertices)
            .all(|(a, b)| a == b)
    );
}
