//! FBX export (binary, version 7.4) of the garment and the avatar.
//!
//! Written from publicly documented descriptions of the binary FBX layout: a 27-byte header,
//! nested node records (end offset, property count, property bytes, name, typed properties,
//! children, a 13-byte null record closing any list of children) and a footer. Each garment
//! piece is a mesh model with its fabric's material (diffuse colour, and its texture image
//! copied next to the file); the avatar is one more model. Positions are in centimetres, the
//! file's declared unit, Y up. Polygons are wound counter-clockwise seen from the fabric's
//! front face; normals and texture coordinates are given per polygon corner.
use crate::{
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate},
    gltf_export::{front_triangles, texture_coordinates},
    model::State,
};
use glam::DVec3;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

const VERSION: u32 = 7400;

/// A typed node property.
enum Prop {
    Bool(bool),
    I32(i32),
    I64(i64),
    F64(f64),
    Str(String),
    F64s(Vec<f64>),
    I32s(Vec<i32>),
}

/// A node with its properties and children.
struct Node {
    name: &'static str,
    props: Vec<Prop>,
    children: Vec<Node>,
}

fn node(name: &'static str, props: Vec<Prop>) -> Node {
    Node {
        name,
        props,
        children: Vec::new(),
    }
}

fn with(name: &'static str, props: Vec<Prop>, children: Vec<Node>) -> Node {
    Node {
        name,
        props,
        children,
    }
}

fn s(text: &str) -> Prop {
    Prop::Str(text.to_string())
}

/// An object's name as FBX stores it: the name, a separator and its class.
fn object_name(name: &str, class: &str) -> Prop {
    Prop::Str(format!("{name}\u{0}\u{1}{class}"))
}

impl Prop {
    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Prop::Bool(v) => {
                out.push(b'C');
                out.push(*v as u8);
            }
            Prop::I32(v) => {
                out.push(b'I');
                out.extend_from_slice(&v.to_le_bytes());
            }
            Prop::I64(v) => {
                out.push(b'L');
                out.extend_from_slice(&v.to_le_bytes());
            }
            Prop::F64(v) => {
                out.push(b'D');
                out.extend_from_slice(&v.to_le_bytes());
            }
            Prop::Str(v) => {
                out.push(b'S');
                out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                out.extend_from_slice(v.as_bytes());
            }
            Prop::F64s(v) => {
                out.push(b'd');
                out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&((v.len() * 8) as u32).to_le_bytes());
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            Prop::I32s(v) => {
                out.push(b'i');
                out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&((v.len() * 4) as u32).to_le_bytes());
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
    }
}

impl Node {
    /// Write this record at the end of `out`, whose length is its offset in the file.
    fn write(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.extend_from_slice(&[0u8; 12]);
        out.push(self.name.len() as u8);
        out.extend_from_slice(self.name.as_bytes());
        let props_start = out.len();
        for p in &self.props {
            p.write(out);
        }
        let props_len = out.len() - props_start;
        for child in &self.children {
            child.write(out);
        }
        if !self.children.is_empty() {
            out.extend_from_slice(&[0u8; 13]);
        }
        let end = out.len() as u32;
        out[start..start + 4].copy_from_slice(&end.to_le_bytes());
        out[start + 4..start + 8].copy_from_slice(&(self.props.len() as u32).to_le_bytes());
        out[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
    }
}

/// A `P` entry of a Properties70 block.
fn p(name: &str, kind: &str, label: &str, flags: &str, values: Vec<Prop>) -> Node {
    let mut props = vec![s(name), s(kind), s(label), s(flags)];
    props.extend(values);
    node("P", props)
}

fn properties(entries: Vec<Node>) -> Node {
    with("Properties70", vec![], entries)
}

/// What to write.
pub struct FbxScene<'a> {
    pub template: &'a GarmentTemplate,
    pub garment: &'a CompiledGarment,
    pub state: &'a State,
    /// Image files of fabric textures, by asset ID.
    pub images: &'a [(String, PathBuf)],
    pub avatar: Option<&'a SurfaceMesh>,
}

