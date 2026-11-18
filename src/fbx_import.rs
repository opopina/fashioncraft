//! FBX import (binary, versions 7.x) of static meshes, for avatars and props.
//!
//! Read from publicly documented descriptions of the binary FBX layout: a 27-byte header with
//! the version, then nested node records (end offset, property count, property bytes, name,
//! typed properties, children), with 64-bit offsets from version 7500 on. Array properties
//! may be zlib-compressed. Every `Geometry` mesh is read (its `Vertices` and
//! `PolygonVertexIndex`, polygons fanned into triangles) and placed by the models it hangs
//! from, through the file's `Connections`: each model's translation, pre-rotation, rotation
//! (XYZ order) and scaling. Skinned meshes are read in their bind pose. The file's unit
//! (`UnitScaleFactor`, centimetres per unit) becomes metres, and a Z-up file is turned Y up.
use crate::{avatar::SurfaceMesh, mesh_import::Imported};
use glam::{DMat4, DQuat, DVec3, EulerRot};
use std::collections::HashMap;

/// At most this many nodes, so a hostile file cannot exhaust memory through tiny records.
const MAX_NODES: usize = 2_000_000;
/// Nested node lists deeper than this are refused.
const MAX_DEPTH: usize = 64;

/// A typed node property.
#[derive(Clone, Debug)]
enum Prop {
    Int(i64),
    Float(f64),
    Str(String),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Other,
}

impl Prop {
    fn int(&self) -> Option<i64> {
        match self {
            Prop::Int(v) => Some(*v),
            Prop::Float(v) => Some(*v as i64),
            _ => None,
        }
    }
    fn float(&self) -> Option<f64> {
        match self {
            Prop::Int(v) => Some(*v as f64),
            Prop::Float(v) => Some(*v),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Node {
    name: String,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// A `P` entry of the node's `Properties70` by name: its values after the four labels.
    fn property(&self, name: &str) -> Option<&[Prop]> {
        self.child("Properties70")?
            .children("P")
            .find(|p| p.props.first().and_then(Prop::str) == Some(name))
            .map(|p| p.props.get(4..).unwrap_or(&[]))
    }
    fn vector(&self, name: &str) -> Option<DVec3> {
        let v = self.property(name)?;
        Some(DVec3::new(
            v.first()?.float()?,
            v.get(1)?.float()?,
            v.get(2)?.float()?,
        ))
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    wide: bool,
    nodes: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("The FBX file ends early")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn offset(&mut self) -> Result<u64, String> {
        if self.wide {
            self.u64()
        } else {
            self.u32().map(u64::from)
        }
    }

    /// An array property: its element count, encoding and bytes, inflated when compressed.
    fn array(&mut self, size: usize) -> Result<(usize, Vec<u8>), String> {
        let count = self.u32()? as usize;
        let encoding = self.u32()?;
        let length = self.u32()? as usize;
        let data = self.take(length)?;
        let expected = count
            .checked_mul(size)
            .filter(|n| *n <= 1 << 30)
            .ok_or("An FBX array is too large")?;
        let raw = match encoding {
            0 => data.to_vec(),
            1 => miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, expected)
                .map_err(|e| format!("A compressed FBX array cannot be read: {e:?}"))?,
            _ => return Err(format!("Unknown FBX array encoding {encoding}")),
        };
        if raw.len() != expected {
            return Err("An FBX array has the wrong length".into());
        }
        Ok((count, raw))
    }

    fn prop(&mut self) -> Result<Prop, String> {
        let kind = self.u8()?;
        Ok(match kind {
            b'Y' => Prop::Int(i16::from_le_bytes(self.take(2)?.try_into().unwrap()).into()),
            b'C' => Prop::Int(self.u8()?.into()),
            b'I' => Prop::Int(i32::from_le_bytes(self.take(4)?.try_into().unwrap()).into()),
            b'L' => Prop::Int(i64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            b'F' => Prop::Float(f32::from_le_bytes(self.take(4)?.try_into().unwrap()).into()),
            b'D' => Prop::Float(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            b'S' | b'R' => {
                let n = self.u32()? as usize;
                let raw = self.take(n)?;
                if kind == b'S' {
                    Prop::Str(String::from_utf8_lossy(raw).into_owned())
                } else {
                    Prop::Other
                }
            }
            b'f' => {
                let (_, raw) = self.array(4)?;
                Prop::Floats(
                    raw.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| f32::from_le_bytes(*c).into())
                        .collect(),
                )
            }
            b'd' => {
                let (_, raw) = self.array(8)?;
                Prop::Floats(
                    raw.as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| f64::from_le_bytes(*c))
                        .collect(),
                )
            }
            b'i' => {
                let (_, raw) = self.array(4)?;
                Prop::Ints(
                    raw.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| i32::from_le_bytes(*c).into())
                        .collect(),
                )
            }
            b'l' => {
                let (_, raw) = self.array(8)?;
                Prop::Ints(
                    raw.as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| i64::from_le_bytes(*c))
                        .collect(),
                )
            }
            b'b' | b'c' => {
                let (count, raw) = self.array(1)?;
                Prop::Ints(raw.iter().take(count).map(|b| i64::from(*b)).collect())
            }
            _ => return Err(format!("Unknown FBX property type {:?}", kind as char)),
        })
    }

    /// One node record, or `None` at the null record that ends a list.
    fn node(&mut self, depth: usize) -> Result<Option<Node>, String> {
        if depth > MAX_DEPTH {
            return Err("FBX nodes are nested too deeply".into());
        }
        let end = self.offset()? as usize;
        let count = self.offset()? as usize;
        let _list_length = self.offset()?;
        let name_length = self.u8()? as usize;
        if end == 0 {
            return Ok(None);
        }
        if end > self.bytes.len() || end <= self.at {
            return Err("An FBX node points outside the file".into());
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err("The FBX file has too many nodes".into());
        }
        let name = String::from_utf8_lossy(self.take(name_length)?).into_owned();
        let mut props = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            props.push(self.prop()?);
        }
        let mut children = vec![];
        while self.at < end {
            match self.node(depth + 1)? {
                Some(child) => children.push(child),
                None => break,
            }
        }
        self.at = end;
        Ok(Some(Node {
            name,
            props,
            children,
        }))
    }
}

