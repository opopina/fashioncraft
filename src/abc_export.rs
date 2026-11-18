//! Alembic export (Ogawa container): the garment as one polygon mesh with its texture
//! coordinates, its points sampled at every frame of a recorded take (or once, as it is now),
//! and the avatar beside it, moving with the take when its frames are given. Y up, metres.
//!
//! An Ogawa file is a 16-byte header and a tree of groups (child offsets) and data blocks
//! (sized bytes); a data child's offset has its top bit set. Alembic lays its objects and
//! properties out on that tree: an object is a group of its property compound, its child
//! objects and their headers; a compound is a group of its properties and their headers; an
//! array property is a group of (sample, dimensions) pairs, a scalar one of samples, each
//! sample prefixed by a 16-byte key. Metadata strings are kept in one indexed table.
use crate::{
    avatar::SurfaceMesh,
    avatar_animation::Take,
    garment::{CompiledGarment, GarmentTemplate},
    gltf_export::{front_triangles, texture_coordinates},
    model::State,
};
use glam::DVec3;
use std::path::Path;

/// What to write.
pub struct AbcScene<'a> {
    pub template: &'a GarmentTemplate,
    pub garment: &'a CompiledGarment,
    pub state: &'a State,
    pub avatar: Option<&'a SurfaceMesh>,
    /// A recorded take of the cloth, and the avatar's vertices at each of its frames.
    pub take: Option<(&'a Take, Option<Vec<Vec<DVec3>>>)>,
}

/// What was written.
#[derive(Debug, PartialEq)]
pub struct AbcSummary {
    pub meshes: usize,
    pub frames: usize,
}

const DATA: u64 = 1 << 63;
const OGAWA_FILE_VERSION: i32 = 0;
/// The Alembic library version the layout follows (1.8.3).
const LIBRARY_VERSION: i32 = 10803;

// Plain old data types.
const POD_U32: u32 = 5;
const POD_I32: u32 = 6;
const POD_F32: u32 = 10;
const POD_F64: u32 = 11;

/// MurmurHash3 (x64, 128 bits) of `bytes`: the key of a sample.
fn key(bytes: &[u8]) -> [u8; 16] {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;
    let fmix = |mut k: u64| {
        k ^= k >> 33;
        k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
        k ^= k >> 33;
        k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        k ^ (k >> 33)
    };
    let (mut h1, mut h2) = (0u64, 0u64);
    let blocks = bytes.len() / 16;
    for i in 0..blocks {
        let b = &bytes[i * 16..i * 16 + 16];
        let k1 = u64::from_le_bytes(b[..8].try_into().unwrap());
        let k2 = u64::from_le_bytes(b[8..].try_into().unwrap());
        h1 ^= k1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        h1 = h1
            .rotate_left(27)
            .wrapping_add(h2)
            .wrapping_mul(5)
            .wrapping_add(0x52dc_e729);
        h2 ^= k2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        h2 = h2
            .rotate_left(31)
            .wrapping_add(h1)
            .wrapping_mul(5)
            .wrapping_add(0x3849_5ab5);
    }
    let tail = &bytes[blocks * 16..];
    let (mut k1, mut k2) = (0u64, 0u64);
    for (i, b) in tail.iter().enumerate() {
        if i < 8 {
            k1 |= (*b as u64) << (8 * i);
        } else {
            k2 |= (*b as u64) << (8 * (i - 8));
        }
    }
    if tail.len() > 8 {
        h2 ^= k2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
    }
    if !tail.is_empty() {
        h1 ^= k1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
    }
    let len = bytes.len() as u64;
    h1 ^= len;
    h2 ^= len;
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    h1 = fmix(h1);
    h2 = fmix(h2);
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    let mut out = [0; 16];
    out[..8].copy_from_slice(&h1.to_le_bytes());
    out[8..].copy_from_slice(&h2.to_le_bytes());
    out
}

/// A property to write.
enum Property {
    Compound {
        name: String,
        meta: String,
        children: Vec<Property>,
    },
    /// Samples of `pod` values, `extent` per element: one written sample when `constant`.
    Values {
        name: String,
        meta: String,
        array: bool,
        pod: u32,
        extent: u32,
        samples: Vec<Vec<u8>>,
        /// How many elements each sample holds.
        counts: Vec<usize>,
        constant: bool,
        time_sampling: u32,
    },
}

