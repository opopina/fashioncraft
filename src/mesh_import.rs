//! Static triangle meshes from STL, PLY and COLLADA files, for avatars and props.
//!
//! STL (ASCII or binary) stores loose triangles: corners at exactly the same position are
//! joined. PLY (ASCII or binary, either byte order) keeps its vertices and fans polygons into
//! triangles. COLLADA reads the geometry of every node of its visual scene, skinned or not
//! (in its bind pose), with the node transforms, the declared unit and up axis.
//!
//! STL and PLY declare no unit: a body larger than 300 units is taken to be in millimetres,
//! one larger than 3 in centimetres, otherwise metres. Degenerate triangles are dropped.
//! Every guess and every drop is reported as a warning.
use crate::avatar::SurfaceMesh;
use glam::{DMat4, DVec3, DVec4};
use std::collections::HashMap;

/// A mesh with the notes the caller shows before accepting it.
pub struct Imported {
    pub mesh: SurfaceMesh,
    pub warnings: Vec<String>,
}

/// Join triangle corners at the same position, drop degenerate triangles.
fn from_corners(corners: &[[DVec3; 3]]) -> (SurfaceMesh, usize) {
    let mut index: HashMap<[u64; 3], usize> = HashMap::new();
    let mut mesh = SurfaceMesh {
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    let mut dropped = 0;
    for t in corners {
        let ids = t.map(|p| {
            *index
                .entry(p.to_array().map(f64::to_bits))
                .or_insert_with(|| {
                    mesh.vertices.push(p);
                    mesh.vertices.len() - 1
                })
        });
        let [a, b, c] = t;
        if ids[0] == ids[1]
            || ids[1] == ids[2]
            || ids[0] == ids[2]
            || (*b - *a).cross(*c - *a).length_squared() < 1e-24
        {
            dropped += 1;
            continue;
        }
        mesh.triangles.push(ids);
    }
    (mesh, dropped)
}

/// Remove degenerate triangles from an indexed mesh.
fn drop_degenerate(mesh: &mut SurfaceMesh) -> usize {
    let before = mesh.triangles.len();
    let v = &mesh.vertices;
    mesh.triangles.retain(|t| {
        let [a, b, c] = t.map(|i| v[i]);
        t[0] != t[1]
            && t[1] != t[2]
            && t[0] != t[2]
            && (b - a).cross(c - a).length_squared() >= 1e-24
    });
    before - mesh.triangles.len()
}

/// Scale a mesh without a declared unit to metres by its height, and say how.
fn guess_units(mesh: &mut SurfaceMesh, warnings: &mut Vec<String>, format: &str) {
    let (lo, hi) = mesh.vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let size = (hi - lo).max_element();
    // Bodies stand between about 0.5 and 2.5 m: 500 to 2500 mm, 50 to 250 cm.
    let (scale, unit) = if size > 300. {
        (0.001, "millimetres")
    } else if size > 3. {
        (0.01, "centimetres")
    } else {
        (1., "metres")
    };
    for p in &mut mesh.vertices {
        *p *= scale;
    }
    warnings.push(format!(
        "{format} declares no unit: from its size ({size:.1} units) it is read as {unit}, Y up."
    ));
}

fn finish(
    mut mesh: SurfaceMesh,
    dropped: usize,
    mut warnings: Vec<String>,
) -> Result<Imported, String> {
    let dropped = dropped + drop_degenerate(&mut mesh);
    if dropped > 0 {
        warnings.push(format!("{dropped} degenerate triangles were left out."));
    }
    mesh.validate()?;
    Ok(Imported { mesh, warnings })
}

/// STL, binary or ASCII.
pub fn stl(bytes: &[u8]) -> Result<Imported, String> {
    let binary = bytes.len() >= 84 && {
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        bytes.len() == 84 + count * 50
    };
    let mut corners = Vec::new();
    if binary {
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        let float = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as f64;
        for k in 0..count {
            let at = 84 + k * 50 + 12;
            corners.push([0, 1, 2].map(|c| {
                let p = at + c * 12;
                DVec3::new(float(p), float(p + 4), float(p + 8))
            }));
        }
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| "Not an STL file")?;
        if !text.trim_start().starts_with("solid") {
            return Err("Not an STL file".into());
        }
        let mut loop_points = Vec::new();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            if words.next() == Some("vertex") {
                let p: Vec<f64> = words
                    .take(3)
                    .map(|w| w.parse().map_err(|_| "Malformed STL vertex"))
                    .collect::<Result<_, _>>()?;
                if p.len() != 3 {
                    return Err("Malformed STL vertex".into());
                }
                loop_points.push(DVec3::new(p[0], p[1], p[2]));
                if loop_points.len() == 3 {
                    corners.push([loop_points[0], loop_points[1], loop_points[2]]);
                    loop_points.clear();
                }
            }
        }
    }
    if corners.is_empty() {
        return Err("The STL file holds no triangles".into());
    }
    if corners.iter().flatten().any(|p| !p.is_finite()) {
        return Err("The STL file holds invalid coordinates".into());
    }
    let (mut mesh, dropped) = from_corners(&corners);
    let mut warnings = vec![
        "STL holds geometry only: corners at the same position are joined into one surface."
            .to_string(),
    ];
    guess_units(&mut mesh, &mut warnings, "STL");
    finish(mesh, dropped, warnings)
}