/// The top-level nodes of a binary FBX file.
fn parse(bytes: &[u8]) -> Result<Vec<Node>, String> {
    if bytes.starts_with(b"; FBX") {
        return Err("This is an ASCII FBX file; export it as binary FBX".into());
    }
    if bytes.len() < 27 || &bytes[..21] != b"Kaydara FBX Binary  \0" {
        return Err("Not a binary FBX file".into());
    }
    let version = u32::from_le_bytes(bytes[23..27].try_into().unwrap());
    if !(7000..8000).contains(&version) {
        return Err(format!("FBX version {version} is not supported (7.x only)"));
    }
    let mut reader = Reader {
        bytes,
        at: 27,
        wide: version >= 7500,
        nodes: 0,
    };
    let mut nodes = vec![];
    // The footer follows the null record that closes the top-level list.
    while reader.at < bytes.len() {
        match reader.node(0)? {
            Some(node) => nodes.push(node),
            None => break,
        }
    }
    Ok(nodes)
}

/// A model's own transform: translation · pre-rotation · rotation · scaling.
fn local_transform(model: &Node) -> DMat4 {
    let euler = |v: DVec3| {
        let r = v * std::f64::consts::PI / 180.;
        DQuat::from_euler(EulerRot::ZYX, r.z, r.y, r.x)
    };
    let translation = model.vector("Lcl Translation").unwrap_or(DVec3::ZERO);
    let pre = model.vector("PreRotation").unwrap_or(DVec3::ZERO);
    let rotation = model.vector("Lcl Rotation").unwrap_or(DVec3::ZERO);
    let scaling = model.vector("Lcl Scaling").unwrap_or(DVec3::ONE);
    DMat4::from_translation(translation)
        * DMat4::from_quat(euler(pre))
        * DMat4::from_quat(euler(rotation))
        * DMat4::from_scale(scaling)
}

