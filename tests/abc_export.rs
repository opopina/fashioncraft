//! Alembic export: an Ogawa file whose tree holds the garment as a polygon mesh with its
//! points at every frame of a take, its faces wound clockwise, its texture coordinates, and
//! the avatar; read back here with a small Ogawa reader.
use glam::DVec3;
use marvelous_cloth::{
    abc_export::{self, AbcScene},
    avatar::SurfaceMesh,
    avatar_animation::Take,
    fitting::FittingProject,
    garment::presets::GarmentKind,
    gltf_export::front_triangles,
};
use std::path::PathBuf;

const DATA: u64 = 1 << 63;

struct File(Vec<u8>);

impl File {
    fn u64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.0[at..at + 8].try_into().unwrap())
    }
    fn group(&self, entry: u64) -> Vec<u64> {
        assert_eq!(entry & DATA, 0, "a group");
        if entry == 0 {
            return vec![];
        }
        let n = self.u64(entry as usize) as usize;
        (0..n)
            .map(|k| self.u64(entry as usize + 8 + 8 * k))
            .collect()
    }
    fn data(&self, entry: u64) -> &[u8] {
        assert_ne!(entry & DATA, 0, "data");
        let at = (entry & !DATA) as usize;
        if at == 0 {
            return &[];
        }
        let n = self.u64(at) as usize;
        &self.0[at + 8..at + 8 + n]
    }
}

#[derive(Debug)]
struct Header {
    info: u32,
    samples: usize,
    time_sampling: usize,
    name: String,
}

/// A compound's property headers (one-byte fields: names and counts below 256).
fn headers(bytes: &[u8]) -> Vec<Header> {
    let mut out = vec![];
    let mut at = 0;
    while at < bytes.len() {
        let info = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        at += 4;
        assert_eq!((info >> 2) & 3, 0);
        let (mut samples, mut time_sampling) = (0, 0);
        if info & 3 != 0 {
            samples = bytes[at] as usize;
            at += 1;
            assert_eq!(info & 0x200, 0);
            if info & 0x100 != 0 {
                time_sampling = bytes[at] as usize;
                at += 1;
            }
        }
        let n = bytes[at] as usize;
        let name = String::from_utf8(bytes[at + 1..at + 1 + n].to_vec()).unwrap();
        at += 1 + n;
        assert_ne!((info >> 20) & 0xff, 0xff, "metadata from the table");
        out.push(Header {
            info,
            samples,
            time_sampling,
            name,
        });
    }
    out
}

/// An object's children: (name, metadata index, entry).
fn children(file: &File, object: u64) -> Vec<(String, usize, u64)> {
    let entries = file.group(object);
    let bytes = file.data(*entries.last().unwrap());
    let mut out = vec![];
    let mut at = 0;
    let mut k = 1;
    while at < bytes.len() - 32 {
        let n = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let name = String::from_utf8(bytes[at + 4..at + 4 + n].to_vec()).unwrap();
        at += 4 + n;
        out.push((name, bytes[at] as usize, entries[k]));
        at += 1;
        k += 1;
    }
    assert_eq!(k, entries.len() - 1);
    out
}

/// A compound's properties by name: header and entry.
fn properties(file: &File, compound: u64) -> Vec<(Header, u64)> {
    let entries = file.group(compound);
    headers(file.data(*entries.last().unwrap()))
        .into_iter()
        .zip(entries)
        .collect()
}

fn find<'a>(list: &'a [(Header, u64)], name: &str) -> &'a (Header, u64) {
    list.iter().find(|(h, _)| h.name == name).unwrap()
}

/// The samples written for an array property: their bytes after the 16-byte key.
fn array_samples(file: &File, entry: u64) -> Vec<Vec<u8>> {
    let kids = file.group(entry);
    assert_eq!(kids.len() % 2, 0);
    kids.chunks(2)
        .map(|pair| {
            assert_eq!(pair[1], DATA, "one-dimensional");
            file.data(pair[0])[16..].to_vec()
        })
        .collect()
}