#[derive(Clone, Copy)]
enum Scalar {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Scalar {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "char" | "int8" => Self::I8,
            "uchar" | "uint8" => Self::U8,
            "short" | "int16" => Self::I16,
            "ushort" | "uint16" => Self::U16,
            "int" | "int32" => Self::I32,
            "uint" | "uint32" => Self::U32,
            "float" | "float32" => Self::F32,
            "double" | "float64" => Self::F64,
            _ => return None,
        })
    }
    fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }
    fn read(self, b: &[u8], little: bool) -> f64 {
        macro_rules! num {
            ($t:ty) => {{
                let a = b[..std::mem::size_of::<$t>()].try_into().unwrap();
                (if little {
                    <$t>::from_le_bytes(a)
                } else {
                    <$t>::from_be_bytes(a)
                }) as f64
            }};
        }
        match self {
            Self::I8 => b[0] as i8 as f64,
            Self::U8 => b[0] as f64,
            Self::I16 => num!(i16),
            Self::U16 => num!(u16),
            Self::I32 => num!(i32),
            Self::U32 => num!(u32),
            Self::F32 => num!(f32),
            Self::F64 => num!(f64),
        }
    }
}

enum Property {
    Value(String, Scalar),
    List(String, Scalar, Scalar),
}

/// PLY, ASCII or binary in either byte order.
pub fn ply(bytes: &[u8]) -> Result<Imported, String> {
    let end = bytes
        .windows(10)
        .position(|w| w == b"end_header")
        .ok_or("Not a PLY file")?;
    let body = bytes[end..]
        .iter()
        .position(|b| *b == b'\n')
        .map(|n| end + n + 1)
        .ok_or("Not a PLY file")?;
    let header = std::str::from_utf8(&bytes[..end]).map_err(|_| "Not a PLY file")?;
    if !header.starts_with("ply") {
        return Err("Not a PLY file".into());
    }
    let mut format = "";
    let mut elements: Vec<(String, usize, Vec<Property>)> = Vec::new();
    for line in header.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["format", f, ..] => format = f,
            ["element", name, count] => elements.push((
                name.to_string(),
                count.parse().map_err(|_| "Malformed PLY element")?,
                Vec::new(),
            )),
            ["property", "list", count, item, name] => {
                let (Some(c), Some(i)) = (Scalar::parse(count), Scalar::parse(item)) else {
                    return Err("Unknown PLY property type".into());
                };
                elements
                    .last_mut()
                    .ok_or("Malformed PLY header")?
                    .2
                    .push(Property::List(name.to_string(), c, i));
            }
            ["property", kind, name] => {
                let kind = Scalar::parse(kind).ok_or("Unknown PLY property type")?;
                elements
                    .last_mut()
                    .ok_or("Malformed PLY header")?
                    .2
                    .push(Property::Value(name.to_string(), kind));
            }
            _ => {}
        }
    }
    let ascii = format == "ascii";
    let little = format == "binary_little_endian";
    if !ascii && !little && format != "binary_big_endian" {
        return Err(format!("Unknown PLY format {format}"));
    }
    let mut mesh = SurfaceMesh {
        vertices: Vec::new(),
        triangles: Vec::new(),
    };
    let mut words = std::str::from_utf8(&bytes[body..])
        .ok()
        .filter(|_| ascii)
        .map(|t| t.split_whitespace());
    let mut at = body;
    let mut next = |kind: Scalar| -> Result<f64, String> {
        match words.as_mut() {
            Some(w) => w
                .next()
                .ok_or("The PLY file ends early")?
                .parse()
                .map_err(|_| "Malformed PLY value".to_string()),
            None => {
                let size = kind.size();
                let slice = bytes.get(at..at + size).ok_or("The PLY file ends early")?;
                at += size;
                Ok(kind.read(slice, little))
            }
        }
    };
    let mut polygons = 0;
    for (name, count, properties) in &elements {
        for _ in 0..*count {
            let mut p = [0.; 3];
            for property in properties {
                match property {
                    Property::Value(field, kind) => {
                        let v = next(*kind)?;
                        if name == "vertex" {
                            match field.as_str() {
                                "x" => p[0] = v,
                                "y" => p[1] = v,
                                "z" => p[2] = v,
                                _ => {}
                            }
                        }
                    }
                    Property::List(field, count_kind, item) => {
                        let n = next(*count_kind)? as usize;
                        let mut ids = Vec::with_capacity(n);
                        for _ in 0..n {
                            ids.push(next(*item)? as usize);
                        }
                        if name == "face" && (field == "vertex_indices" || field == "vertex_index")
                        {
                            if n > 3 {
                                polygons += 1;
                            }
                            for k in 1..n.saturating_sub(1) {
                                mesh.triangles.push([ids[0], ids[k], ids[k + 1]]);
                            }
                        }
                    }
                }
            }
            if name == "vertex" {
                mesh.vertices.push(DVec3::from_array(p));
            }
        }
    }
    if mesh
        .triangles
        .iter()
        .flatten()
        .any(|i| *i >= mesh.vertices.len())
    {
        return Err("The PLY file refers to a missing vertex".into());
    }
    let mut warnings =
        vec!["PLY holds geometry only: colours and normals are not kept.".to_string()];
    if polygons > 0 {
        warnings.push(format!(
            "{polygons} polygons were split into triangles as fans."
        ));
    }
    guess_units(&mut mesh, &mut warnings, "PLY");
    finish(mesh, 0, warnings)
}

