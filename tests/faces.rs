//! Front and back faces and the procedural weave: document-level checks. Flipping a piece's
//! normal only changes its look (no new simulation), toggles back, and goes with the piece;
//! weaves are validated and old documents stay unchanged.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        appearance::{FabricAppearance, Weave, WeaveKind},
    },
};

#[test]
fn flipping_a_normal_changes_only_the_look_and_toggles_back() {
    let t = GarmentTemplate::two_panel_demo();
    let id = t.panels[0].id.clone();
    let flipped = authoring::apply(
        &t,
        Command::FlipNormals {
            panels: vec![id.clone()],
        },
    )
    .unwrap();
    assert!(!flipped.reset_simulation, "a look, not physics");
    assert!(authoring::only_appearance_changed(&t, &flipped.template));
    assert!(flipped.template.flipped_panels.contains(&id));
    let back = authoring::apply(&flipped.template, Command::FlipNormals { panels: vec![id] })
        .unwrap()
        .template;
    assert!(back.flipped_panels.is_empty());
    assert!(
        authoring::apply(
            &t,
            Command::FlipNormals {
                panels: vec!["nope".into()]
            }
        )
        .is_err()
    );
    let removed = authoring::apply(
        &flipped.template,
        Command::RemovePanel {
            panel: flipped.template.panels[0].id.clone(),
        },
    )
    .unwrap();
    assert!(
        removed.template.flipped_panels.is_empty(),
        "goes with the piece"
    );
}

#[test]
fn weaves_and_back_colours_are_validated_and_optional() {
    let mut a = FabricAppearance::default();
    assert!(!serde_json::to_string(&a).unwrap().contains("weave"));
    assert!(!serde_json::to_string(&a).unwrap().contains("back_color"));
    a.back_color_srgb = Some([10, 20, 30]);
    a.weave = Some(Weave {
        kind: WeaveKind::Satin,
        ..Default::default()
    });
    a.validate().unwrap();
    let back: FabricAppearance = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(back, a);
    assert_eq!(a.weave_parameters(), [3., 0.001, 0.35]);
    for (spacing, contrast) in [(0.01, 0.3), (1., 1.5)] {
        a.weave = Some(Weave {
            kind: WeaveKind::Plain,
            spacing_mm: spacing,
            contrast,
        });
        assert!(a.validate().is_err());
    }
}
