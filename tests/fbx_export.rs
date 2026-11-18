//! FBX export: a binary FBX 7.4 whose node records nest and close properly, with a mesh model
//! per piece in centimetres, its material and texture, and the avatar.
use marvelous_cloth::{
    avatar::SurfaceMesh,
    fbx_export::{self, FbxScene},
    fitting::FittingProject,
    garment::{
        appearance::{FabricAppearance, FabricTexture},
        presets::GarmentKind,
    },
};
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// A read-back node: its name, the raw bytes of its properties, and its children.
struct Node {
    name: String,
    props: Vec<u8>,
    count: u32,
    children: Vec<Node>,
}

fn read(bytes: &[u8], at: &mut usize) -> Option<Node> {
    let u32_at = |p: usize| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    let end = u32_at(*at) as usize;
    if end == 0 {
        assert_eq!(&bytes[*at..*at + 13], &[0u8; 13]);
        *at += 13;
        return None;
    }
    let count = u32_at(*at + 4);
    let props_len = u32_at(*at + 8) as usize;
    let name_len = bytes[*at + 12] as usize;
    let name = String::from_utf8(bytes[*at + 13..*at + 13 + name_len].to_vec()).unwrap();
    let props_start = *at + 13 + name_len;
    let props = bytes[props_start..props_start + props_len].to_vec();
    let mut p = props_start + props_len;
    let mut children = Vec::new();
    if p < end {
        while let Some(child) = read(bytes, &mut p) {
            children.push(child);
        }
    }
    assert_eq!(p, end, "{name} ends where it says");
    *at = end;
    Some(Node {
        name,
        props,
        count,
        children,
    })
}

fn find<'a>(nodes: &'a [Node], name: &str) -> Vec<&'a Node> {
    nodes.iter().filter(|n| n.name == name).collect()
}

/// The values of an int or double array property.
fn array_len(node: &Node) -> usize {
    assert!(matches!(node.props[0], b'i' | b'd'));
    u32::from_le_bytes(node.props[1..5].try_into().unwrap()) as usize
}

#[test]
fn a_binary_fbx_holds_a_model_per_piece_and_the_avatar() {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let fabric = project.template.panels[0].fabric.clone();
    project.template.fabric_appearance.insert(
        fabric,
        FabricAppearance {
            color_srgb: [255, 0, 0],
            roughness: 0.5,
            texture: Some(FabricTexture {
                asset_id: "print".into(),
                width_mm: 100.,
                height_mm: 100.,
                rotation_degrees: 0.,
                relief_mm: 0.,
            }),
            ..Default::default()
        },
    );
    let scene = project.compile().unwrap();
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("print.png");
    std::fs::write(&image, b"pixels").unwrap();
    let images = [("print".to_string(), image)];
    let export = FbxScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: Some(&avatar),
    };
    let path = dir.path().join("out").join("tee.fbx");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    fbx_export::write(&export, &path).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("out/tee_print.png")).unwrap(),
        b"pixels"
    );
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..23], b"Kaydara FBX Binary  \x00\x1a\x00");
    assert_eq!(u32::from_le_bytes(bytes[23..27].try_into().unwrap()), 7400);
    let mut at = 27;
    let mut top = Vec::new();
    while let Some(n) = read(&bytes, &mut at) {
        top.push(n);
    }
    let names: Vec<&str> = top.iter().map(|n| n.name.as_str()).collect();
    for wanted in [
        "FBXHeaderExtension",
        "GlobalSettings",
        "Definitions",
        "Objects",
        "Connections",
    ] {
        assert!(names.contains(&wanted), "{names:?}");
    }
    // The footer ends with its closing magic.
    assert_eq!(
        &bytes[bytes.len() - 16..bytes.len() - 12],
        &[0xf8, 0x5a, 0x8c, 0x6a]
    );
    let objects = &find(&top, "Objects")[0].children;
    let pieces = scene.garment.panels.len();
    assert_eq!(find(objects, "Model").len(), pieces + 1);
    assert_eq!(find(objects, "Video").len(), 1);
    let geometries = find(objects, "Geometry");
    assert_eq!(geometries.len(), pieces + 1);
    // Each piece: its vertices, and one closing (negative) corner per triangle.
    for (geometry, panel) in geometries.iter().zip(&scene.garment.panels) {
        assert_eq!(geometry.count, 3);
        let vertices = find(&geometry.children, "Vertices")[0];
        assert_eq!(array_len(vertices), 3 * panel.vertices.len());
        let corners = find(&geometry.children, "PolygonVertexIndex")[0];
        let n = array_len(corners);
        assert_eq!(n, 3 * panel.triangles.len());
        let values: Vec<i32> = corners.props[13..13 + 4 * n]
            .chunks(4)
            .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(
            values.iter().filter(|v| **v < 0).count(),
            panel.triangles.len()
        );
        assert!(values.chunks(3).all(|t| t[0] >= 0 && t[1] >= 0 && t[2] < 0));
        assert_eq!(find(&geometry.children, "LayerElementUV").len(), 1);
    }
    // Everything hangs from the scene root or from its model.
    let connections = &find(&top, "Connections")[0].children;
    assert_eq!(
        connections.len(),
        3 * (pieces + 1) + 2,
        "models to root, geometry and material to model, video to texture to material"
    );
}
