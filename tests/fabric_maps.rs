//! Fabric maps: texture sets recognised from the names texture tools give their exports, the
//! maps kept through a save and reload, and carried by every export where the format has a
//! place for them (converted where the format reads them another way).
use fashioncraft::{
    fbx_export::{self, FbxScene},
    fitting::FittingProject,
    garment::{
        appearance::{FabricAppearance, FabricTexture, MAX_DISPLACEMENT_MM, MapSlot},
        presets::GarmentKind,
        texture_set::{self, MapKind, NormalConvention},
    },
    gltf_export::{self, GltfScene},
    obj_export::{self, ObjOptions, ObjScene},
    usd_export::{self, UsdScene},
};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

fn kind(name: &str) -> Option<(MapKind, Option<NormalConvention>, String)> {
    texture_set::recognise(Path::new(name)).map(|r| (r.kind, r.convention, r.material))
}

#[test]
fn map_suffixes_of_texture_tools_are_recognised() {
    use MapKind::*;
    use NormalConvention::*;
    // Substance Painter's usual export names.
    for (name, expected) in [
        ("Jacket_BaseColor.png", BaseColor),
        ("Jacket_Normal.png", Normal),
        ("Jacket_Roughness.png", Roughness),
        ("Jacket_Metallic.png", Metalness),
        ("Jacket_Height.png", Height),
        ("Jacket_Opacity.png", Opacity),
        ("Jacket_Mixed_AO.png", Occlusion),
        // Designer and Sampler: lower case.
        ("lace_basecolor.png", BaseColor),
        ("lace_normal.png", Normal),
        ("lace_roughness.png", Roughness),
        ("lace_metallic.png", Metalness),
        ("lace_height.png", Height),
        ("lace_opacity.png", Opacity),
        ("lace_ambientocclusion.png", Occlusion),
        // Other words for the same maps.
        ("denim_albedo.jpg", BaseColor),
        ("denim_diffuse.tga", BaseColor),
        ("denim_diff_2k.jpg", BaseColor),
        ("Fabric062_1K-JPG_Color.jpg", BaseColor),
        ("denim_metalness.png", Metalness),
        ("denim_displacement.png", Height),
        ("denim_disp_4k.png", Height),
        ("denim_alpha.png", Opacity),
        ("denim_ao.png", Occlusion),
        ("Fabric062_1K-JPG_AmbientOcclusion.jpg", Occlusion),
    ] {
        assert_eq!(kind(name).map(|k| k.0), Some(expected), "{name}");
    }
    // Glossiness is roughness turned over; normal maps may say which way they are drawn.
    assert_eq!(kind("silk_Glossiness.png").unwrap().0, Glossiness);
    assert_eq!(kind("silk_smoothness.png").unwrap().0, Glossiness);
    for (name, convention) in [
        ("Fabric062_1K-JPG_NormalGL.jpg", Some(OpenGl)),
        ("Fabric062_1K-JPG_NormalDX.jpg", Some(DirectX)),
        ("denim_nor_gl_4k.jpg", Some(OpenGl)),
        ("denim_nor_dx_4k.jpg", Some(DirectX)),
        ("Jacket_Normal_DirectX.png", Some(DirectX)),
        ("Jacket_Normal_OpenGL.png", Some(OpenGl)),
        ("Jacket_Normal.png", None),
    ] {
        let (k, c, _) = kind(name).unwrap();
        assert_eq!((k, c), (Normal, convention), "{name}");
    }
    // The material is the name before the suffix, without resolutions and formats.
    assert_eq!(kind("Fabric062_1K-JPG_Color.jpg").unwrap().2, "fabric062");
    assert_eq!(
        kind("denim_fabric_nor_gl_4k.jpg").unwrap().2,
        "denim_fabric"
    );
    assert_eq!(kind("Jacket_Mixed_AO.png").unwrap().2, "jacket");
    assert_eq!(kind("albedo.png").unwrap().2, "");
    // Packed channel maps, maps we do not draw and files we cannot read are left alone.
    for name in [
        "Jacket_occlusionRoughnessMetallic.png",
        "Jacket_MetallicSmoothness.png",
        "denim_arm_4k.jpg",
        "Jacket_Emissive.png",
        "Jacket_Specular.png",
        "Jacket_BaseColor.exr",
        "Jacket_BaseColor.tif",
        "Jacket.sbsar",
        "notes.txt",
        "Jacket.png",
    ] {
        assert_eq!(kind(name), None, "{name}");
    }
    assert!(texture_set::is_substance_archive(Path::new("Velvet.SBSAR")));
}