fn bytes_f32(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

fn bytes_i32(values: impl IntoIterator<Item = i32>) -> Vec<u8> {
    values.into_iter().flat_map(i32::to_le_bytes).collect()
}

fn bytes_f64(values: impl IntoIterator<Item = f64>) -> Vec<u8> {
    values.into_iter().flat_map(f64::to_le_bytes).collect()
}

/// The Ogawa file being written, with Alembic's metadata table.
struct Writer {
    out: Vec<u8>,
    metadata: Vec<String>,
}

impl Writer {
    fn new() -> Self {
        let mut out = b"Ogawa".to_vec();
        // Not frozen yet; version 0.1; the root group's offset comes last.
        out.extend_from_slice(&[0, 0, 1]);
        out.extend_from_slice(&0u64.to_le_bytes());
        Self {
            out,
            metadata: vec![],
        }
    }

    /// A data block; its child entry.
    fn data(&mut self, bytes: &[u8]) -> u64 {
        if bytes.is_empty() {
            return DATA;
        }
        let at = self.out.len() as u64;
        self.out
            .extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.out.extend_from_slice(bytes);
        at | DATA
    }

    /// A group of `children`; its child entry.
    fn group(&mut self, children: &[u64]) -> u64 {
        if children.is_empty() {
            return 0;
        }
        let at = self.out.len() as u64;
        self.out
            .extend_from_slice(&(children.len() as u64).to_le_bytes());
        for c in children {
            self.out.extend_from_slice(&c.to_le_bytes());
        }
        at
    }

    /// The index of `meta` in the metadata table (0 is empty).
    fn meta(&mut self, meta: &str) -> u32 {
        if meta.is_empty() {
            return 0;
        }
        let k = match self.metadata.iter().position(|m| m == meta) {
            Some(k) => k,
            None => {
                self.metadata.push(meta.to_string());
                self.metadata.len() - 1
            }
        };
        k as u32 + 1
    }

    /// A sample: its key, then its bytes.
    fn sample(&mut self, bytes: &[u8]) -> u64 {
        let mut block = key(bytes).to_vec();
        block.extend_from_slice(bytes);
        self.data(&block)
    }

    /// Write `property`; its group entry and its header.
    fn property(&mut self, property: &Property) -> Result<(u64, Vec<u8>), String> {
        let push = |header: &mut Vec<u8>, value: usize, hint: u32| match hint {
            0 => header.push(value as u8),
            1 => header.extend_from_slice(&(value as u16).to_le_bytes()),
            _ => header.extend_from_slice(&(value as u32).to_le_bytes()),
        };
        let hint_for = |largest: usize| match largest {
            0..=255 => 0,
            256..=65535 => 1,
            _ => 2,
        };
        match property {
            Property::Compound {
                name,
                meta,
                children,
            } => {
                let entry = self.compound(children)?;
                let index = self.meta(meta);
                let hint = hint_for(name.len());
                let info: u32 = (hint << 2) | (index << 20);
                let mut header = info.to_le_bytes().to_vec();
                push(&mut header, name.len(), hint);
                header.extend_from_slice(name.as_bytes());
                Ok((entry, header))
            }
            Property::Values {
                name,
                meta,
                array,
                pod,
                extent,
                samples,
                counts,
                constant,
                time_sampling,
            } => {
                let written = if *constant {
                    &samples[..1]
                } else {
                    &samples[..]
                };
                let mut children = vec![];
                for s in written {
                    children.push(self.sample(s));
                    if *array {
                        // A one-dimensional array: its dimensions follow from its size.
                        children.push(DATA);
                    }
                }
                let entry = self.group(&children);
                let index = self.meta(meta);
                let hint = hint_for(name.len().max(samples.len()));
                let scalar_like = counts.iter().all(|c| *c == 1);
                let mut info: u32 = if *array { 2 } else { 1 };
                if *array && scalar_like {
                    info |= 1;
                }
                info |= hint << 2;
                info |= pod << 4;
                if *time_sampling != 0 {
                    info |= 0x100;
                }
                // Integer arrays and scalars keep one element count over their samples.
                if !*array || *pod == POD_I32 || *pod == POD_U32 {
                    info |= 0x400;
                }
                if *constant {
                    info |= 0x800;
                }
                info |= extent << 12;
                info |= index << 20;
                let mut header = info.to_le_bytes().to_vec();
                push(&mut header, samples.len(), hint);
                if *time_sampling != 0 {
                    push(&mut header, *time_sampling as usize, hint);
                }
                push(&mut header, name.len(), hint);
                header.extend_from_slice(name.as_bytes());
                Ok((entry, header))
            }
        }
    }

    /// Write a compound's properties; its group entry.
    fn compound(&mut self, properties: &[Property]) -> Result<u64, String> {
        let mut children = vec![];
        let mut headers = vec![];
        for p in properties {
            let (entry, header) = self.property(p)?;
            children.push(entry);
            headers.extend(header);
        }
        let headers = self.data(&headers);
        children.push(headers);
        Ok(self.group(&children))
    }

    /// Write an object: its properties, then its children (name, metadata, written object).
    fn object(
        &mut self,
        properties: &[Property],
        children: &[(String, String, u64)],
    ) -> Result<u64, String> {
        let props = self.compound(properties)?;
        let mut entries = vec![props];
        let mut headers = vec![];
        for (name, meta, entry) in children {
            entries.push(*entry);
            headers.extend_from_slice(&(name.len() as u32).to_le_bytes());
            headers.extend_from_slice(name.as_bytes());
            headers.push(self.meta(meta) as u8);
        }
        // Hashes of the properties and of the children: readers skip them.
        headers.extend_from_slice(&[0; 32]);
        let headers = self.data(&headers);
        entries.push(headers);
        Ok(self.group(&entries))
    }
}

/// An empty box: the bounds of nothing.
fn empty_box() -> Vec<u8> {
    bytes_f64([
        f64::MAX,
        f64::MAX,
        f64::MAX,
        -f64::MAX,
        -f64::MAX,
        -f64::MAX,
    ])
}

fn bounds(points: &[DVec3]) -> Vec<u8> {
    let (lo, hi) = points.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(-f64::MAX)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    bytes_f64([lo.x, lo.y, lo.z, hi.x, hi.y, hi.z])
}

const MESH_SCHEMA: &str = "schema=AbcGeom_PolyMesh_v1;schemaBaseType=AbcGeom_GeomBase_v1";

/// A polygon mesh object's properties: `frames` of its points (one when still), its faces
/// (corners counter-clockwise seen from the front) and, optionally, a texture coordinate per
/// point.
fn poly_mesh(
    frames: &[Vec<DVec3>],
    faces: &[[usize; 3]],
    uv: Option<&[glam::DVec2]>,
) -> Vec<Property> {
    let animated = frames.len() > 1;
    let n = frames.len();
    let points: Vec<Vec<u8>> = frames
        .iter()
        .map(|f| bytes_f32(f.iter().flat_map(|p| [p.x as f32, p.y as f32, p.z as f32])))
        .collect();
    let point_counts: Vec<usize> = frames.iter().map(Vec::len).collect();
    // Alembic winds faces clockwise.
    let corners: Vec<usize> = faces.iter().flat_map(|t| [t[2], t[1], t[0]]).collect();
    let mut geom = vec![
        Property::Values {
            name: ".selfBnds".into(),
            meta: "interpretation=box".into(),
            array: false,
            pod: POD_F64,
            extent: 6,
            samples: frames.iter().map(|f| bounds(f)).collect(),
            counts: vec![1; n],
            constant: !animated,
            time_sampling: 1,
        },
        Property::Values {
            name: "P".into(),
            meta: "geoScope=vtx;interpretation=point".into(),
            array: true,
            pod: POD_F32,
            extent: 3,
            samples: points,
            counts: point_counts,
            constant: !animated,
            time_sampling: 1,
        },
        Property::Values {
            name: ".faceIndices".into(),
            meta: String::new(),
            array: true,
            pod: POD_I32,
            extent: 1,
            samples: vec![bytes_i32(corners.iter().map(|c| *c as i32)); n],
            counts: vec![corners.len(); n],
            constant: true,
            time_sampling: 1,
        },
        Property::Values {
            name: ".faceCounts".into(),
            meta: String::new(),
            array: true,
            pod: POD_I32,
            extent: 1,
            samples: vec![bytes_i32(faces.iter().map(|_| 3)); n],
            counts: vec![faces.len(); n],
            constant: true,
            time_sampling: 1,
        },
    ];
    if let Some(uv) = uv {
        let meta = "arrayExtent=1;geoScope=fvr;interpretation=vector;isGeomParam=true;podExtent=2;podName=float32_t";
        geom.push(Property::Compound {
            name: "uv".into(),
            meta: meta.into(),
            children: vec![
                Property::Values {
                    name: ".vals".into(),
                    meta: meta.into(),
                    array: true,
                    pod: POD_F32,
                    extent: 2,
                    samples: vec![bytes_f32(uv.iter().flat_map(|q| [q.x as f32, q.y as f32])); n],
                    counts: vec![uv.len(); n],
                    constant: true,
                    time_sampling: 1,
                },
                Property::Values {
                    name: ".indices".into(),
                    meta: String::new(),
                    array: true,
                    pod: POD_U32,
                    extent: 1,
                    samples: vec![
                        corners
                            .iter()
                            .flat_map(|c| (*c as u32).to_le_bytes())
                            .collect();
                        n
                    ],
                    counts: vec![corners.len(); n],
                    constant: true,
                    time_sampling: 1,
                },
            ],
        });
    }
    vec![Property::Compound {
        name: ".geom".into(),
        meta: MESH_SCHEMA.into(),
        children: geom,
    }]
}

/// Write `path`: the garment as an animated mesh over the take's frames (or still), and the
/// avatar when given.
pub fn write(scene: &AbcScene, path: &Path) -> Result<AbcSummary, String> {
    let garment = scene.garment;
    let count = garment.mesh.uv.len();
    if scene.state.x.len() != count {
        return Err("The cloth does not match the garment".into());
    }
    let (fps, frames): (f64, Vec<Vec<DVec3>>) = match &scene.take {
        Some((take, _)) => {
            if take.frames.is_empty() || take.frames.iter().any(|f| f.len() != count) {
                return Err("The take was recorded for another garment".into());
            }
            (
                take.fps,
                (0..take.frames.len()).map(|k| take.positions(k)).collect(),
            )
        }
        None => (30., vec![scene.state.x.clone()]),
    };
    if !(fps.is_finite() && fps > 0.) {
        return Err("The take has no frame rate".into());
    }
    let triangles = front_triangles(scene.template, garment);
    let uv = texture_coordinates(scene.template, garment);
    let mut w = Writer::new();
    let mut children = vec![];
    let cloth = poly_mesh(&frames, &triangles, Some(&uv));
    let entry = w.object(&cloth, &[])?;
    let object_meta = format!("{MESH_SCHEMA};schemaObjTitle=AbcGeom_PolyMesh_v1:.geom");
    children.push(("garment".to_string(), object_meta.clone(), entry));
    if let Some(avatar) = scene.avatar {
        let body: Vec<Vec<DVec3>> = match &scene.take {
            Some((_, Some(bodies))) if bodies.len() == frames.len() => {
                if bodies.iter().any(|b| b.len() != avatar.vertices.len()) {
                    return Err("The avatar's frames do not match its mesh".into());
                }
                bodies.clone()
            }
            _ => vec![avatar.vertices.clone(); frames.len().min(1)],
        };
        let props = poly_mesh(&body, &avatar.triangles, None);
        let entry = w.object(&props, &[])?;
        children.push(("avatar".to_string(), object_meta, entry));
    }
    let meshes = children.len();
    let top_props = vec![Property::Values {
        name: ".childBnds".into(),
        meta: "interpretation=box".into(),
        array: false,
        pod: POD_F64,
        extent: 6,
        samples: vec![empty_box(); frames.len()],
        counts: vec![1; frames.len()],
        constant: true,
        time_sampling: 1,
    }];
    let top = w.object(&top_props, &children)?;
    // The archive: versions, the top object, its metadata, time samplings, metadata table.
    let version = w.data(&OGAWA_FILE_VERSION.to_le_bytes());
    let library = w.data(&LIBRARY_VERSION.to_le_bytes());
    let archive_meta = format!(
        "FramesPerTimeUnit={fps:.6};_ai_Application=ClothCraft;_ai_Description=ClothCraft garment"
    );
    let archive_meta = w.data(archive_meta.as_bytes());
    let mut samplings = vec![];
    // Index 0: the identity; index 1: the take's frames from time 0.
    for (max, step) in [(1u32, 1.), (frames.len() as u32, 1. / fps)] {
        samplings.extend_from_slice(&max.to_le_bytes());
        samplings.extend_from_slice(&f64::to_le_bytes(step));
        samplings.extend_from_slice(&1u32.to_le_bytes());
        samplings.extend_from_slice(&0f64.to_le_bytes());
    }
    let samplings = w.data(&samplings);
    let mut table = vec![];
    for m in &w.metadata {
        if m.len() > 255 {
            return Err("Metadata too long for the table".into());
        }
        table.push(m.len() as u8);
        table.extend_from_slice(m.as_bytes());
    }
    if w.metadata.len() > 254 {
        return Err("Too many metadata strings".into());
    }
    let table = w.data(&table);
    let root = w.group(&[version, library, top, archive_meta, samplings, table]);
    w.out[8..16].copy_from_slice(&root.to_le_bytes());
    // Frozen: the file is complete.
    w.out[5] = 0xff;
    std::fs::write(path, &w.out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(AbcSummary {
        meshes,
        frames: frames.len(),
    })
}