fn f32s(bytes: &[u8]) -> Vec<f32> {
    let (chunks, _) = bytes.as_chunks::<4>();
    chunks.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

fn i32s(bytes: &[u8]) -> Vec<i32> {
    let (chunks, _) = bytes.as_chunks::<4>();
    chunks.iter().map(|b| i32::from_le_bytes(*b)).collect()
}

#[test]
fn a_take_is_written_as_an_animated_alembic_mesh_with_the_avatar() {
    let mannequin =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin.clone());
    let scene = project.compile().unwrap();
    let x = &scene.state.x;
    // Four frames: the cloth rises 1 cm a frame; the avatar steps back.
    let frames: Vec<Vec<[f32; 3]>> = (0..4)
        .map(|k| {
            x.iter()
                .map(|p| [p.x as f32, (p.y + 0.01 * k as f64) as f32, p.z as f32])
                .collect()
        })
        .collect();
    let take = Take { fps: 24., frames };
    let avatar = SurfaceMesh::load_obj(&mannequin).unwrap();
    let bodies: Vec<Vec<DVec3>> = (0..4)
        .map(|k| {
            avatar
                .vertices
                .iter()
                .map(|p| *p - DVec3::Z * 0.01 * k as f64)
                .collect()
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("take.abc");
    let export = AbcScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        avatar: Some(&avatar),
        take: Some((&take, Some(bodies.clone()))),
    };
    let summary = abc_export::write(&export, &path).unwrap();
    assert_eq!((summary.meshes, summary.frames), (2, 4));
    let file = File(std::fs::read(&path).unwrap());
    // The container: magic, frozen, version 0.1, the root group.
    assert_eq!(&file.0[..5], b"Ogawa");
    assert_eq!(file.0[5], 0xff);
    assert_eq!(&file.0[6..8], &[0, 1]);
    let root = file.group(file.u64(8));
    assert_eq!(root.len(), 6);
    assert!(root[2] & DATA == 0 && [0, 1, 3, 4, 5].iter().all(|k| root[*k] & DATA != 0));
    assert!(i32::from_le_bytes(file.data(root[1]).try_into().unwrap()) >= 9999);
    let archive = String::from_utf8(file.data(root[3]).to_vec()).unwrap();
    assert!(archive.contains("FramesPerTimeUnit=24.000000"));
    // Time samplings: the identity, then the take's frames.
    let ts = file.data(root[4]);
    let step = f64::from_le_bytes(ts[28..36].try_into().unwrap());
    assert_eq!(u32::from_le_bytes(ts[24..28].try_into().unwrap()), 4);
    assert!((step - 1. / 24.).abs() < 1e-15);
    // The metadata table: strings after their lengths.
    let mut table = vec![String::new()];
    let bytes = file.data(root[5]);
    let mut at = 0;
    while at < bytes.len() {
        let n = bytes[at] as usize;
        table.push(String::from_utf8(bytes[at + 1..at + 1 + n].to_vec()).unwrap());
        at += 1 + n;
    }
    // Two polygon meshes under the top object.
    let meshes = children(&file, root[2]);
    let names: Vec<&str> = meshes.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(names, ["garment", "avatar"]);
    for (_, meta, _) in &meshes {
        assert!(table[*meta].contains("schemaObjTitle=AbcGeom_PolyMesh_v1:.geom"));
    }
    let props = |object: u64| properties(&file, file.group(object)[0]);
    let geom = |object: u64| {
        let list = props(object);
        let (header, entry) = find(&list, ".geom");
        assert_eq!(header.info & 3, 0);
        properties(&file, *entry)
    };
    let garment = geom(meshes[0].2);
    // Points: one sample per frame, as recorded.
    let (p, entry) = find(&garment, "P");
    assert_eq!((p.samples, p.time_sampling), (4, 1));
    assert_eq!(
        (p.info & 3, (p.info >> 4) & 0xf, (p.info >> 12) & 0xff),
        (2, 10, 3)
    );
    assert_eq!(p.info & 0x800, 0);
    let samples = array_samples(&file, *entry);
    assert_eq!(samples.len(), 4);
    for (k, s) in samples.iter().enumerate() {
        let flat: Vec<f32> = take.frames[k].iter().flatten().copied().collect();
        assert_eq!(f32s(s), flat);
    }
    // Different frames, different keys.
    let keys: std::collections::BTreeSet<&[u8]> = file
        .group(*entry)
        .iter()
        .step_by(2)
        .map(|e| &file.data(*e)[..16])
        .collect();
    assert_eq!(keys.len(), 4);
    // Faces: constant, every one a triangle wound clockwise seen from the front.
    let triangles = front_triangles(&project.template, &scene.garment);
    let (h, entry) = find(&garment, ".faceIndices");
    assert_ne!(h.info & 0x800, 0);
    let corners = i32s(&array_samples(&file, *entry)[0]);
    let expected: Vec<i32> = triangles
        .iter()
        .flat_map(|t| [t[2], t[1], t[0]].map(|v| v as i32))
        .collect();
    assert_eq!(corners, expected);
    let (_, entry) = find(&garment, ".faceCounts");
    let counts = i32s(&array_samples(&file, *entry)[0]);
    assert!(counts.len() == triangles.len() && counts.iter().all(|c| *c == 3));
    // Texture coordinates per corner, face-varying.
    let (uv, entry) = find(&garment, "uv");
    assert!(table[((uv.info >> 20) & 0xff) as usize].contains("geoScope=fvr"));
    let uv_props = properties(&file, *entry);
    let (_, vals) = find(&uv_props, ".vals");
    assert_eq!(f32s(&array_samples(&file, *vals)[0]).len(), 2 * x.len());
    let (_, indices) = find(&uv_props, ".indices");
    assert_eq!(i32s(&array_samples(&file, *indices)[0]), expected);
    // Bounds: scalar boxes per frame.
    let (b, entry) = find(&garment, ".selfBnds");
    assert_eq!((b.info & 3, b.samples), (1, 4));
    let first = &file.data(file.group(*entry)[0])[16..];
    let low_y = f64::from_le_bytes(first[8..16].try_into().unwrap());
    // Bounds of the recorded (single-precision) points.
    let min_y = take.frames[0]
        .iter()
        .map(|p| p[1] as f64)
        .fold(f64::INFINITY, f64::min);
    assert!((low_y - min_y).abs() < 1e-12);
    // The avatar moves with the take.
    let body = geom(meshes[1].2);
    let (p, entry) = find(&body, "P");
    assert_eq!(p.samples, 4);
    let last = f32s(&array_samples(&file, *entry)[3]);
    assert!((last[2] as f64 - bodies[3][0].z).abs() < 1e-6);
    // The top object's bounds key matches the Alembic library's for an empty box.
    let top = props(root[2]);
    let (_, entry) = find(&top, ".childBnds");
    let key = &file.data(file.group(*entry)[0])[..16];
    assert_eq!(
        key,
        [
            0x31, 0x04, 0x88, 0xee, 0x1c, 0x82, 0xf5, 0x93, 0xc7, 0xce, 0xfe, 0xb3, 0x87, 0x58,
            0x3d, 0xbb
        ]
    );
}

#[test]
fn without_a_take_the_garment_is_written_still_and_bad_takes_are_refused() {
    let mannequin =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin);
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("still.abc");
    let mut export = AbcScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        avatar: None,
        take: None,
    };
    let summary = abc_export::write(&export, &path).unwrap();
    assert_eq!((summary.meshes, summary.frames), (1, 1));
    let file = File(std::fs::read(&path).unwrap());
    let root = file.group(file.u64(8));
    let mesh = children(&file, root[2])[0].2;
    let props = properties(&file, file.group(mesh)[0]);
    let geom = properties(&file, find(&props, ".geom").1);
    let (p, entry) = find(&geom, "P");
    assert_eq!(p.samples, 1);
    assert_ne!(p.info & 0x800, 0);
    let points = f32s(&array_samples(&file, *entry)[0]);
    assert_eq!(points[1], scene.state.x[0].y as f32);
    // A take of another garment.
    let take = Take {
        fps: 30.,
        frames: vec![vec![[0.; 3]; 3]],
    };
    export.take = Some((&take, None));
    assert!(abc_export::write(&export, &path).is_err());
}
