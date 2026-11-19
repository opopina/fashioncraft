//! VRM avatars: GLB files with the VRM extension, whose skinned and morphed meshes are read in
//! their bind pose; VRM 0.x avatars are turned to face +Z.
use fashioncraft::gltf_avatar::{self, Limits};
use glam::DVec3;
use std::path::Path;

/// A GLB with one skinned, morphed triangle under a node moved 5 m along X (which glTF ignores
/// for skinned meshes), and the given document extensions.
fn glb(extensions: &str) -> Vec<u8> {
    let positions = [[1f32, 0., 2.], [2., 0., 2.], [1., 1., 2.]];
    let mut bin: Vec<u8> = positions
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    bin.extend([0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    for _ in 0..3 {
        bin.extend([1f32, 0., 0., 0.].iter().flat_map(|v| v.to_le_bytes()));
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"extensions":{extensions},
        "scene":0,"scenes":[{{"nodes":[0,1]}}],
        "nodes":[{{"mesh":0,"skin":0,"translation":[5,0,0]}},{{"translation":[0,1,0]}}],
        "skins":[{{"joints":[1]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}},"targets":[{{"POSITION":0}}]}}],"weights":[0.0]}}],
        "accessors":[
          {{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[1,0,2],"max":[2,1,2]}},
          {{"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"}},
          {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}}],
        "bufferViews":[
          {{"buffer":0,"byteOffset":0,"byteLength":36}},
          {{"buffer":0,"byteOffset":36,"byteLength":12}},
          {{"buffer":0,"byteOffset":48,"byteLength":48}}],
        "buffers":[{{"byteLength":{}}}]}}"#,
        bin.len()
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(0x4e4f534au32.to_le_bytes());
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004e4942u32.to_le_bytes());
    out.extend(bin);
    out
}

fn vertices(avatar: &gltf_avatar::ImportedAvatar) -> Vec<DVec3> {
    avatar.mesh.vertices.clone()
}

#[test]
fn a_vrm_1_avatar_is_read_in_its_bind_pose() {
    let bytes = glb(r#"{"VRMC_vrm":{"specVersion":"1.0"}}"#);
    // The plain glTF importer stays strict about skins and morphs.
    assert!(gltf_avatar::import(&bytes, Path::new("."), None, Limits::default()).is_err());
    let avatar = gltf_avatar::import_vrm(&bytes, Path::new("."), Limits::default()).unwrap();
    // As authored: the skinned mesh's node translation does not apply.
    assert_eq!(
        vertices(&avatar),
        vec![
            DVec3::new(1., 0., 2.),
            DVec3::new(2., 0., 2.),
            DVec3::new(1., 1., 2.)
        ]
    );
    assert_eq!(avatar.mesh.triangles.len(), 1);
}

#[test]
fn a_vrm_0_avatar_is_turned_to_face_plus_z() {
    let avatar = gltf_avatar::import_vrm(
        &glb(r#"{"VRM":{"exporterVersion":"test"}}"#),
        Path::new("."),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        vertices(&avatar),
        vec![
            DVec3::new(-1., 0., -2.),
            DVec3::new(-2., 0., -2.),
            DVec3::new(-1., 1., -2.)
        ]
    );
    assert!(avatar.warnings.iter().any(|w| w.contains("+Z")));
}

#[test]
fn a_glb_without_the_vrm_extension_is_not_a_vrm() {
    assert!(gltf_avatar::import_vrm(&glb("{}"), Path::new("."), Limits::default()).is_err());
}