/// Every mesh of a binary FBX file as one triangle mesh in metres, Y up.
pub fn fbx(bytes: &[u8]) -> Result<Imported, String> {
    let nodes = parse(bytes)?;
    let top = |name: &str| nodes.iter().find(|n| n.name == name);
    let objects = top("Objects").ok_or("The FBX file has no Objects")?;
    let mut warnings = vec![];
    // Unit and axes.
    let settings = top("GlobalSettings");
    let unit_cm = settings
        .and_then(|s| s.property("UnitScaleFactor"))
        .and_then(|v| v.first()?.float())
        .filter(|v| v.is_finite() && *v > 0.)
        .unwrap_or(1.);
    let up = settings
        .and_then(|s| s.property("UpAxis"))
        .and_then(|v| v.first()?.int())
        .unwrap_or(1);
    let up_sign = settings
        .and_then(|s| s.property("UpAxisSign"))
        .and_then(|v| v.first()?.int())
        .unwrap_or(1);
    let axes = match (up, up_sign) {
        (1, 1) => DMat4::IDENTITY,
        // Z up: turn it to Y up.
        (2, 1) => DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2),
        _ => {
            warnings.push(format!(
                "The FBX file declares an unusual up axis ({up}, sign {up_sign}); it is read as Y up."
            ));
            DMat4::IDENTITY
        }
    };
    let to_metres = DMat4::from_scale(DVec3::splat(unit_cm / 100.)) * axes;
    // Objects by id, and the parent of each object.
    let mut models: HashMap<i64, &Node> = HashMap::new();
    let mut geometries: Vec<(i64, &Node)> = vec![];
    for object in &objects.children {
        let Some(id) = object.props.first().and_then(Prop::int) else {
            continue;
        };
        match object.name.as_str() {
            "Model" => {
                models.insert(id, object);
            }
            "Geometry" if object.props.get(2).and_then(Prop::str) == Some("Mesh") => {
                geometries.push((id, object));
            }
            _ => {}
        }
    }
    let mut parents: HashMap<i64, Vec<i64>> = HashMap::new();
    if let Some(connections) = top("Connections") {
        for c in connections.children("C") {
            if c.props.first().and_then(Prop::str) != Some("OO") {
                continue;
            }
            if let (Some(child), Some(parent)) = (
                c.props.get(1).and_then(Prop::int),
                c.props.get(2).and_then(Prop::int),
            ) {
                parents.entry(child).or_default().push(parent);
            }
        }
    }
    // A model's transform in the scene, walking up its parents.
    let global = |mut id: i64| {
        let mut m = DMat4::IDENTITY;
        for _ in 0..MAX_DEPTH {
            let Some(model) = models.get(&id) else {
                break;
            };
            m = local_transform(model) * m;
            match parents
                .get(&id)
                .and_then(|p| p.iter().find(|p| models.contains_key(p)))
            {
                Some(parent) => id = *parent,
                None => break,
            }
        }
        m
    };
    let mut mesh = SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    };
    let mut skipped = 0;
    for (id, geometry) in &geometries {
        let (Some(Prop::Floats(coords)), Some(Prop::Ints(polygons))) = (
            geometry.child("Vertices").and_then(|v| v.props.first()),
            geometry
                .child("PolygonVertexIndex")
                .and_then(|v| v.props.first()),
        ) else {
            skipped += 1;
            continue;
        };
        // A geometry used by several models appears once per model.
        let owners: Vec<i64> = parents
            .get(id)
            .map(|p| {
                p.iter()
                    .copied()
                    .filter(|p| models.contains_key(p))
                    .collect()
            })
            .unwrap_or_default();
        let placements: Vec<DMat4> = if owners.is_empty() {
            vec![DMat4::IDENTITY]
        } else {
            owners.iter().map(|o| global(*o)).collect()
        };
        let count = coords.len() / 3;
        for placement in placements {
            let m = to_metres * placement;
            let base = mesh.vertices.len();
            mesh.vertices.extend(
                coords
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| m.transform_point3(DVec3::new(c[0], c[1], c[2]))),
            );
            let mut polygon: Vec<usize> = vec![];
            for &raw in polygons {
                // A negative index closes its polygon: the vertex is its bitwise complement.
                let (index, last) = if raw < 0 { (!raw, true) } else { (raw, false) };
                let index = usize::try_from(index)
                    .ok()
                    .filter(|i| *i < count)
                    .ok_or("An FBX polygon refers to a missing vertex")?;
                polygon.push(base + index);
                if last {
                    for k in 1..polygon.len().saturating_sub(1) {
                        mesh.triangles
                            .push([polygon[0], polygon[k], polygon[k + 1]]);
                    }
                    polygon.clear();
                }
            }
        }
    }
    if skipped > 0 {
        warnings.push(format!(
            "{skipped} FBX geometries without vertices or polygons were left out."
        ));
    }
    if mesh.triangles.is_empty() {
        return Err("The FBX file has no mesh".into());
    }
    warnings.push(
        "Geometry only: materials, textures, skinning and animation are not read; skinned meshes are in their bind pose."
            .into(),
    );
    crate::mesh_import::finish(mesh, 0, warnings)
}
