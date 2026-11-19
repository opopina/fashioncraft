//! FBX import: binary FBX 7.x meshes placed by their models, in metres and Y up — from our own
//! FBX export and from a hand-made 7.5 file with compressed arrays, a metre unit and Z up.
use fashioncraft::{
    avatar::SurfaceMesh,
    fbx_export::{self, FbxScene},
    fbx_import,
    fitting::FittingProject,
    garment::presets::GarmentKind,
};
use glam::DVec3;
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

#[test]
fn our_own_export_reads_back_with_the_avatar_in_metres() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let export = FbxScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&avatar),
        take: None,
    };
    let (bytes, _) = fbx_export::fbx(&export, "tee").unwrap();
    let imported = fbx_import::fbx(&bytes).unwrap();
    // Every avatar vertex comes back where it was (the file is in centimetres).
    let key = |p: DVec3| (p * 1e6).round().to_array().map(|v| v as i64);
    let read: std::collections::HashSet<_> =
        imported.mesh.vertices.iter().map(|p| key(*p)).collect();
    let missing = avatar
        .vertices
        .iter()
        .filter(|p| !read.contains(&key(**p)))
        .count();
    assert_eq!(missing, 0, "{missing} avatar vertices moved");
    assert!(imported.mesh.triangles.len() >= avatar.triangles.len());
}

/// A minimal binary FBX 7.5 writer: 64-bit node records, arrays zlib-compressed.
mod write {
    pub enum P {
        I32(i32),
        I64(i64),
        F64(f64),
        S(&'static str),
        F64s(Vec<f64>),
        I32s(Vec<i32>),
    }
    pub struct N {
        pub name: &'static str,
        pub props: Vec<P>,
        pub children: Vec<N>,
    }
    fn prop(p: &P, out: &mut Vec<u8>) {
        let array = |out: &mut Vec<u8>, kind: u8, count: usize, raw: Vec<u8>| {
            let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
            out.push(kind);
            out.extend((count as u32).to_le_bytes());
            out.extend(1u32.to_le_bytes());
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend(packed);
        };
        match p {
            P::I32(v) => {
                out.push(b'I');
                out.extend(v.to_le_bytes());
            }
            P::I64(v) => {
                out.push(b'L');
                out.extend(v.to_le_bytes());
            }
            P::F64(v) => {
                out.push(b'D');
                out.extend(v.to_le_bytes());
            }
            P::S(s) => {
                out.push(b'S');
                out.extend((s.len() as u32).to_le_bytes());
                out.extend(s.as_bytes());
            }
            P::F64s(v) => array(
                out,
                b'd',
                v.len(),
                v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ),
            P::I32s(v) => array(
                out,
                b'i',
                v.len(),
                v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ),
        }
    }
    fn node(n: &N, out: &mut Vec<u8>) {
        let start = out.len();
        out.extend([0u8; 24]);
        out.push(n.name.len() as u8);
        out.extend(n.name.as_bytes());
        let props_start = out.len();
        for p in &n.props {
            prop(p, out);
        }
        let props_len = out.len() - props_start;
        if !n.children.is_empty() {
            for c in &n.children {
                node(c, out);
            }
            out.extend([0u8; 25]);
        }
        let end = out.len() as u64;
        out[start..start + 8].copy_from_slice(&end.to_le_bytes());
        out[start + 8..start + 16].copy_from_slice(&(n.props.len() as u64).to_le_bytes());
        out[start + 16..start + 24].copy_from_slice(&(props_len as u64).to_le_bytes());
    }
    pub fn file(nodes: &[N]) -> Vec<u8> {
        let mut out = b"Kaydara FBX Binary  \0\x1a\0".to_vec();
        out.extend(7500u32.to_le_bytes());
        for n in nodes {
            node(n, &mut out);
        }
        out.extend([0u8; 25]);
        out
    }
    pub fn n(name: &'static str, props: Vec<P>, children: Vec<N>) -> N {
        N {
            name,
            props,
            children,
        }
    }
    /// A `P` property entry with its four labels.
    pub fn p(name: &'static str, values: Vec<P>) -> N {
        let mut props = vec![P::S(name), P::S(""), P::S(""), P::S("")];
        props.extend(values);
        n("P", props, vec![])
    }
}

/// A unit quad in the XY plane under a model moved by `translation` and scaled ×2, in a file
/// whose unit is the metre and whose up axis is Z.
fn quad_file() -> Vec<u8> {
    use write::*;
    let settings = n(
        "GlobalSettings",
        vec![],
        vec![n(
            "Properties70",
            vec![],
            vec![
                p("UpAxis", vec![P::I32(2)]),
                p("UpAxisSign", vec![P::I32(1)]),
                p("UnitScaleFactor", vec![P::F64(100.)]),
            ],
        )],
    );
    let geometry = n(
        "Geometry",
        vec![P::I64(10), P::S("quad\0\x01Geometry"), P::S("Mesh")],
        vec![
            n(
                "Vertices",
                vec![P::F64s(vec![
                    0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0.,
                ])],
                vec![],
            ),
            n(
                "PolygonVertexIndex",
                vec![P::I32s(vec![0, 1, 2, !3])],
                vec![],
            ),
        ],
    );
    let model = n(
        "Model",
        vec![P::I64(20), P::S("quad\0\x01Model"), P::S("Mesh")],
        vec![n(
            "Properties70",
            vec![],
            vec![
                p("Lcl Translation", vec![P::F64(10.), P::F64(0.), P::F64(0.)]),
                p("Lcl Scaling", vec![P::F64(2.), P::F64(2.), P::F64(2.)]),
            ],
        )],
    );
    let objects = n("Objects", vec![], vec![geometry, model]);
    let connections = n(
        "Connections",
        vec![],
        vec![
            n("C", vec![P::S("OO"), P::I64(10), P::I64(20)], vec![]),
            n("C", vec![P::S("OO"), P::I64(20), P::I64(0)], vec![]),
        ],
    );
    file(&[settings, objects, connections])
}

#[test]
fn a_compressed_7_5_file_is_placed_scaled_and_turned_y_up() {
    let imported = fbx_import::fbx(&quad_file()).unwrap();
    assert_eq!(imported.mesh.triangles.len(), 2);
    // Scaled ×2 then moved 10 m along X; Z up becomes Y up, so the quad's +Y goes to -Z.
    let expected = [
        DVec3::new(10., 0., 0.),
        DVec3::new(12., 0., 0.),
        DVec3::new(12., 0., -2.),
        DVec3::new(10., 0., -2.),
    ];
    for (got, want) in imported.mesh.vertices.iter().zip(expected) {
        assert!(got.distance(want) < 1e-12, "{got} vs {want}");
    }
}

#[test]
fn broken_and_ascii_files_are_refused() {
    let good = quad_file();
    assert!(fbx_import::fbx(&good[..good.len() / 2]).is_err());
    assert!(fbx_import::fbx(b"; FBX 7.4.0 project file\n").is_err());
    assert!(fbx_import::fbx(b"not an fbx at all, just some bytes here").is_err());
    let mut bad_version = good.clone();
    bad_version[23..27].copy_from_slice(&6100u32.to_le_bytes());
    assert!(fbx_import::fbx(&bad_version).is_err());
}
