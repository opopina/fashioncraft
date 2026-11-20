//! Edit texture: a piece's texture moved across it (looks only, followed by the exported
//! texture coordinates), and a piece's grain turned (warp and weft follow).
use fashioncraft::{
    authoring::{self, Command, only_appearance_changed, same_physics},
    garment::{
        appearance::{FabricAppearance, FabricTexture},
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
    },
    gltf_export::texture_coordinates,
};
use glam::DVec2;

fn textured() -> fashioncraft::garment::GarmentTemplate {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let fabric = base.panels[0].fabric.clone();
    let texture: FabricTexture = serde_json::from_value(serde_json::json!({
        "asset_id": "image_1",
        "width_mm": 50.0,
        "height_mm": 50.0,
        "rotation_degrees": 0.0,
    }))
    .unwrap();
    authoring::apply(
        &base,
        Command::FabricAppearance {
            fabric,
            appearance: Some(FabricAppearance {
                texture: Some(texture),
                ..Default::default()
            }),
        },
    )
    .unwrap()
    .template
}

#[test]
fn moving_a_texture_shifts_its_coordinates_and_only_the_look() {
    let base = textured();
    let id = base.panels[0].id.clone();
    let offset = DVec2::new(0.01, 0.025);
    let moved = authoring::apply(
        &base,
        Command::SetTextureOffset {
            panel: id.clone(),
            offset,
        },
    )
    .unwrap()
    .template;
    assert!(same_physics(&base, &moved));
    assert!(only_appearance_changed(&base, &moved));
    let g = base.clone().compile().unwrap();
    let (before, after) = (
        texture_coordinates(&base, &g),
        texture_coordinates(&moved, &g),
    );
    let range = g.panels.iter().find(|r| r.id == id).unwrap();
    let grain = g.materials[range.material].grain_degrees.to_radians();
    let (s, c) = grain.sin_cos();
    // The image moved by the offset, seen along the grain, in 50 mm repeats.
    let shift = DVec2::new(offset.x * c + offset.y * s, -offset.x * s + offset.y * c) / 0.05;
    for v in range.vertices.clone() {
        assert!((before[v] - after[v] - shift).length() < 1e-9);
    }
    // Other pieces keep theirs; zero puts it back; bad values are refused.
    let other = g.panels.iter().find(|r| r.id != id).unwrap();
    assert!(other.vertices.clone().all(|v| before[v] == after[v]));
    let back = authoring::apply(
        &moved,
        Command::SetTextureOffset {
            panel: id.clone(),
            offset: DVec2::ZERO,
        },
    )
    .unwrap()
    .template;
    assert!(back.texture_offsets.is_empty());
    assert!(
        authoring::apply(
            &base,
            Command::SetTextureOffset {
                panel: id,
                offset: DVec2::new(f64::NAN, 0.)
            }
        )
        .is_err()
    );
}

#[test]
fn turning_a_pieces_grain_changes_its_physics_and_keeps_a_quad_grid_along_it() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let id = base.panels[0].id.clone();
    let quads = authoring::apply(
        &base,
        Command::SetMeshQuads {
            panels: vec![id.clone()],
            quads: true,
        },
    )
    .unwrap()
    .template;
    let turned = authoring::apply(
        &quads,
        Command::SetPanelGrain {
            panel: id.clone(),
            degrees: 405.,
        },
    )
    .unwrap()
    .template;
    assert_eq!(turned.panels[0].grain_degrees, 45.);
    assert!(!same_physics(&quads, &turned));
    let meshing: MeshingSettings = turned.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .meshing_settings();
    assert_eq!(meshing.axis_degrees, 45.);
    turned.clone().compile().unwrap();
    assert!(
        authoring::apply(
            &turned,
            Command::SetPanelGrain {
                panel: id,
                degrees: 45.
            }
        )
        .is_err()
    );
}