#[test]
fn a_folder_sorts_into_sets_with_the_surest_file_for_each_map() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "Velvet_BaseColor.png",
        "Velvet_Normal_DirectX.png",
        "Velvet_Glossiness.png",
        "Velvet_Roughness.png",
        "Velvet_Height.png",
        "Velvet_Bump.png",
        "Velvet_Metallic.png",
        "Velvet_Opacity.png",
        "Velvet_Mixed_AO.png",
        "Velvet_Emissive.png",
        "Lining_BaseColor.png",
        "Lining_Normal.png",
        "readme.txt",
    ] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    std::fs::create_dir(dir.path().join("previews")).unwrap();
    let files = texture_set::folder_files(dir.path()).unwrap();
    assert_eq!(files.len(), 13, "sub-folders are not read");
    let found = texture_set::group(&files);
    assert_eq!(found.sets.len(), 2);
    let velvet = &found.sets[0];
    assert_eq!(velvet.name, "velvet");
    assert_eq!(velvet.len(), 7);
    let file = |p: &Option<PathBuf>| p.as_ref().unwrap().file_name().unwrap().to_owned();
    assert_eq!(file(&velvet.roughness), "Velvet_Roughness.png");
    assert!(!velvet.roughness_is_glossiness);
    assert_eq!(file(&velvet.height), "Velvet_Height.png");
    assert_eq!(file(&velvet.occlusion), "Velvet_Mixed_AO.png");
    assert!(velvet.normal_directx());
    let lining = &found.sets[1];
    assert_eq!((lining.name.as_str(), lining.len()), ("lining", 2));
    assert!(
        !lining.normal_directx(),
        "OpenGL unless the name says otherwise"
    );
    let ignored: Vec<_> = found
        .ignored
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        ignored,
        [
            "Velvet_Bump.png",
            "Velvet_Emissive.png",
            "Velvet_Glossiness.png",
            "readme.txt"
        ]
    );
    // A glossiness map alone fills the roughness slot, inverted.
    let gloss = texture_set::group(&[PathBuf::from("Silk_BaseColor.png"), "Silk_Gloss.png".into()]);
    assert!(gloss.sets[0].roughness_is_glossiness);
}

