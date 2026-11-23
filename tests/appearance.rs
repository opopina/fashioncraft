use fashioncraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        appearance::{self, FabricAppearance},
    },
};

#[test]
fn optics_are_validated_and_do_not_change_mechanics_or_legacy_bytes() {
    let source = GarmentTemplate::two_panel_demo();
    let old = serde_json::to_value(&source).unwrap();
    assert!(old.get("fabric_appearance").is_none());
    let g = source.compile().unwrap();
    let optics = FabricAppearance {
        color_srgb: [255, 0, 128],
        roughness: 0.3,
        ..Default::default()
    };
    let edit = authoring::apply(
        &source,
        Command::FabricAppearance {
            fabric: source.fabrics[0].id.clone(),
            appearance: Some(optics.clone()),
        },
    )
    .unwrap();
    assert!(!edit.reset_simulation);
    assert!(authoring::only_appearance_changed(&source, &edit.template));
    assert_eq!(edit.garment.mesh.uv, g.mesh.uv);
    assert_eq!(edit.garment.mesh.masses, g.mesh.masses);
    assert_eq!(edit.garment.initial_state.x, g.initial_state.x);
    assert_eq!(
        appearance::bind(&edit.template, &g),
        vec![Some(optics); g.materials.len()]
    );
    for roughness in [f64::NAN, -0.1, 1.1] {
        assert!(
            authoring::apply(
                &source,
                Command::FabricAppearance {
                    fabric: source.fabrics[0].id.clone(),
                    appearance: Some(FabricAppearance {
                        roughness,
                        ..Default::default()
                    })
                }
            )
            .is_err()
        );
    }
    assert_eq!(serde_json::to_value(&source).unwrap(), old);
    let reset = authoring::apply(
        &edit.template,
        Command::FabricAppearance {
            fabric: source.fabrics[0].id.clone(),
            appearance: None,
        },
    )
    .unwrap();
    assert_eq!(serde_json::to_value(reset.template).unwrap(), old);
    let color = FabricAppearance {
        color_srgb: [0, 255, 128],
        ..Default::default()
    }
    .linear_color();
    assert_eq!(color[0], 0.);
    assert_eq!(color[1], 1.);
    assert!((color[2] - 0.21586).abs() < 0.00001);
}

#[test]
fn cached_drape_reopens_after_appearance_change_with_identical_signature() {
    use fashioncraft::{fitting::FittingProject, garment::presets::GarmentKind};
    let dir = tempfile::tempdir().unwrap();
    let avatar = dir.path().join("avatar.obj");
    std::fs::write(&avatar, "v 10 0 -10\nv 11 0 -10\nv 10 1 -10\nf 1 2 3\n").unwrap();
    let file = dir.path().join("garment.clothcraft");
    let mut p = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    p.template = GarmentTemplate::two_panel_demo();
    let mut state = p.template.compile().unwrap().initial_state;
    state.time = 3.;
    state.v.fill(glam::DVec3::X);
    p.state = Some(state.clone());
    p.save(&file).unwrap();
    let mut before = FittingProject::load(&file).unwrap();
    let signature = before.mesh_signature.clone();
    before
        .template
        .fabric_appearance
        .insert(before.template.fabrics[0].id.clone(), Default::default());
    let scene = before.compile().unwrap();
    assert_eq!(scene.state.x, state.x);
    assert_eq!(scene.state.v, state.v);
    assert_eq!(scene.state.time, state.time);
    before.save(&file).unwrap();
    let reopened = FittingProject::load(&file).unwrap();
    assert_eq!(reopened.mesh_signature, signature);
    assert_eq!(
        reopened.template.fabric_appearance,
        before.template.fabric_appearance
    );
}

