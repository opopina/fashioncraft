//! USD export: a text layer with a mesh per piece, preview-surface materials, the avatar and
//! a take as time samples; and a USDZ package that stores its files uncompressed on 64-byte
//! boundaries, root layer first.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_animation::Take,
    fitting::FittingProject,
    garment::{
        appearance::{FabricAppearance, FabricTexture},
        presets::GarmentKind,
    },
    usd_export::{self, UsdScene},
};
use glam::DVec3;
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// The files of a stored zip: name, offset of the data, and the data.
fn entries(bytes: &[u8]) -> Vec<(String, usize, Vec<u8>, u32)> {
    let mut out = Vec::new();
    let mut at = 0;
    let u16 = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
    let u32 = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    while u32(at) == 0x0403_4b50 {
        assert_eq!(u16(at + 8), 0, "stored, not compressed");
        let crc = u32(at + 14);
        let size = u32(at + 18) as usize;
        let name_len = u16(at + 26);
        let extra = u16(at + 28);
        let name = String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec()).unwrap();
        let data = at + 30 + name_len + extra;
        out.push((name, data, bytes[data..data + size].to_vec(), crc));
        at = data + size;
    }
    assert_eq!(u32(at), 0x0201_4b50, "central directory follows the files");
    out
}

#[test]
fn the_layer_has_a_mesh_per_piece_with_materials_and_the_avatar() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let export = UsdScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&avatar),
        take: None,
    };
    let (text, textures) = usd_export::usda(&export).unwrap();
    assert!(textures.is_empty());
    assert!(text.starts_with("#usda 1.0"));
    assert!(text.contains("upAxis = \"Y\"") && text.contains("metersPerUnit = 1"));
    assert_eq!(
        text.matches("def Mesh ").count(),
        scene.garment.panels.len() + 1
    );
    assert!(text.contains("def Mesh \"avatar\""));
    assert!(text.contains("info:id = \"UsdPreviewSurface\""));
    assert!(!text.contains("timeSamples"));
    // Every triangle once, as three corners.
    let faces: usize = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("int[] faceVertexCounts = ["))
        .map(|l| l.trim_end_matches(']').split(", ").count())
        .sum();
    assert_eq!(
        faces,
        scene.garment.mesh.triangles.len() + avatar.triangles.len()
    );
    assert_eq!(
        text.matches("uniform bool doubleSided = 1").count(),
        scene.garment.panels.len()
    );
}

#[test]
fn a_take_animates_the_points_one_sample_per_frame() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let x = &scene.state.x;
    let frame = |dy: f32| -> Vec<[f32; 3]> {
        x.iter()
            .map(|p| [p.x as f32, p.y as f32 + dy, p.z as f32])
            .collect()
    };
    let take = Take {
        fps: 24.,
        frames: vec![frame(0.), frame(0.1), frame(0.2)],
    };
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let bodies: Vec<Vec<DVec3>> = (0..3).map(|_| avatar.vertices.clone()).collect();
    let export = UsdScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&avatar),
        take: Some((&take, Some(bodies))),
    };
    let (text, _) = usd_export::usda(&export).unwrap();
    assert!(text.contains("endTimeCode = 2") && text.contains("timeCodesPerSecond = 24"));
    assert_eq!(
        text.matches("point3f[] points.timeSamples").count(),
        scene.garment.panels.len() + 1
    );
    assert_eq!(
        text.matches("            2: [").count(),
        scene.garment.panels.len() + 1
    );
    let mut other = take.clone();
    other.frames[1].pop();
    other.frames[0].pop();
    let export = UsdScene {
        take: Some((&other, None)),
        ..export
    };
    assert!(usd_export::usda(&export).is_err());
}

#[test]
fn a_usdz_package_stores_the_layer_first_and_its_textures_on_64_byte_boundaries() {
    // The zip records the standard CRC-32 of each file.
    let package = usd_export::usdz_package(&[("a.usda".into(), b"123456789".to_vec())]).unwrap();
    let files = entries(&package);
    assert_eq!(files[0].3, 0xCBF4_3926);
    assert_eq!(files[0].1 % 64, 0);

    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let fabric = project.template.panels[0].fabric.clone();
    project.template.fabric_appearance.insert(
        fabric,
        FabricAppearance {
            color_srgb: [200, 30, 30],
            roughness: 0.6,
            texture: Some(FabricTexture {
                asset_id: "print".into(),
                width_mm: 80.,
                height_mm: 80.,
                rotation_degrees: 0.,
                relief_mm: 0.,
                normal_asset_id: None,
                roughness_asset_id: None,
            }),
            ..Default::default()
        },
    );
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("print.png");
    std::fs::write(&image, b"image bytes, any length at all").unwrap();
    let images = [("print".to_string(), image)];
    let export = UsdScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: None,
        take: None,
    };
    let path = dir.path().join("tee.usdz");
    usd_export::write(&export, &path).unwrap();
    let files = entries(&std::fs::read(&path).unwrap());
    assert_eq!(files[0].0, "tee.usda");
    assert_eq!(files[1].0, "textures/print.png");
    assert_eq!(files[1].2, b"image bytes, any length at all");
    for (name, offset, _, _) in &files {
        assert_eq!(offset % 64, 0, "{name}");
    }
    let text = String::from_utf8(files[0].2.clone()).unwrap();
    assert!(text.contains("asset inputs:file = @textures/print.png@"));
    // As a text layer, the texture goes into a folder beside it.
    let path = dir.path().join("tee.usda");
    usd_export::write(&export, &path).unwrap();
    assert!(dir.path().join("textures/print.png").is_file());
}