/// A fabric whose texture has every map.
fn every_map() -> FabricAppearance {
    FabricAppearance {
        color_srgb: [200, 180, 160],
        roughness: 0.7,
        metalness: 0.25,
        texture: Some(FabricTexture {
            asset_id: "color".into(),
            width_mm: 100.,
            height_mm: 100.,
            normal_asset_id: Some("normal".into()),
            normal_directx: true,
            roughness_asset_id: Some("gloss".into()),
            roughness_is_glossiness: true,
            metalness_asset_id: Some("metal".into()),
            opacity_asset_id: Some("opacity".into()),
            displacement_asset_id: Some("height".into()),
            displacement_mm: 1.5,
            occlusion_asset_id: Some("ao".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn every_map_survives_a_save_and_reload_and_old_files_still_open() {
    let a = every_map();
    a.validate().unwrap();
    let text = serde_json::to_string(&a).unwrap();
    let back: FabricAppearance = serde_json::from_str(&text).unwrap();
    assert_eq!(back, a);
    let t = a.texture.as_ref().unwrap();
    assert_eq!(
        t.asset_ids().collect::<Vec<_>>(),
        [
            "color", "normal", "gloss", "metal", "opacity", "height", "ao"
        ]
    );
    for slot in MapSlot::ALL {
        assert!(t.map(slot).is_some(), "{slot:?}");
    }
    // A file from before the maps opens with none, and a plain fabric writes no new keys.
    let old = r#"{"color_srgb":[1,2,3],"roughness":0.5,"texture":{"asset_id":"x","width_mm":10.0,"height_mm":10.0}}"#;
    let old: FabricAppearance = serde_json::from_str(old).unwrap();
    assert_eq!(old.metalness, 0.);
    let old_texture = old.texture.as_ref().unwrap();
    assert!(MapSlot::ALL.iter().all(|s| old_texture.map(*s).is_none()));
    assert!(!old_texture.normal_directx && !old_texture.roughness_is_glossiness);
    let plain = serde_json::to_string(&old).unwrap();
    for key in [
        "metalness",
        "directx",
        "glossiness",
        "opacity",
        "displacement",
        "occlusion",
    ] {
        assert!(!plain.contains(key), "{key} in {plain}");
    }
    // Out-of-range values are refused.
    let mut bad = a.clone();
    bad.metalness = 1.5;
    assert!(bad.validate().is_err());
    let mut bad = a.clone();
    bad.metalness = f64::NAN;
    assert!(bad.validate().is_err());
    let mut bad = a;
    bad.texture.as_mut().unwrap().displacement_mm = MAX_DISPLACEMENT_MM + 1.;
    assert!(bad.validate().is_err());
    bad.texture.as_mut().unwrap().metalness_asset_id = Some("bad id!".into());
    assert!(bad.validate().is_err());
}

/// Small images for each map, written into `dir`: every pixel of a map holds `value`.
fn images(dir: &Path) -> Vec<(String, PathBuf)> {
    let maps = [
        ("color", [200, 100, 50, 255]),
        ("normal", [128, 40, 255, 255]),
        ("gloss", [64, 64, 64, 255]),
        ("metal", [255, 255, 255, 255]),
        ("opacity", [0, 0, 0, 255]),
        ("height", [128, 128, 128, 255]),
        ("ao", [200, 200, 200, 255]),
    ];
    maps.iter()
        .map(|(name, value)| {
            let path = dir.join(format!("{name}.png"));
            image::RgbaImage::from_pixel(4, 4, image::Rgba(*value))
                .save(&path)
                .unwrap();
            (name.to_string(), path)
        })
        .collect()
}

fn project() -> (FittingProject, String) {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let fabric = project.template.panels[0].fabric.clone();
    for panel in &mut project.template.panels {
        panel.fabric = fabric.clone();
    }
    project
        .template
        .fabric_appearance
        .insert(fabric.clone(), every_map());
    (project, fabric)
}

/// The JSON and binary chunks of a GLB.
fn chunks(bytes: &[u8]) -> (Value, Vec<u8>) {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let json_len = word(12);
    let json = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    let at = 20 + json_len;
    (json, bytes[at + 8..at + 8 + word(at)].to_vec())
}

/// The first pixel of the image a glTF texture embeds.
fn texel(json: &Value, bin: &[u8], texture: &Value) -> [u8; 4] {
    let texture = texture["index"].as_u64().unwrap() as usize;
    let image = json["textures"][texture]["source"].as_u64().unwrap() as usize;
    let view = json["images"][image]["bufferView"].as_u64().unwrap() as usize;
    let view = &json["bufferViews"][view];
    let start = view["byteOffset"].as_u64().unwrap() as usize;
    let len = view["byteLength"].as_u64().unwrap() as usize;
    image::load_from_memory(&bin[start..start + len])
        .unwrap()
        .to_rgba8()
        .get_pixel(0, 0)
        .0
}

#[test]
fn gltf_carries_the_maps_the_way_gltf_reads_them() {
    let (project, _) = project();
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let images = images(dir.path());
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
    let pbr = &material["pbrMetallicRoughness"];
    // Opacity rides in the colour image's alpha, cut out at one half.
    assert_eq!(material["alphaMode"], "MASK");
    assert_eq!(material["alphaCutoff"], 0.5);
    assert_eq!(
        texel(&json, &bin, &pbr["baseColorTexture"]),
        [200, 100, 50, 0]
    );
    // Roughness (the glossiness map inverted) in green, metalness in blue.
    assert_eq!(pbr["metallicFactor"], 1.0);
    assert_eq!(pbr["roughnessFactor"], 1.0);
    let packed = texel(&json, &bin, &pbr["metallicRoughnessTexture"]);
    assert_eq!((packed[1], packed[2]), (255 - 64, 255));
    // The DirectX normal map made OpenGL-handed: green flipped.
    assert_eq!(
        texel(&json, &bin, &material["normalTexture"]),
        [128, 255 - 40, 255, 255]
    );
    assert_eq!(texel(&json, &bin, &material["occlusionTexture"])[0], 200);
    // glTF has no displacement.
    assert!(!json.to_string().contains("height"));
}

#[test]
fn usd_wires_each_map_into_the_preview_surface() {
    let (project, _) = project();
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let images = images(dir.path());
    let export = UsdScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: None,
        take: None,
    };
    let (text, textures) = usd_export::usda(&export).unwrap();
    for input in [
        "normal3f inputs:normal.connect",
        "float inputs:roughness.connect",
        "float inputs:metallic.connect",
        "float inputs:opacity.connect",
        "float inputs:occlusion.connect",
        "float inputs:displacement.connect",
        "float inputs:opacityThreshold = 0.5",
        "token outputs:displacement.connect",
        // The DirectX normal map's green flipped by the reader, the glossiness inverted.
        "float4 inputs:scale = (2.0000, -2.0000, 2.0000, 1.0000)",
        "float4 inputs:bias = (-1.0000, 1.0000, -1.0000, 0.0000)",
        "float4 inputs:scale = (-1.0000, -1.0000, -1.0000, 1.0000)",
        // 1.5 mm of displacement, in the stage's metres.
        "float4 inputs:scale = (0.0015, 0.0015, 0.0015, 1.0000)",
        "token inputs:sourceColorSpace = \"raw\"",
    ] {
        assert!(text.contains(input), "{input}");
    }
    assert!(!text.contains("float inputs:roughness = "));
    assert_eq!(textures.len(), 7);
    // As a package, every map goes in.
    let path = dir.path().join("tee.usdz");
    usd_export::write(&export, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for name in [
        "textures/normal.png",
        "textures/gloss.png",
        "textures/ao.png",
    ] {
        assert!(
            bytes.windows(name.len()).any(|w| w == name.as_bytes()),
            "{name}"
        );
    }
}

#[test]
fn obj_and_fbx_write_the_maps_beside_the_file() {
    let (project, fabric) = project();
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let images = images(dir.path());
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let export = ObjScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: None,
        atlas: None,
    };
    let summary = obj_export::write(&export, &ObjOptions::default(), &out.join("tee.obj")).unwrap();
    assert_eq!(
        summary.textures, 6,
        "colour and five maps; MTL has no occlusion"
    );
    let mtl = std::fs::read_to_string(out.join("tee.mtl")).unwrap();
    assert!(mtl.contains(&format!("newmtl {fabric}")));
    for line in [
        "Pr 0.700",
        "Pm 0.250",
        "map_Kd tee_color.png",
        "map_Pr tee_gloss_rough.png",
        "map_Pm tee_metal.png",
        "map_d tee_opacity.png",
        "disp tee_height.png",
        "map_Bump tee_normal_normal_gl.png",
    ] {
        assert!(mtl.contains(line), "{line} in\n{mtl}");
    }
    // The glossiness map written as roughness, the normal map OpenGL-handed.
    let pixel = |name: &str| {
        image::open(out.join(name))
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)
            .0
    };
    assert_eq!(pixel("tee_gloss_rough.png")[0], 255 - 64);
    assert_eq!(pixel("tee_normal_normal_gl.png")[1], 255 - 40);
    let export = FbxScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: None,
        take: None,
    };
    let path = out.join("tee.fbx");
    fbx_export::write(&export, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let has = |s: &str| bytes.windows(s.len()).any(|w| w == s.as_bytes());
    for property in [
        "DiffuseColor",
        "NormalMap",
        "ShininessExponent",
        "ReflectionFactor",
        "TransparencyFactor",
        "DisplacementColor",
    ] {
        assert!(has(property), "{property}");
    }
    assert!(has("tee_normal_normal_gl.png") && has("tee_gloss_rough.png"));
    assert!(out.join("tee_height.png").is_file());
}
