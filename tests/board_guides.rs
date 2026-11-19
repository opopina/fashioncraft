//! Rulers and guidelines travel with the project, are validated, and projects without any are
//! written exactly as before.
use fashioncraft::{
    fitting::{BoardGuides, FittingProject, GuideAxis, GuideLine},
    garment::presets::GarmentKind,
};
use std::path::PathBuf;

fn project() -> FittingProject {
    FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
}

fn guides() -> BoardGuides {
    BoardGuides {
        origin_m: [0.05, -0.1],
        lines: vec![
            GuideLine {
                axis: GuideAxis::Horizontal,
                position_m: 0.25,
                locked: false,
            },
            GuideLine {
                axis: GuideAxis::Vertical,
                position_m: -0.4,
                locked: true,
            },
        ],
    }
}

#[test]
fn guides_round_trip_through_a_saved_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = project();
    source.board_guides = guides();
    let file = dir.path().join("coat.json");
    source.save(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"axis\": \"horizontal\""));
    assert_eq!(FittingProject::load(&file).unwrap().board_guides, guides());
}

#[test]
fn projects_without_guides_are_written_as_before() {
    let text = serde_json::to_string(&project()).unwrap();
    assert!(!text.contains("board_guides"));
    assert!(BoardGuides::default().is_empty());
    // Only moving the origin is enough to store the block.
    let moved = BoardGuides {
        origin_m: [0.1, 0.],
        lines: vec![],
    };
    assert!(!moved.is_empty());
}

#[test]
fn impossible_guides_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut broken = guides();
    broken.lines[0].position_m = f64::NAN;
    assert!(broken.validate().is_err());
    let mut source = project();
    source.board_guides = broken;
    let file = dir.path().join("broken.json");
    assert!(source.save(&file).is_err());
    assert!(!file.exists());
    let too_many = BoardGuides {
        origin_m: [0., 0.],
        lines: vec![guides().lines[0]; BoardGuides::MAX_LINES + 1],
    };
    assert!(too_many.validate().is_err());
    let lost_origin = BoardGuides {
        origin_m: [f64::INFINITY, 0.],
        lines: vec![],
    };
    assert!(lost_origin.validate().is_err());
}