/// A mesh's geometry node: centimetre positions, corners closing each triangle with a
/// negative index, and per-corner normals and texture coordinates.
fn geometry(
    id: i64,
    name: &str,
    points: &[DVec3],
    triangles: &[[usize; 3]],
    uv: Option<&[[f64; 2]]>,
) -> Node {
    let mut normal = vec![DVec3::ZERO; points.len()];
    for t in triangles {
        let face = (points[t[1]] - points[t[0]]).cross(points[t[2]] - points[t[0]]);
        for i in t {
            normal[*i] += face;
        }
    }
    let normal: Vec<DVec3> = normal.iter().map(|n| n.normalize_or(DVec3::Y)).collect();
    let vertices: Vec<f64> = points.iter().flat_map(|q| (*q * 100.).to_array()).collect();
    let corners: Vec<i32> = triangles
        .iter()
        .flat_map(|[a, b, c]| [*a as i32, *b as i32, -(*c as i32) - 1])
        .collect();
    let normals: Vec<f64> = triangles
        .iter()
        .flatten()
        .flat_map(|i| normal[*i].to_array())
        .collect();
    let mut children = vec![
        node("GeometryVersion", vec![Prop::I32(124)]),
        node("Vertices", vec![Prop::F64s(vertices)]),
        node("PolygonVertexIndex", vec![Prop::I32s(corners)]),
        with(
            "LayerElementNormal",
            vec![Prop::I32(0)],
            vec![
                node("Version", vec![Prop::I32(101)]),
                node("Name", vec![s("")]),
                node("MappingInformationType", vec![s("ByPolygonVertex")]),
                node("ReferenceInformationType", vec![s("Direct")]),
                node("Normals", vec![Prop::F64s(normals)]),
            ],
        ),
    ];
    let mut layers = vec![with(
        "LayerElement",
        vec![],
        vec![
            node("Type", vec![s("LayerElementNormal")]),
            node("TypedIndex", vec![Prop::I32(0)]),
        ],
    )];
    if let Some(uv) = uv {
        let values: Vec<f64> = uv.iter().flatten().copied().collect();
        let index: Vec<i32> = triangles.iter().flatten().map(|i| *i as i32).collect();
        children.push(with(
            "LayerElementUV",
            vec![Prop::I32(0)],
            vec![
                node("Version", vec![Prop::I32(101)]),
                node("Name", vec![s("UVMap")]),
                node("MappingInformationType", vec![s("ByPolygonVertex")]),
                node("ReferenceInformationType", vec![s("IndexToDirect")]),
                node("UV", vec![Prop::F64s(values)]),
                node("UVIndex", vec![Prop::I32s(index)]),
            ],
        ));
        layers.push(with(
            "LayerElement",
            vec![],
            vec![
                node("Type", vec![s("LayerElementUV")]),
                node("TypedIndex", vec![Prop::I32(0)]),
            ],
        ));
    }
    children.push(with(
        "LayerElementMaterial",
        vec![Prop::I32(0)],
        vec![
            node("Version", vec![Prop::I32(101)]),
            node("Name", vec![s("")]),
            node("MappingInformationType", vec![s("AllSame")]),
            node("ReferenceInformationType", vec![s("IndexToDirect")]),
            node("Materials", vec![Prop::I32s(vec![0])]),
        ],
    ));
    layers.push(with(
        "LayerElement",
        vec![],
        vec![
            node("Type", vec![s("LayerElementMaterial")]),
            node("TypedIndex", vec![Prop::I32(0)]),
        ],
    ));
    let mut layer = vec![node("Version", vec![Prop::I32(100)])];
    layer.extend(layers);
    children.push(with("Layer", vec![Prop::I32(0)], layer));
    with(
        "Geometry",
        vec![Prop::I64(id), object_name(name, "Geometry"), s("Mesh")],
        children,
    )
}

fn model(id: i64, name: &str) -> Node {
    with(
        "Model",
        vec![Prop::I64(id), object_name(name, "Model"), s("Mesh")],
        vec![
            node("Version", vec![Prop::I32(232)]),
            properties(vec![
                p(
                    "Lcl Translation",
                    "Lcl Translation",
                    "",
                    "A",
                    vec![Prop::F64(0.), Prop::F64(0.), Prop::F64(0.)],
                ),
                p(
                    "Lcl Rotation",
                    "Lcl Rotation",
                    "",
                    "A",
                    vec![Prop::F64(0.), Prop::F64(0.), Prop::F64(0.)],
                ),
                p(
                    "Lcl Scaling",
                    "Lcl Scaling",
                    "",
                    "A",
                    vec![Prop::F64(1.), Prop::F64(1.), Prop::F64(1.)],
                ),
            ]),
            node("Shading", vec![Prop::Bool(true)]),
            node("Culling", vec![s("CullingOff")]),
        ],
    )
}

