//! glTF export: a GLB or glTF that the glTF importer reads back with every triangle, the
//! fabric's front faces as glTF's front faces, fabrics as materials with their embedded
//! textures, and a take as morph targets driven by an animation of the weights.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    avatar_animation::Take,
    fitting::FittingProject,
    garment::{
        appearance::{FabricAppearance, FabricTexture},
        presets::GarmentKind,
    },
    gltf_avatar,
    gltf_export::{self, GltfScene},
};
use serde_json::Value;
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// The JSON and binary chunks of a GLB.
fn chunks(bytes: &[u8]) -> (Value, Vec<u8>) {
    assert_eq!(&bytes[..4], b"glTF");
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!(word(4), 2);
    assert_eq!(word(8), bytes.len());
    let json_len = word(12);
    assert_eq!(&bytes[16..20], b"JSON");
    assert_eq!(json_len % 4, 0);
    let json = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    let at = 20 + json_len;
    assert_eq!(&bytes[at + 4..at + 8], b"BIN\0");
    (json, bytes[at + 8..at + 8 + word(at)].to_vec())
}

#[test]
fn a_static_glb_reads_back_with_the_fabric_front_facing_out() {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    // The preset's back piece is laid out like the front, its front face towards the body;
    // turned over, both pieces show their front outside.
    let back_piece = project.template.panels[1].id.clone();
    project.template.flipped_panels.insert(back_piece.clone());
    let scene = project.compile().unwrap();
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garment.glb");
    let export = GltfScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&avatar),
        take: None,
    };
    gltf_export::write(&export, &path).unwrap();
    let back = gltf_avatar::load(&path, None, Default::default()).unwrap();
    let triangles = scene.garment.mesh.triangles.len() + avatar.triangles.len();
    assert_eq!(back.mesh.triangles.len(), triangles);
    // The front piece faces +z and the turned-over back piece -z, away from the body.
    let (json, bin) = chunks(&std::fs::read(&path).unwrap());
    assert_eq!(json["meshes"].as_array().unwrap().len(), 2);
    assert!(json["materials"][0]["doubleSided"].as_bool().unwrap());
    let x = &scene.state.x;
    let primitives = json["meshes"][0]["primitives"].as_array().unwrap();
    let mut seen = 0;
    for primitive in primitives {
        let indices = primitive["indices"].as_u64().unwrap() as usize;
        let view = &json["bufferViews"]
            [json["accessors"][indices]["bufferView"].as_u64().unwrap() as usize];
        let start = view["byteOffset"].as_u64().unwrap() as usize;
        let len = view["byteLength"].as_u64().unwrap() as usize;
        let written: Vec<usize> = bin[start..start + len]
            .chunks(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()) as usize)
            .collect();
        seen += written.len() / 3;
        let side = written
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [x[t[0]], x[t[1]], x[t[2]]];
                let centre = (a + b + c) / 3.;
                (b - a).cross(c - a).z * centre.z.signum()
            })
            .sum::<f64>();
        assert!(side > 0., "{} faces inwards", primitive["material"]);
    }
    assert_eq!(seen, scene.garment.mesh.triangles.len());
    assert!(project.template.flipped_panels.contains(&back_piece));
    // The positions' bounds are those of the cloth.
    let position = json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .unwrap() as usize;
    let max = json["accessors"][position]["max"].as_array().unwrap();
    let top = x.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    assert!((max[1].as_f64().unwrap() - top).abs() < 1e-5);
    assert!(!bin.is_empty());

    // As glTF JSON, the buffer goes next to it.
    let path = dir.path().join("garment.gltf");
    gltf_export::write(&export, &path).unwrap();
    let text: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(text["buffers"][0]["uri"], "garment.bin");
    assert!(dir.path().join("garment.bin").is_file());
    let back = gltf_avatar::load(&path, None, Default::default()).unwrap();
    assert_eq!(back.mesh.triangles.len(), triangles);
}