/// A parsed XML document: elements with their attributes and text, by index.
struct Tree {
    nodes: Vec<Element>,
}

struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<usize>,
}

impl Tree {
    fn parse(text: &str) -> Result<Self, String> {
        use quick_xml::events::Event;
        let mut reader = quick_xml::Reader::from_str(text);
        reader.config_mut().expand_empty_elements = true;
        let mut nodes: Vec<Element> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        loop {
            match reader.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) => {
                    let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    let mut attributes = Vec::new();
                    for a in e.attributes() {
                        let a = a.map_err(|e| e.to_string())?;
                        let key = String::from_utf8_lossy(a.key.local_name().as_ref()).into_owned();
                        let value = String::from_utf8_lossy(&a.value).into_owned();
                        attributes.push((key, value));
                    }
                    nodes.push(Element {
                        name,
                        attributes,
                        text: String::new(),
                        children: Vec::new(),
                    });
                    let id = nodes.len() - 1;
                    if let Some(&parent) = stack.last() {
                        nodes[parent].children.push(id);
                    } else if id != 0 {
                        return Err("More than one root element".into());
                    }
                    stack.push(id);
                }
                Event::End(_) => {
                    stack.pop().ok_or("Unmatched closing tag")?;
                }
                Event::Text(e) => {
                    if let Some(&top) = stack.last() {
                        nodes[top]
                            .text
                            .push_str(&e.decode().map_err(|e| e.to_string())?);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if nodes.is_empty() || !stack.is_empty() {
            return Err("Incomplete XML".into());
        }
        Ok(Self { nodes })
    }
    fn name(&self, n: usize) -> &str {
        &self.nodes[n].name
    }
    fn attribute(&self, n: usize, key: &str) -> Option<&str> {
        self.nodes[n]
            .attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    fn children(&self, n: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes[n].children.iter().copied()
    }
    fn child(&self, n: usize, name: &str) -> Option<usize> {
        self.children(n).find(|c| self.name(*c) == name)
    }
    fn floats(&self, n: usize) -> Result<Vec<f64>, String> {
        self.nodes[n]
            .text
            .split_whitespace()
            .map(|w| {
                w.parse::<f64>()
                    .map_err(|_| "Malformed COLLADA number".to_string())
            })
            .collect()
    }
}

/// A COLLADA mesh's positions, its triangles, and how many polygons were split into them.
type MeshPart = (Vec<DVec3>, Vec<[usize; 3]>, usize);

/// COLLADA geometry of the whole visual scene, in metres and Y up.
pub fn dae(text: &str) -> Result<Imported, String> {
    let doc = Tree::parse(text).map_err(|e| format!("Not a COLLADA file: {e}"))?;
    let root = 0;
    if doc.name(root) != "COLLADA" {
        return Err("Not a COLLADA file".into());
    }
    let mut warnings = Vec::new();
    let asset = doc.child(root, "asset");
    let meter = asset
        .and_then(|a| doc.child(a, "unit"))
        .and_then(|u| doc.attribute(u, "meter"))
        .and_then(|m| m.parse::<f64>().ok())
        .filter(|m| m.is_finite() && *m > 0.)
        .unwrap_or(1.);
    let up = asset
        .and_then(|a| doc.child(a, "up_axis"))
        .map_or("Y_UP".to_string(), |u| doc.nodes[u].text.trim().to_string());
    let by_id: HashMap<&str, usize> = (0..doc.nodes.len())
        .filter_map(|n| doc.attribute(n, "id").map(|id| (id, n)))
        .collect();
    let target = |url: &str| by_id.get(url.trim_start_matches('#')).copied();
    // Positions of a geometry's mesh, its triangles, and how many polygons were split.
    let geometry = |geom: usize| -> Result<MeshPart, String> {
        let mesh = doc
            .child(geom, "mesh")
            .ok_or("Only COLLADA meshes are read")?;
        let vertices = doc
            .child(mesh, "vertices")
            .ok_or("A COLLADA mesh has no vertices")?;
        let source = doc
            .children(vertices)
            .find(|c| doc.name(*c) == "input" && doc.attribute(*c, "semantic") == Some("POSITION"))
            .and_then(|i| doc.attribute(i, "source"))
            .and_then(target)
            .ok_or("A COLLADA mesh has no positions")?;
        let values = doc.floats(
            doc.child(source, "float_array")
                .ok_or("A COLLADA source has no array")?,
        )?;
        let stride = doc
            .child(source, "technique_common")
            .and_then(|t| doc.child(t, "accessor"))
            .and_then(|a| doc.attribute(a, "stride"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(3)
            .max(3);
        let points: Vec<DVec3> = values
            .chunks_exact(stride)
            .map(|c| DVec3::new(c[0], c[1], c[2]))
            .collect();
        let mut triangles = Vec::new();
        let mut polygons = 0;
        for primitive in doc.children(mesh) {
            let kind = doc.name(primitive);
            if !matches!(kind, "triangles" | "polylist" | "polygons") {
                continue;
            }
            let inputs: Vec<usize> = doc
                .children(primitive)
                .filter(|c| doc.name(*c) == "input")
                .collect();
            let width = inputs
                .iter()
                .filter_map(|i| doc.attribute(*i, "offset")?.parse::<usize>().ok())
                .max()
                .map_or(1, |m| m + 1);
            let offset = inputs
                .iter()
                .find(|i| doc.attribute(**i, "semantic") == Some("VERTEX"))
                .and_then(|i| doc.attribute(*i, "offset")?.parse::<usize>().ok())
                .unwrap_or(0);
            let corner_lists: Vec<Vec<usize>> = match kind {
                "triangles" => {
                    let p = doc.floats(
                        doc.child(primitive, "p")
                            .ok_or("COLLADA triangles without p")?,
                    )?;
                    p.chunks_exact(width * 3)
                        .map(|t| (0..3).map(|k| t[k * width + offset] as usize).collect())
                        .collect()
                }
                "polylist" => {
                    let counts = doc.floats(
                        doc.child(primitive, "vcount")
                            .ok_or("COLLADA polylist without vcount")?,
                    )?;
                    let p = doc.floats(
                        doc.child(primitive, "p")
                            .ok_or("COLLADA polylist without p")?,
                    )?;
                    let mut at = 0;
                    let mut lists = Vec::new();
                    for n in counts {
                        let n = n as usize;
                        let span = p
                            .get(at..at + n * width)
                            .ok_or("COLLADA polylist ends early")?;
                        lists.push((0..n).map(|k| span[k * width + offset] as usize).collect());
                        at += n * width;
                    }
                    lists
                }
                _ => doc
                    .children(primitive)
                    .filter(|c| doc.name(*c) == "p")
                    .map(|p| {
                        let v = doc.floats(p)?;
                        Ok(v.chunks_exact(width).map(|c| c[offset] as usize).collect())
                    })
                    .collect::<Result<_, String>>()?,
            };
            for corners in corner_lists {
                if corners.len() > 3 {
                    polygons += 1;
                }
                for k in 1..corners.len().saturating_sub(1) {
                    triangles.push([corners[0], corners[k], corners[k + 1]]);
                }
            }
        }
        if triangles.iter().flatten().any(|i| *i >= points.len()) {
            return Err("A COLLADA mesh refers to a missing position".into());
        }
        Ok((points, triangles, polygons))
    };
    // COLLADA writes matrices row by row.
    let matrix = |v: &[f64]| -> Result<DMat4, String> {
        if v.len() != 16 {
            return Err("Malformed COLLADA matrix".into());
        }
        Ok(DMat4::from_cols_array(&[
            v[0], v[4], v[8], v[12], v[1], v[5], v[9], v[13], v[2], v[6], v[10], v[14], v[3], v[7],
            v[11], v[15],
        ]))
    };
    let local = |node: usize| -> Result<DMat4, String> {
        let mut m = DMat4::IDENTITY;
        for t in doc.children(node) {
            let v = || doc.floats(t);
            let three = |v: &[f64]| -> Result<DVec3, String> {
                (v.len() >= 3)
                    .then(|| DVec3::new(v[0], v[1], v[2]))
                    .ok_or_else(|| "Malformed COLLADA transform".to_string())
            };
            m *= match doc.name(t) {
                "matrix" => matrix(&v()?)?,
                "translate" => DMat4::from_translation(three(&v()?)?),
                "scale" => DMat4::from_scale(three(&v()?)?),
                "rotate" => {
                    let v = v()?;
                    if v.len() != 4 {
                        return Err("Malformed COLLADA rotation".into());
                    }
                    DMat4::from_axis_angle(
                        DVec3::new(v[0], v[1], v[2]).normalize_or(DVec3::Y),
                        v[3].to_radians(),
                    )
                }
                _ => DMat4::IDENTITY,
            };
        }
        Ok(m)
    };
    let scene = doc
        .child(root, "scene")
        .and_then(|s| doc.child(s, "instance_visual_scene"))
        .and_then(|i| doc.attribute(i, "url"))
        .and_then(target)
        .or_else(|| {
            doc.child(root, "library_visual_scenes")
                .and_then(|l| doc.child(l, "visual_scene"))
        })
        .ok_or("The COLLADA file has no visual scene")?;
    let mut corners: Vec<[DVec3; 3]> = Vec::new();
    let (mut polygons, mut skinned) = (0, 0);
    let mut stack: Vec<(usize, DMat4)> = doc
        .children(scene)
        .filter(|c| doc.name(*c) == "node")
        .map(|n| (n, DMat4::IDENTITY))
        .collect();
    while let Some((node, parent)) = stack.pop() {
        let world = parent * local(node)?;
        for instance in doc.children(node) {
            let (geom, bind) = match doc.name(instance) {
                "instance_geometry" => (
                    doc.attribute(instance, "url").and_then(target),
                    DMat4::IDENTITY,
                ),
                "instance_controller" => {
                    let skin = doc
                        .attribute(instance, "url")
                        .and_then(target)
                        .and_then(|c| doc.child(c, "skin"));
                    let Some(skin) = skin else { continue };
                    skinned += 1;
                    let bind = match doc.child(skin, "bind_shape_matrix") {
                        Some(m) => matrix(&doc.floats(m)?)?,
                        None => DMat4::IDENTITY,
                    };
                    (doc.attribute(skin, "source").and_then(target), bind)
                }
                "node" => {
                    stack.push((instance, world));
                    continue;
                }
                _ => continue,
            };
            let Some(geom) = geom else { continue };
            let (points, triangles, n) = geometry(geom)?;
            polygons += n;
            let m = world * bind;
            let placed: Vec<DVec3> = points
                .iter()
                .map(|p| (m * DVec4::new(p.x, p.y, p.z, 1.)).truncate())
                .collect();
            corners.extend(triangles.iter().map(|t| t.map(|i| placed[i])));
        }
    }
    if corners.is_empty() {
        return Err("The COLLADA scene holds no triangles".into());
    }
    let turn = |p: DVec3| -> DVec3 {
        let p = p * meter;
        match up.as_str() {
            "Z_UP" => DVec3::new(p.x, p.z, -p.y),
            "X_UP" => DVec3::new(-p.y, p.x, p.z),
            _ => p,
        }
    };
    let corners: Vec<[DVec3; 3]> = corners.iter().map(|t| t.map(turn)).collect();
    if corners.iter().flatten().any(|p| !p.is_finite()) {
        return Err("The COLLADA file holds invalid coordinates".into());
    }
    let (mesh, dropped) = from_corners(&corners);
    warnings.push(format!(
        "COLLADA geometry in units of {meter} m, {up} turned to Y up; materials, UVs and normals are not kept."
    ));
    if skinned > 0 {
        warnings.push(format!(
            "{skinned} skinned meshes are read in their bind pose."
        ));
    }
    if polygons > 0 {
        warnings.push(format!(
            "{polygons} polygons were split into triangles as fans."
        ));
    }
    finish(mesh, dropped, warnings)
}