fn material(id: i64, name: &str, color: [f64; 3]) -> Node {
    with(
        "Material",
        vec![Prop::I64(id), object_name(name, "Material"), s("")],
        vec![
            node("Version", vec![Prop::I32(102)]),
            node("ShadingModel", vec![s("Phong")]),
            node("MultiLayer", vec![Prop::I32(0)]),
            properties(vec![
                p(
                    "DiffuseColor",
                    "Color",
                    "",
                    "A",
                    color.map(Prop::F64).into(),
                ),
                p("DiffuseFactor", "Number", "", "A", vec![Prop::F64(1.)]),
                p("SpecularFactor", "Number", "", "A", vec![Prop::F64(0.05)]),
            ]),
        ],
    )
}

fn connection(child: i64, parent: i64) -> Node {
    node("C", vec![s("OO"), Prop::I64(child), Prop::I64(parent)])
}

/// Texture images to copy next to the file, by the name the file uses for each.
pub type Textures = Vec<(String, PathBuf)>;

/// The file's bytes, and the texture images to copy next to it by the name it uses.
pub fn fbx(scene: &FbxScene, stem: &str) -> Result<(Vec<u8>, Textures), String> {
    let garment = scene.garment;
    if scene.state.x.len() != garment.mesh.uv.len() {
        return Err("The cloth does not match the garment".into());
    }
    let appearances = crate::garment::appearance::bind(scene.template, garment);
    let triangles = front_triangles(scene.template, garment);
    let uv = texture_coordinates(scene.template, garment);
    let x = &scene.state.x;
    let mut next_id = 1_000_000_i64;
    let mut id = || {
        next_id += 1;
        next_id
    };
    let mut objects = Vec::new();
    let mut connections = Vec::new();
    let mut materials: BTreeMap<String, i64> = BTreeMap::new();
    let mut textures: Vec<(String, PathBuf)> = Vec::new();
    let (mut models, mut geometries, mut videos) = (0, 0, 0);
    for panel in &garment.panels {
        let fabric = scene
            .template
            .panels
            .iter()
            .find(|q| q.id == panel.id)
            .map_or_else(
                || format!("fabric_{}", panel.material),
                |q| q.fabric.clone(),
            );
        let material_id = match materials.get(&fabric) {
            Some(m) => *m,
            None => {
                let m = id();
                let appearance = appearances[panel.material].as_ref();
                let color =
                    appearance.map_or([0.91, 0.92, 0.94], |a| a.linear_color().map(f64::from));
                objects.push(material(m, &fabric, color));
                if let Some(t) = appearance.and_then(|a| a.texture.as_ref()) {
                    let source = scene
                        .images
                        .iter()
                        .find(|(asset, _)| *asset == t.asset_id)
                        .map(|(_, path)| path.clone())
                        .ok_or_else(|| format!("Texture image {} is missing", t.asset_id))?;
                    let extension = source
                        .extension()
                        .map_or("png".into(), |e| e.to_string_lossy().to_ascii_lowercase());
                    let file = format!("{stem}_{}.{extension}", t.asset_id);
                    let (video, texture) = (id(), id());
                    objects.push(with(
                        "Video",
                        vec![
                            Prop::I64(video),
                            object_name(&t.asset_id, "Video"),
                            s("Clip"),
                        ],
                        vec![
                            node("Type", vec![s("Clip")]),
                            node("FileName", vec![s(&file)]),
                            node("RelativeFilename", vec![s(&file)]),
                        ],
                    ));
                    objects.push(with(
                        "Texture",
                        vec![
                            Prop::I64(texture),
                            object_name(&t.asset_id, "Texture"),
                            s(""),
                        ],
                        vec![
                            node("Type", vec![s("TextureVideoClip")]),
                            node("Version", vec![Prop::I32(202)]),
                            node("TextureName", vec![object_name(&t.asset_id, "Texture")]),
                            node("Media", vec![object_name(&t.asset_id, "Video")]),
                            node("FileName", vec![s(&file)]),
                            node("RelativeFilename", vec![s(&file)]),
                        ],
                    ));
                    connections.push(connection(video, texture));
                    connections.push(node(
                        "C",
                        vec![s("OP"), Prop::I64(texture), Prop::I64(m), s("DiffuseColor")],
                    ));
                    textures.push((file, source));
                    videos += 1;
                }
                materials.insert(fabric.clone(), m);
                m
            }
        };
        let local: Vec<usize> = panel.vertices.clone().collect();
        let index: HashMap<usize, usize> = local.iter().enumerate().map(|(k, i)| (*i, k)).collect();
        let piece: Vec<[usize; 3]> = triangles[panel.triangles.clone()]
            .iter()
            .map(|t| t.map(|i| index[&i]))
            .collect();
        let points: Vec<DVec3> = local.iter().map(|i| x[*i]).collect();
        let st: Vec<[f64; 2]> = local.iter().map(|i| [uv[*i].x, uv[*i].y]).collect();
        let (m, g) = (id(), id());
        objects.push(geometry(g, &panel.id, &points, &piece, Some(&st)));
        objects.push(model(m, &panel.id));
        connections.push(connection(m, 0));
        connections.push(connection(g, m));
        connections.push(connection(material_id, m));
        models += 1;
        geometries += 1;
    }
    let mut avatar_material = 0;
    if let Some(avatar) = scene.avatar {
        let (m, g, mat) = (id(), id(), id());
        objects.push(geometry(
            g,
            "avatar",
            &avatar.vertices,
            &avatar.triangles,
            None,
        ));
        objects.push(model(m, "avatar"));
        objects.push(material(mat, "avatar", [0.45, 0.45, 0.47]));
        connections.push(connection(m, 0));
        connections.push(connection(g, m));
        connections.push(connection(mat, m));
        models += 1;
        geometries += 1;
        avatar_material = 1;
    }
    let count = |kind: &str, n: usize| {
        with(
            "ObjectType",
            vec![s(kind)],
            vec![node("Count", vec![Prop::I32(n as i32)])],
        )
    };
    let mut definitions = vec![
        node("Version", vec![Prop::I32(100)]),
        node(
            "Count",
            vec![Prop::I32(
                (1 + models + geometries + materials.len() + avatar_material + 2 * videos) as i32,
            )],
        ),
        count("GlobalSettings", 1),
        count("Model", models),
        count("Geometry", geometries),
        count("Material", materials.len() + avatar_material),
    ];
    if videos > 0 {
        definitions.push(count("Texture", videos));
        definitions.push(count("Video", videos));
    }
    let top = vec![
        with(
            "FBXHeaderExtension",
            vec![],
            vec![
                node("FBXHeaderVersion", vec![Prop::I32(1003)]),
                node("FBXVersion", vec![Prop::I32(VERSION as i32)]),
                node("Creator", vec![s("ClothCraft")]),
            ],
        ),
        node("Creator", vec![s("ClothCraft")]),
        with(
            "GlobalSettings",
            vec![],
            vec![
                node("Version", vec![Prop::I32(1000)]),
                properties(vec![
                    p("UpAxis", "int", "Integer", "", vec![Prop::I32(1)]),
                    p("UpAxisSign", "int", "Integer", "", vec![Prop::I32(1)]),
                    p("FrontAxis", "int", "Integer", "", vec![Prop::I32(2)]),
                    p("FrontAxisSign", "int", "Integer", "", vec![Prop::I32(1)]),
                    p("CoordAxis", "int", "Integer", "", vec![Prop::I32(0)]),
                    p("CoordAxisSign", "int", "Integer", "", vec![Prop::I32(1)]),
                    p(
                        "UnitScaleFactor",
                        "double",
                        "Number",
                        "",
                        vec![Prop::F64(1.)],
                    ),
                    p(
                        "OriginalUnitScaleFactor",
                        "double",
                        "Number",
                        "",
                        vec![Prop::F64(1.)],
                    ),
                ]),
            ],
        ),
        with("Documents", vec![], vec![node("Count", vec![Prop::I32(0)])]),
        node("References", vec![]),
        with("Definitions", vec![], definitions),
        with("Objects", vec![], objects),
        with("Connections", vec![], connections),
    ];
    let mut out = Vec::new();
    out.extend_from_slice(b"Kaydara FBX Binary  \x00\x1a\x00");
    out.extend_from_slice(&VERSION.to_le_bytes());
    for n in &top {
        n.write(&mut out);
    }
    out.extend_from_slice(&[0u8; 13]);
    // Footer: an identifier, padding to a 16-byte boundary, the version, and a closing magic.
    out.extend_from_slice(&[
        0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26,
        0x7e,
    ]);
    let pad = match out.len() % 16 {
        0 => 16,
        r => 16 - r,
    };
    out.resize(out.len() + pad, 0);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.resize(out.len() + 120, 0);
    out.extend_from_slice(&[
        0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29,
        0x0b,
    ]);
    Ok((out, textures))
}

/// Write `path` and copy the fabric texture images next to it.
pub fn write(scene: &FbxScene, path: &Path) -> Result<(), String> {
    let stem = path
        .file_stem()
        .ok_or("The file needs a name")?
        .to_string_lossy()
        .into_owned();
    let (bytes, textures) = fbx(scene, &stem)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    for (name, source) in textures {
        std::fs::copy(&source, dir.join(&name))
            .map_err(|e| format!("{}: {e}", source.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
