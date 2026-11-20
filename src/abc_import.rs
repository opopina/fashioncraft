//! Alembic mesh import (Ogawa container), for avatars and props: every polygon mesh in its
//! first sample, placed by the transforms of the objects above it, polygons fanned into
//! triangles. Alembic winds faces clockwise; they are turned to counter-clockwise. Alembic
//! declares no unit: like STL and PLY, the size of the whole decides between metres,
//! centimetres and millimetres. Y is up.
//!
//! The layout read is the one `abc_export` writes and Alembic's own Ogawa writer uses: a tree
//! of groups and data blocks; an object is a group of its property compound, its children and
//! their headers; a compound is a group of its properties and their packed headers; a sample is
//! a data block of a 16-byte key and the values.
use crate::{avatar::SurfaceMesh, mesh_import::Imported};
use glam::{DMat4, DVec3};

const DATA: u64 = 1 << 63;
/// Deepest object nesting followed.
const MAX_DEPTH: usize = 64;
/// Largest number of values one sample may hold.
const MAX_VALUES: usize = 30_000_000;

struct Ogawa<'a> {
    bytes: &'a [u8],
}

impl<'a> Ogawa<'a> {
    fn u64_at(&self, at: usize) -> Result<u64, String> {
        self.bytes
            .get(at..at + 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
            .ok_or_else(|| "The Alembic file is cut short".to_string())
    }
    /// The child entries of the group at `entry`.
    fn group(&self, entry: u64) -> Result<Vec<u64>, String> {
        if entry & DATA != 0 {
            return Err("Expected an Alembic group, found data".into());
        }
        if entry == 0 {
            return Ok(vec![]);
        }
        let at = entry as usize;
        let count = self.u64_at(at)? as usize;
        if count > self.bytes.len() / 8 {
            return Err("An Alembic group is larger than the file".into());
        }
        (0..count).map(|k| self.u64_at(at + 8 + 8 * k)).collect()
    }
    /// The bytes of the data block at `entry`.
    fn data(&self, entry: u64) -> Result<&'a [u8], String> {
        if entry & DATA == 0 {
            return Err("Expected Alembic data, found a group".into());
        }
        let at = (entry & !DATA) as usize;
        if at == 0 {
            return Ok(&[]);
        }
        let size = self.u64_at(at)? as usize;
        self.bytes
            .get(at + 8..(at + 8).saturating_add(size))
            .ok_or_else(|| "An Alembic data block runs past the file".to_string())
    }
}

/// A property header.
struct Header {
    name: String,
    /// 0 compound, 1 scalar, 2 array.
    kind: u32,
    pod: u32,
}

/// Size of one value of each plain old data type, bytes.
fn pod_size(pod: u32) -> Option<usize> {
    Some(match pod {
        0..=2 => 1,
        3 | 4 | 9 => 2,
        5 | 6 | 10 => 4,
        7 | 8 | 11 => 8,
        _ => return None,
    })
}

struct Reader<'a> {
    ogawa: Ogawa<'a>,
    metadata: Vec<String>,
}

