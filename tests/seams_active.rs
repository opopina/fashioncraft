//! Seams switched off stay in the document but are not sewn; switching them back on, or
//! deleting several at once, is one undo step each.
use fashioncraft::{
    authoring::{self, Command},
    garment::presets::{self, GarmentKind},
};

#[test]
fn an_inactive_seam_is_kept_but_not_sewn() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let sewn = base.clone().compile().unwrap().seams.len();
    let first = base.seams[0].id.clone();
    let off = authoring::apply(
        &base,
        Command::SetSeamsActive {
            seams: vec![first.clone()],
            active: false,
        },
    )
    .unwrap()
    .template;
    assert_eq!(off.seams.len(), base.seams.len());
    assert!(off.inactive_seams.contains(&first));
    assert_eq!(off.clone().compile().unwrap().seams.len(), sewn - 1);
    // Saved and read back.
    let text = serde_json::to_string(&off).unwrap();
    let back: fashioncraft::garment::GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(back.inactive_seams, off.inactive_seams);
    // Turning it off again is refused; turning every seam on brings it back.
    assert!(
        authoring::apply(
            &off,
            Command::SetSeamsActive {
                seams: vec![first.clone()],
                active: false
            }
        )
        .is_err()
    );
    let all: Vec<String> = off.seams.iter().map(|s| s.id.clone()).collect();
    let on = authoring::apply(
        &off,
        Command::SetSeamsActive {
            seams: all,
            active: true,
        },
    )
    .unwrap()
    .template;
    assert!(on.inactive_seams.is_empty());
    assert_eq!(on.compile().unwrap().seams.len(), sewn);
}

#[test]
fn several_seams_go_in_one_edit_and_take_their_switch_with_them() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let ids: Vec<String> = base.seams.iter().take(2).map(|s| s.id.clone()).collect();
    let off = authoring::apply(
        &base,
        Command::SetSeamsActive {
            seams: ids.clone(),
            active: false,
        },
    )
    .unwrap()
    .template;
    let gone = authoring::apply(&off, Command::RemoveSeams { seams: ids.clone() })
        .unwrap()
        .template;
    assert_eq!(gone.seams.len(), base.seams.len() - 2);
    assert!(gone.inactive_seams.is_empty());
    gone.compile().unwrap();
    assert!(authoring::apply(&base, Command::RemoveSeams { seams: vec![] }).is_err());
    assert!(
        authoring::apply(
            &base,
            Command::RemoveSeams {
                seams: vec!["nowhere".into()]
            }
        )
        .is_err()
    );
    assert!(
        authoring::apply(
            &base,
            Command::SetSeamsActive {
                seams: vec!["nowhere".into()],
                active: false
            }
        )
        .is_err()
    );
}