#[test]
fn fabrics_become_materials_and_textures_are_embedded() {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let fabric = project.template.panels[0].fabric.clone();
    project.template.fabric_appearance.insert(
        fabric.clone(),
        FabricAppearance {
            color_srgb: [255, 0, 0],
            roughness: 0.4,
            texture: Some(FabricTexture {
                asset_id: "print".into(),
                width_mm: 100.,
                height_mm: 50.,
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
    let pixels = b"\x89PNG\r\n\x1a\n not really an image";
    std::fs::write(&image, pixels).unwrap();
    let images = [("print".to_string(), image)];
    let export = GltfScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: None,
        take: None,
    };
    let (json, bin) = chunks(&gltf_export::glb(&export).unwrap());
    let material = &json["materials"][0];
    assert_eq!(material["name"], fabric.as_str());
    let pbr = &material["pbrMetallicRoughness"];
    assert_eq!(pbr["baseColorFactor"][0].as_f64().unwrap(), 1.);
    assert!((pbr["roughnessFactor"].as_f64().unwrap() - 0.4).abs() < 1e-9);
    assert_eq!(pbr["baseColorTexture"]["index"], 0);
    assert_eq!(json["images"][0]["mimeType"], "image/png");
    let view = &json["bufferViews"][json["images"][0]["bufferView"].as_u64().unwrap() as usize];
    let start = view["byteOffset"].as_u64().unwrap() as usize;
    assert_eq!(&bin[start..start + pixels.len()], pixels);
    // Texture coordinates count repeats: a 100 mm wide repeat on a ~0.5 m piece spans several.
    let texcoord = json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"]
        .as_u64()
        .unwrap() as usize;
    let view =
        &json["bufferViews"][json["accessors"][texcoord]["bufferView"].as_u64().unwrap() as usize];
    let start = view["byteOffset"].as_u64().unwrap() as usize;
    let len = view["byteLength"].as_u64().unwrap() as usize;
    let us: Vec<f32> = bin[start..start + len]
        .chunks(8)
        .map(|c| f32::from_le_bytes(c[..4].try_into().unwrap()))
        .collect();
    let span = us.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        - us.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(span > 3., "{span} repeats across");
    // A missing image is an error rather than a silent untextured export.
    let export = GltfScene {
        images: &[],
        ..export
    };
    assert!(gltf_export::glb(&export).is_err());
}

#[test]
fn a_take_becomes_morph_targets_and_an_animation_of_the_weights() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let x = &scene.state.x;
    let frame = |dy: f32| -> Vec<[f32; 3]> {
        x.iter()
            .map(|p| [p.x as f32, p.y as f32 + dy, p.z as f32])
            .collect()
    };
    let take = Take {
        fps: 10.,
        frames: vec![frame(0.), frame(0.05), frame(0.1)],
    };
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let bodies: Vec<Vec<DVec3>> = (0..3)
        .map(|k| {
            avatar
                .vertices
                .iter()
                .map(|p| *p + DVec3::X * k as f64 * 0.01)
                .collect()
        })
        .collect();
    let export = GltfScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&avatar),
        take: Some((&take, Some(bodies))),
    };
    let (json, bin) = chunks(&gltf_export::glb(&export).unwrap());
    let garment = &json["meshes"][0];
    assert_eq!(
        garment["primitives"][0]["targets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(garment["weights"].as_array().unwrap().len(), 2);
    assert_eq!(
        json["meshes"][1]["primitives"][0]["targets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let animation = &json["animations"][0];
    assert_eq!(animation["channels"].as_array().unwrap().len(), 2);
    let sampler = &animation["samplers"][0];
    let input = &json["accessors"][sampler["input"].as_u64().unwrap() as usize];
    assert_eq!(input["count"], 3);
    assert!((input["max"][0].as_f64().unwrap() - 0.2).abs() < 1e-6);
    let output = &json["accessors"][sampler["output"].as_u64().unwrap() as usize];
    assert_eq!(output["count"], 6);
    // The second target lifts the cloth 10 cm: its displacements say so.
    let target = garment["primitives"][0]["targets"][1]["POSITION"]
        .as_u64()
        .unwrap() as usize;
    let max = json["accessors"][target]["max"][1].as_f64().unwrap();
    assert!((max - 0.1).abs() < 1e-5);
    // Weights: frame 0 shows nothing, frame k shows target k - 1 alone.
    let view = &json["bufferViews"][output["bufferView"].as_u64().unwrap() as usize];
    let start = view["byteOffset"].as_u64().unwrap() as usize;
    let weights: Vec<f32> = bin[start..start + 24]
        .chunks(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(weights, vec![0., 0., 1., 0., 0., 1.]);
    // A take of another mesh is refused.
    let mut other = take.clone();
    other.frames[0].pop();
    let export = GltfScene {
        take: Some((&other, None)),
        ..export
    };
    assert!(gltf_export::glb(&export).is_err());
}