/// Bytes read one after another.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let out = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or("An Alembic header is cut short")?;
        self.at += n;
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    /// A size written in 1, 2 or 4 bytes.
    fn sized(&mut self, hint: u32) -> Result<usize, String> {
        Ok(match hint {
            0 => self.take(1)?[0] as usize,
            1 => u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize,
            _ => self.u32()? as usize,
        })
    }
    fn text(&mut self, n: usize) -> Result<String, String> {
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

impl Reader<'_> {
    fn meta(&self, index: usize) -> String {
        if index == 0 {
            String::new()
        } else {
            self.metadata.get(index - 1).cloned().unwrap_or_default()
        }
    }

    /// The properties of the compound at `entry`: their headers and group entries.
    fn compound(&self, entry: u64) -> Result<Vec<(Header, u64)>, String> {
        let children = self.ogawa.group(entry)?;
        let Some((&headers, entries)) = children.split_last() else {
            return Ok(vec![]);
        };
        let mut c = Cursor {
            bytes: self.ogawa.data(headers)?,
            at: 0,
        };
        let mut out = vec![];
        for &entry in entries {
            let info = c.u32()?;
            let kind = match info & 3 {
                0 => 0,
                1 => 1,
                _ => 2,
            };
            let hint = (info >> 2) & 3;
            let meta = (info >> 20) & 0xff;
            let mut pod = 0;
            if kind != 0 {
                pod = (info >> 4) & 0xf;
                c.sized(hint)?; // samples
                if info & 0x200 != 0 {
                    c.sized(hint)?; // first changed
                    c.sized(hint)?; // last changed
                }
                if info & 0x100 != 0 {
                    c.sized(hint)?; // time sampling
                }
            }
            let n = c.sized(hint)?;
            let name = c.text(n)?;
            if meta == 0xff {
                let n = c.sized(hint)?;
                c.take(n)?;
            }
            out.push((Header { name, kind, pod }, entry));
        }
        Ok(out)
    }

    /// The values of the first sample of the scalar or array property at `entry`, as f64.
    fn first_sample(&self, header: &Header, entry: u64) -> Result<Vec<f64>, String> {
        let samples = self.ogawa.group(entry)?;
        let Some(&first) = samples.first() else {
            return Ok(vec![]);
        };
        let block = self.ogawa.data(first)?;
        // Every sample starts with its 16-byte key.
        let values = block.get(16..).unwrap_or(&[]);
        let size = pod_size(header.pod).ok_or("Unsupported Alembic value type")?;
        if values.len() % size != 0 || values.len() / size > MAX_VALUES {
            return Err(format!(
                "Alembic property {} has a broken sample",
                header.name
            ));
        }
        Ok(values
            .chunks_exact(size)
            .map(|b| match header.pod {
                0 | 1 => b[0] as f64,
                2 => b[0] as i8 as f64,
                3 => u16::from_le_bytes(b.try_into().unwrap()) as f64,
                4 => i16::from_le_bytes(b.try_into().unwrap()) as f64,
                5 => u32::from_le_bytes(b.try_into().unwrap()) as f64,
                6 => i32::from_le_bytes(b.try_into().unwrap()) as f64,
                7 => u64::from_le_bytes(b.try_into().unwrap()) as f64,
                8 => i64::from_le_bytes(b.try_into().unwrap()) as f64,
                10 => f32::from_le_bytes(b.try_into().unwrap()) as f64,
                11 => f64::from_le_bytes(b.try_into().unwrap()),
                _ => f64::NAN,
            })
            .collect())
    }

    /// A transform object's local matrix from its `.xform` compound (identity without one).
    fn xform(&self, props: &[(Header, u64)], warnings: &mut Vec<String>) -> Result<DMat4, String> {
        let Some((_, entry)) = props
            .iter()
            .find(|(h, _)| h.name == ".xform" && h.kind == 0)
        else {
            return Ok(DMat4::IDENTITY);
        };
        let xform = self.compound(*entry)?;
        let get = |name: &str| -> Result<Vec<f64>, String> {
            match xform.iter().find(|(h, _)| h.name == name) {
                Some((h, e)) if h.kind != 0 => self.first_sample(h, *e),
                _ => Ok(vec![]),
            }
        };
        let ops = get(".ops")?;
        let vals = get(".vals")?;
        if xform.iter().any(|(h, _)| h.name == ".inherits")
            && get(".inherits")?.first().is_some_and(|v| *v == 0.)
        {
            warnings.push(
                "A transform does not inherit its parent's; the parent's is applied anyway".into(),
            );
        }
        let mut m = DMat4::IDENTITY;
        let mut at = 0;
        let mut take = |n: usize| -> Result<&[f64], String> {
            let out = vals
                .get(at..at + n)
                .ok_or("An Alembic transform has too few values")?;
            at += n;
            Ok(out)
        };
        for op in ops {
            let step = match (op as u32) >> 4 {
                0 => {
                    let s = take(3)?;
                    DMat4::from_scale(DVec3::new(s[0], s[1], s[2]))
                }
                1 => {
                    let t = take(3)?;
                    DMat4::from_translation(DVec3::new(t[0], t[1], t[2]))
                }
                2 => {
                    let r = take(4)?;
                    let axis = DVec3::new(r[0], r[1], r[2]).normalize_or_zero();
                    if axis == DVec3::ZERO {
                        DMat4::IDENTITY
                    } else {
                        DMat4::from_axis_angle(axis, r[3].to_radians())
                    }
                }
                // Row-major with the translation in the last row: its rows are glam's columns.
                3 => DMat4::from_cols_array(&take(16)?.try_into().unwrap()),
                4 => DMat4::from_rotation_x(take(1)?[0].to_radians()),
                5 => DMat4::from_rotation_y(take(1)?[0].to_radians()),
                6 => DMat4::from_rotation_z(take(1)?[0].to_radians()),
                _ => return Err("Unsupported Alembic transform operation".into()),
            };
            m *= step;
        }
        if !m.is_finite() {
            return Err("An Alembic transform is not finite".into());
        }
        Ok(m)
    }

    /// The polygon mesh in `props`' `.geom`, placed by `world`, added to `mesh`.
    fn poly_mesh(
        &self,
        props: &[(Header, u64)],
        world: DMat4,
        mesh: &mut SurfaceMesh,
        warnings: &mut Vec<String>,
    ) -> Result<(), String> {
        let Some((_, geom)) = props.iter().find(|(h, _)| h.name == ".geom" && h.kind == 0) else {
            return Err("An Alembic polygon mesh has no .geom".into());
        };
        let geom = self.compound(*geom)?;
        let get = |name: &str| -> Result<Vec<f64>, String> {
            match geom.iter().find(|(h, _)| h.name == name) {
                Some((h, e)) if h.kind != 0 => self.first_sample(h, *e),
                _ => Err(format!("An Alembic polygon mesh has no {name}")),
            }
        };
        let points = get("P")?;
        let indices = get(".faceIndices")?;
        let counts = get(".faceCounts")?;
        if points.len() % 3 != 0 {
            return Err("Alembic points come in threes".into());
        }
        if let Some((h, e)) = geom.iter().find(|(h, _)| h.name == "P")
            && self.ogawa.group(*e)?.len() > 2
            && h.kind == 2
        {
            warnings.push(
                "The mesh is animated: its first sample is imported, as a static mesh".into(),
            );
        }
        let base = mesh.vertices.len();
        let count = points.len() / 3;
        for p in points.as_chunks::<3>().0 {
            mesh.vertices
                .push(world.transform_point3(DVec3::new(p[0], p[1], p[2])));
        }
        let index = |k: f64| -> Result<usize, String> {
            if k < 0. || k as usize >= count {
                return Err("An Alembic face points past its mesh's points".into());
            }
            Ok(base + k as usize)
        };
        let mut at = 0;
        for n in counts {
            let n = n as usize;
            if at + n > indices.len() {
                return Err("Alembic face counts and indices disagree".into());
            }
            if n >= 3 {
                // Clockwise in the file: walk it backwards.
                let face: Vec<usize> = indices[at..at + n]
                    .iter()
                    .rev()
                    .map(|k| index(*k))
                    .collect::<Result<_, _>>()?;
                for k in 1..n - 1 {
                    mesh.triangles.push([face[0], face[k], face[k + 1]]);
                }
            }
            at += n;
        }
        Ok(())
    }

    /// Walk the object at `entry` (the object itself is described by `meta`).
    fn object(
        &self,
        entry: u64,
        meta: &str,
        parent: DMat4,
        depth: usize,
        mesh: &mut SurfaceMesh,
        warnings: &mut Vec<String>,
    ) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("Alembic objects are nested too deep".into());
        }
        let children = self.ogawa.group(entry)?;
        if children.len() < 2 {
            return Ok(());
        }
        let props = self.compound(children[0])?;
        let mut world = parent;
        if meta.contains("AbcGeom_Xform") {
            world = parent * self.xform(&props, warnings)?;
        }
        if meta.contains("AbcGeom_PolyMesh") {
            self.poly_mesh(&props, world, mesh, warnings)?;
        } else if meta.contains("AbcGeom_SubD") {
            // A subdivision surface stores its cage like a polygon mesh.
            self.poly_mesh(&props, world, mesh, warnings)?;
        }
        // Child objects, then their headers.
        let headers = self.ogawa.data(*children.last().unwrap())?;
        let mut c = Cursor {
            bytes: headers,
            at: 0,
        };
        for &child in &children[1..children.len() - 1] {
            if c.bytes.len() - c.at <= 32 {
                break;
            }
            let n = c.u32()? as usize;
            c.text(n)?;
            let index = c.take(1)?[0] as usize;
            let meta = if index == 0xff {
                let n = c.u32()? as usize;
                c.text(n)?
            } else {
                self.meta(index)
            };
            self.object(child, &meta, world, depth + 1, mesh, warnings)?;
        }
        Ok(())
    }
}