#[test]
fn clipboard_keeps_optical_identity_and_material_split_inherits_it() {
    use fashioncraft::authoring::clipboard::PatternClipboard;
    let source = GarmentTemplate::two_panel_demo();
    let id = source.fabrics[0].id.clone();
    let colored = authoring::apply(
        &source,
        Command::FabricAppearance {
            fabric: id.clone(),
            appearance: Some(FabricAppearance {
                color_srgb: [220, 30, 60],
                roughness: 0.4,
                ..Default::default()
            }),
        },
    )
    .unwrap()
    .template;
    let clip = PatternClipboard::capture(&colored, &[colored.panels[0].id.clone()]).unwrap();
    let clip = PatternClipboard::from_text(&clip.to_text().unwrap()).unwrap();
    let pasted = authoring::apply(
        &source,
        Command::Paste {
            clipboard: clip,
            center: glam::DVec2::new(2., 0.),
        },
    )
    .unwrap();
    let copy = pasted.template.panels.last().unwrap();
    assert_ne!(
        copy.fabric, id,
        "equal mechanics/name must not merge distinct optics"
    );
    assert_eq!(
        pasted.template.fabric_appearance.get(&copy.fabric),
        colored.fabric_appearance.get(&id)
    );
    assert!(!pasted.template.fabric_appearance.contains_key(&id));
    let edited = authoring::apply(
        &colored,
        Command::Fabric {
            panel: colored.panels[0].id.clone(),
            name: "Custom mechanical fabric".into(),
            properties: colored.fabrics[0].properties.clone(),
            all_panels: false,
        },
    )
    .unwrap();
    let split = &edited.template.panels[0].fabric;
    assert_ne!(split, &id);
    assert_eq!(
        edited.template.fabric_appearance.get(split),
        colored.fabric_appearance.get(&id)
    );
    let clip = PatternClipboard::capture(&edited.template, &[edited.template.panels[1].id.clone()])
        .unwrap();
    let text: serde_json::Value = serde_json::from_str(&clip.to_text().unwrap()).unwrap();
    assert!(text["template"]["fabric_appearance"].get(split).is_none());
}

#[test]
fn fabric_textures_are_validated_and_turn_with_their_angle() {
    use fashioncraft::garment::appearance::{FabricAppearance, FabricTexture};
    use glam::DVec2;
    let texture = FabricTexture {
        asset_id: "image_1".into(),
        width_mm: 40.,
        height_mm: 20.,
        rotation_degrees: 90.,
        relief_mm: 1.,
        normal_asset_id: None,
        roughness_asset_id: None,
        ..Default::default()
    };
    let mut a = FabricAppearance {
        texture: Some(texture.clone()),
        ..Default::default()
    };
    a.validate().unwrap();
    // Turned a quarter: the warp runs along the image's negative height.
    let uv = texture.image_uv(DVec2::new(0.04, 0.));
    assert!(uv.x.abs() < 1e-12 && (uv.y + 2.).abs() < 1e-12, "{uv:?}");
    for broken in [
        FabricTexture {
            width_mm: 0.,
            ..texture.clone()
        },
        FabricTexture {
            relief_mm: 6.,
            ..texture.clone()
        },
        FabricTexture {
            asset_id: "../escape".into(),
            ..texture.clone()
        },
        FabricTexture {
            rotation_degrees: f64::NAN,
            ..texture.clone()
        },
    ] {
        a.texture = Some(broken);
        assert!(a.validate().is_err());
    }
    // A fabric without a texture writes no texture key, as before.
    let text = serde_json::to_string(&FabricAppearance::default()).unwrap();
    assert!(!text.contains("texture"));
}

#[test]
fn look_thickness_is_bounded_and_absent_by_default() {
    use fashioncraft::garment::appearance::{FabricAppearance, MAX_RENDER_THICKNESS_MM};
    let mut a = FabricAppearance {
        render_thickness_mm: 3.,
        ..Default::default()
    };
    a.validate().unwrap();
    for broken in [-0.1, MAX_RENDER_THICKNESS_MM + 0.1, f64::NAN] {
        a.render_thickness_mm = broken;
        assert!(a.validate().is_err(), "{broken}");
    }
    let text = serde_json::to_string(&FabricAppearance::default()).unwrap();
    assert!(!text.contains("render_thickness"));
}