/// The meshes of an Alembic file, in metres (by their size), Y-up.
pub fn abc(bytes: &[u8]) -> Result<Imported, String> {
    if bytes.len() < 16 || &bytes[..5] != b"Ogawa" {
        return Err(
            "This is not an Ogawa Alembic file (older HDF5 Alembic files are not read)".into(),
        );
    }
    let ogawa = Ogawa { bytes };
    let root = ogawa.u64_at(8)?;
    let archive = ogawa.group(root)?;
    if archive.len() < 6 {
        return Err("The Alembic archive is incomplete".into());
    }
    // Metadata table: a length byte and the text, for each entry.
    let table = ogawa.data(archive[5])?;
    let mut metadata = vec![];
    let mut at = 0;
    while at < table.len() {
        let n = table[at] as usize;
        let text = table
            .get(at + 1..at + 1 + n)
            .ok_or("The Alembic metadata table is cut short")?;
        metadata.push(String::from_utf8_lossy(text).into_owned());
        at += 1 + n;
    }
    let reader = Reader { ogawa, metadata };
    let mut mesh = SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    };
    let mut warnings = vec![];
    reader.object(archive[2], "", DMat4::IDENTITY, 0, &mut mesh, &mut warnings)?;
    if mesh.triangles.is_empty() {
        return Err("The Alembic file has no polygon mesh".into());
    }
    crate::mesh_import::guess_units(&mut mesh, &mut warnings, "Alembic");
    warnings.push("Geometry only: normals, UVs, materials and animation are not retained".into());
    crate::mesh_import::finish(mesh, 0, warnings)
}
