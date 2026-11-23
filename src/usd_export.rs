//! USD export: a text layer (`.usda`), or a USDZ package holding it with the fabric
//! textures, as Apple's Quick Look and AR viewers read.
//!
//! Written from the public USD documentation (the `usda` text syntax, `UsdGeomMesh`,
//! `UsdPreviewSurface`, `UsdUVTexture`) and the USDZ file format specification: an
//! uncompressed zip whose first file is the root layer and whose files start on 64-byte
//! boundaries.
//!
//! Each garment piece is a mesh with its fabric's preview-surface material, its texture's maps
//! (normal, roughness, metalness, opacity, occlusion, displacement) wired into the surface's
//! inputs through texture readers whose scale and bias flip a DirectX normal map's green and
//! invert a glossiness map; images that are not PNG or JPEG are re-encoded as PNG. Triangles are
//! counter-clockwise seen from the fabric's front face, USD's default orientation, and
//! marked double-sided. The avatar is one more mesh. A take animates the points with one
//! time sample per frame, at the take's frame rate. Units are metres, Y up.
use crate::{
    avatar::SurfaceMesh,
    avatar_animation::Take,
    garment::{CompiledGarment, GarmentTemplate, appearance::MapSlot},
    gltf_export::{front_triangles, texture_coordinates},
    model::State,
    texture_maps::{self, TextureFile},
};
use glam::DVec3;
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// What to write.
pub struct UsdScene<'a> {
    pub template: &'a GarmentTemplate,
    pub garment: &'a CompiledGarment,
    pub state: &'a State,
    /// Image files of fabric textures, by asset ID.
    pub images: &'a [(String, PathBuf)],
    pub avatar: Option<&'a SurfaceMesh>,
    /// A recorded take of the cloth, and the avatar's vertices at each of its frames.
    pub take: Option<(&'a Take, Option<Vec<Vec<DVec3>>>)>,
}

/// A USD identifier from any name: letters, digits and underscores, not starting with a
/// digit.
fn identifier(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn points(out: &mut String, p: impl Iterator<Item = DVec3>) {
    out.push('[');
    for (k, p) in p.enumerate() {
        if k > 0 {
            out.push_str(", ");
        }
        write!(out, "({:.5}, {:.5}, {:.5})", p.x, p.y, p.z).unwrap();
    }
    out.push(']');
}

fn normals(x: &[DVec3], triangles: &[[usize; 3]]) -> Vec<DVec3> {
    let mut n = vec![DVec3::ZERO; x.len()];
    for t in triangles {
        let face = (x[t[1]] - x[t[0]]).cross(x[t[2]] - x[t[0]]);
        for i in t {
            n[*i] += face;
        }
    }
    n.iter().map(|v| v.normalize_or(DVec3::Y)).collect()
}

/// One mesh prim: its points (animated when frames are given), normals, texture
/// coordinates and material.
#[allow(clippy::too_many_arguments)]
fn mesh(
    out: &mut String,
    name: &str,
    triangles: &[[usize; 3]],
    rest: &[DVec3],
    frames: Option<&[Vec<DVec3>]>,
    st: Option<&[[f64; 2]]>,
    material: &str,
    double_sided: bool,
) {
    writeln!(
        out,
        "    def Mesh \"{name}\" (\n        prepend apiSchemas = [\"MaterialBindingAPI\"]\n    )\n    {{"
    )
    .unwrap();
    write!(out, "        int[] faceVertexCounts = [").unwrap();
    for k in 0..triangles.len() {
        out.push_str(if k == 0 { "3" } else { ", 3" });
    }
    out.push_str("]\n        int[] faceVertexIndices = [");
    for (k, i) in triangles.iter().flatten().enumerate() {
        if k > 0 {
            out.push_str(", ");
        }
        write!(out, "{i}").unwrap();
    }
    out.push_str("]\n        point3f[] points = ");
    points(out, rest.iter().copied());
    out.push('\n');
    if let Some(frames) = frames {
        out.push_str("        point3f[] points.timeSamples = {\n");
        for (k, frame) in frames.iter().enumerate() {
            write!(out, "            {k}: ").unwrap();
            points(out, frame.iter().copied());
            out.push_str(",\n");
        }
        out.push_str("        }\n");
    }
    out.push_str("        normal3f[] normals = ");
    points(out, normals(rest, triangles).into_iter());
    out.push_str(" (\n            interpolation = \"vertex\"\n        )\n");
    if let Some(st) = st {
        out.push_str("        texCoord2f[] primvars:st = [");
        for (k, [u, v]) in st.iter().enumerate() {
            if k > 0 {
                out.push_str(", ");
            }
            write!(out, "({u:.6}, {v:.6})").unwrap();
        }
        out.push_str("] (\n            interpolation = \"vertex\"\n        )\n");
    }
    writeln!(out, "        uniform token subdivisionScheme = \"none\"").unwrap();
    if double_sided {
        writeln!(out, "        uniform bool doubleSided = 1").unwrap();
    }
    writeln!(
        out,
        "        rel material:binding = </Root/Materials/{material}>\n    }}"
    )
    .unwrap();
}

/// A map feeding one of the preview surface's inputs.
struct MapInput {
    /// The surface input: `normal`, `roughness`, `metallic`, `opacity`, `occlusion`,
    /// `displacement`.
    input: &'static str,
    /// The image in the package.
    file: String,
    /// What the texture reader does to the image's values: `value × scale + bias`.
    scale: [f64; 4],
    bias: [f64; 4],
}

/// How a material looks: its colours and values, and the images that replace them.
struct Look {
    color: [f32; 3],
    roughness: f64,
    metalness: f64,
    /// The colour image, tinted by `color`.
    texture: Option<String>,
    maps: Vec<MapInput>,
}

impl Look {
    fn plain(color: [f32; 3], roughness: f64) -> Self {
        Self {
            color,
            roughness,
            metalness: 0.,
            texture: None,
            maps: vec![],
        }
    }
}

/// A texture reader of the material at `path`, named `prim`, repeating over the pieces.
fn uv_texture(out: &mut String, path: &str, prim: &str, file: &str, data: bool, extra: &str) {
    writeln!(
        out,
        "            def Shader \"{prim}\"\n            {{\n                uniform token info:id = \"UsdUVTexture\"\n                asset inputs:file = @{file}@\n                float2 inputs:st.connect = <{path}/Coordinates.outputs:result>\n                token inputs:wrapS = \"repeat\"\n                token inputs:wrapT = \"repeat\"{}{extra}\n                float3 outputs:rgb\n                float outputs:r\n            }}",
        if data {
            "\n                token inputs:sourceColorSpace = \"raw\""
        } else {
            ""
        }
    )
    .unwrap();
}

fn vec4(v: [f64; 4]) -> String {
    format!("({:.4}, {:.4}, {:.4}, {:.4})", v[0], v[1], v[2], v[3])
}

/// A preview-surface material, its inputs read from images in the package where it has them.
fn material(out: &mut String, name: &str, look: &Look) {
    let path = format!("/Root/Materials/{name}");
    let color = look.color;
    writeln!(out, "        def Material \"{name}\"\n        {{").unwrap();
    writeln!(
        out,
        "            token outputs:surface.connect = <{path}/Surface.outputs:surface>"
    )
    .unwrap();
    if look.maps.iter().any(|m| m.input == "displacement") {
        writeln!(
            out,
            "            token outputs:displacement.connect = <{path}/Surface.outputs:displacement>"
        )
        .unwrap();
    }
    writeln!(
        out,
        "            def Shader \"Surface\"\n            {{\n                uniform token info:id = \"UsdPreviewSurface\""
    )
    .unwrap();
    match look.texture {
        Some(_) => writeln!(
            out,
            "                color3f inputs:diffuseColor.connect = <{path}/Image.outputs:rgb>"
        )
        .unwrap(),
        None => writeln!(
            out,
            "                color3f inputs:diffuseColor = ({:.4}, {:.4}, {:.4})",
            color[0], color[1], color[2]
        )
        .unwrap(),
    }
    let mapped = |input: &str| look.maps.iter().any(|m| m.input == input);
    if !mapped("roughness") {
        writeln!(
            out,
            "                float inputs:roughness = {:.3}",
            look.roughness
        )
        .unwrap();
    }
    if !mapped("metallic") {
        writeln!(
            out,
            "                float inputs:metallic = {:.3}",
            look.metalness
        )
        .unwrap();
    }
    for (k, map) in look.maps.iter().enumerate() {
        let (kind, output) = match map.input {
            "normal" => ("normal3f", "rgb"),
            _ => ("float", "r"),
        };
        writeln!(
            out,
            "                {kind} inputs:{}.connect = <{path}/Map{k}.outputs:{output}>",
            map.input
        )
        .unwrap();
        if map.input == "opacity" {
            // Lace and mesh: cut away where the opacity is under one half.
            writeln!(out, "                float inputs:opacityThreshold = 0.5").unwrap();
        }
    }
    writeln!(out, "                token outputs:surface").unwrap();
    if mapped("displacement") {
        writeln!(out, "                token outputs:displacement").unwrap();
    }
    writeln!(out, "            }}").unwrap();
    if look.texture.is_some() || !look.maps.is_empty() {
        writeln!(
            out,
            "            def Shader \"Coordinates\"\n            {{\n                uniform token info:id = \"UsdPrimvarReader_float2\"\n                string inputs:varname = \"st\"\n                float2 outputs:result\n            }}"
        )
        .unwrap();
    }
    if let Some(file) = &look.texture {
        let tint = format!(
            "\n                float4 inputs:scale = ({:.4}, {:.4}, {:.4}, 1)",
            color[0], color[1], color[2]
        );
        uv_texture(out, &path, "Image", file, false, &tint);
    }
    for (k, map) in look.maps.iter().enumerate() {
        let extra = format!(
            "\n                float4 inputs:scale = {}\n                float4 inputs:bias = {}",
            vec4(map.scale),
            vec4(map.bias)
        );
        uv_texture(out, &path, &format!("Map{k}"), &map.file, true, &extra);
    }
    writeln!(out, "        }}").unwrap();
}

/// The texture images a fabric's maps feed into its preview surface, adding the files to
/// `textures`. Normal maps are read the OpenGL way, a DirectX one by flipping its green with
/// the reader's scale and bias; a glossiness map is inverted the same way.
fn map_inputs(
    texture: &crate::garment::appearance::FabricTexture,
    images: &[(String, PathBuf)],
    textures: &mut Vec<(String, TextureFile)>,
) -> Result<Vec<MapInput>, String> {
    let mut inputs = vec![];
    for slot in MapSlot::ALL {
        let Some(asset) = texture.map(slot) else {
            continue;
        };
        let file = texture_maps::plain(texture_maps::source(images, asset)?, true)?;
        let name = match &file {
            TextureFile::Original(_) => {
                format!("textures/{}.{}", identifier(asset), file.extension())
            }
            TextureFile::Converted { .. } => format!("textures/{}.png", identifier(asset)),
        };
        if !textures.iter().any(|(f, _)| *f == name) {
            textures.push((name.clone(), file));
        }
        let identity = ([1.; 4], [0.; 4]);
        let (input, (scale, bias)) = match slot {
            MapSlot::Normal if texture.normal_directx => {
                ("normal", ([2., -2., 2., 1.], [-1., 1., -1., 0.]))
            }
            MapSlot::Normal => ("normal", ([2., 2., 2., 1.], [-1., -1., -1., 0.])),
            MapSlot::Roughness if texture.roughness_is_glossiness => {
                ("roughness", ([-1., -1., -1., 1.], [1., 1., 1., 0.]))
            }
            MapSlot::Roughness => ("roughness", identity),
            MapSlot::Metalness => ("metallic", identity),
            MapSlot::Opacity => ("opacity", identity),
            MapSlot::Occlusion => ("occlusion", identity),
            // Metres along the normal: the stage's unit is the metre.
            MapSlot::Displacement => {
                let h = texture.displacement_mm / 1000.;
                ("displacement", ([h, h, h, 1.], [0.; 4]))
            }
        };
        inputs.push(MapInput {
            input,
            file: name,
            scale,
            bias,
        });
    }
    Ok(inputs)
}

/// The layer's text and the texture images it refers to, by the name it uses for each.
pub fn usda(scene: &UsdScene) -> Result<(String, Vec<(String, TextureFile)>), String> {
    let garment = scene.garment;
    let n = garment.mesh.uv.len();
    if scene.state.x.len() != n {
        return Err("The cloth does not match the garment".into());
    }
    let take = scene.take.as_ref().map(|(t, b)| (*t, b.as_ref()));
    if let Some((take, bodies)) = take {
        if take.frames.is_empty() || take.frames[0].len() != n {
            return Err("The take was recorded on another mesh".into());
        }
        if let (Some(bodies), Some(avatar)) = (bodies, scene.avatar)
            && (bodies.len() != take.frames.len()
                || bodies.iter().any(|b| b.len() != avatar.vertices.len()))
        {
            return Err("The avatar's frames do not match the take".into());
        }
    }
    let cloth: Option<Vec<Vec<DVec3>>> =
        take.map(|(t, _)| (0..t.frames.len()).map(|k| t.positions(k)).collect());
    let base: Vec<DVec3> = cloth
        .as_ref()
        .map_or_else(|| scene.state.x.clone(), |f| f[0].clone());
    let appearances = crate::garment::appearance::bind(scene.template, garment);
    let triangles = front_triangles(scene.template, garment);
    let uv = texture_coordinates(scene.template, garment);
    let mut out = String::from(
        "#usda 1.0\n(\n    defaultPrim = \"Root\"\n    metersPerUnit = 1\n    upAxis = \"Y\"\n",
    );
    if let Some((take, _)) = take {
        writeln!(
            out,
            "    startTimeCode = 0\n    endTimeCode = {}\n    timeCodesPerSecond = {}",
            take.frames.len() - 1,
            take.fps
        )
        .unwrap();
    }
    out.push_str("    doc = \"FashionCraft garment\"\n)\n\ndef Xform \"Root\"\n{\n");
    let mut materials: BTreeMap<String, (usize, Option<String>, Vec<MapInput>)> = BTreeMap::new();
    let mut textures: Vec<(String, TextureFile)> = Vec::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    for panel in &garment.panels {
        let fabric = scene
            .template
            .panels
            .iter()
            .find(|p| p.id == panel.id)
            .map_or_else(
                || format!("fabric_{}", panel.material),
                |p| p.fabric.clone(),
            );
        let material_name = identifier(&fabric);
        if !materials.contains_key(&material_name) {
            let fabric_texture = appearances[panel.material]
                .as_ref()
                .and_then(|a| a.texture.as_ref());
            let texture = match fabric_texture {
                Some(t) => {
                    let source = texture_maps::source(scene.images, &t.asset_id)?;
                    let file = texture_maps::plain(source, true)?;
                    let name = format!("textures/{}.{}", identifier(&t.asset_id), file.extension());
                    if !textures.iter().any(|(f, _)| *f == name) {
                        textures.push((name.clone(), file));
                    }
                    Some(name)
                }
                None => None,
            };
            let maps = match fabric_texture {
                Some(t) => map_inputs(t, scene.images, &mut textures)?,
                None => vec![],
            };
            materials.insert(material_name.clone(), (panel.material, texture, maps));
        }
        // Piece names may repeat once made identifiers: keep them apart.
        let mut name = identifier(&panel.id);
        let seen = names.entry(name.clone()).or_default();
        *seen += 1;
        if *seen > 1 {
            name = format!("{name}_{seen}");
        }
        let local: Vec<usize> = panel.vertices.clone().collect();
        let index: HashMap<usize, usize> = local.iter().enumerate().map(|(k, i)| (*i, k)).collect();
        let piece_triangles: Vec<[usize; 3]> = triangles[panel.triangles.clone()]
            .iter()
            .map(|t| t.map(|i| index[&i]))
            .collect();
        let rest: Vec<DVec3> = local.iter().map(|i| base[*i]).collect();
        let frames: Option<Vec<Vec<DVec3>>> = cloth.as_ref().map(|f| {
            f.iter()
                .map(|frame| local.iter().map(|i| frame[*i]).collect())
                .collect()
        });
        let st: Vec<[f64; 2]> = local.iter().map(|i| [uv[*i].x, uv[*i].y]).collect();
        mesh(
            &mut out,
            &name,
            &piece_triangles,
            &rest,
            frames.as_deref(),
            Some(&st),
            &material_name,
            true,
        );
    }
    if let Some(avatar) = scene.avatar {
        let frames = take.and_then(|(_, b)| b);
        let rest = frames.map_or_else(|| avatar.vertices.clone(), |f| f[0].clone());
        mesh(
            &mut out,
            "avatar",
            &avatar.triangles,
            &rest,
            frames.map(|f| f.as_slice()),
            None,
            "avatar",
            false,
        );
    }
    out.push_str("    def Scope \"Materials\"\n    {\n");
    for (name, (slot, texture, maps)) in materials {
        let appearance = appearances[slot].as_ref();
        let look = Look {
            color: appearance.map_or([0.91, 0.92, 0.94], |a| a.linear_color()),
            roughness: appearance.map_or(0.8, |a| a.roughness),
            metalness: appearance.map_or(0., |a| a.metalness),
            texture,
            maps,
        };
        material(&mut out, &name, &look);
    }
    if scene.avatar.is_some() {
        material(&mut out, "avatar", &Look::plain([0.45, 0.45, 0.47], 0.7));
    }
    out.push_str("    }\n}\n");
    Ok((out, textures))
}

/// CRC-32 (IEEE 802.3), as zip records it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in bytes {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// An uncompressed zip whose files' data start on 64-byte boundaries, in the given order.
pub fn usdz_package(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let crc = crc32(data);
        let offset = out.len();
        let header = 30 + name.len();
        // Pad the extra field so the data starts on a 64-byte boundary (4 bytes of id and
        // size, then zeros).
        let mut extra = (64 - (offset + header) % 64) % 64;
        if extra > 0 && extra < 4 {
            extra += 64;
        }
        let size = u32::try_from(data.len()).map_err(|_| "A file is too large for USDZ")?;
        let mut local = Vec::with_capacity(header + extra);
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(&0x21u16.to_le_bytes());
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&(extra as u16).to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        if extra > 0 {
            local.extend_from_slice(&0x1986u16.to_le_bytes());
            local.extend_from_slice(&((extra - 4) as u16).to_le_bytes());
            local.resize(local.len() + extra - 4, 0);
        }
        out.extend_from_slice(&local);
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0x21u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&(offset as u32).to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len();
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&(start as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

/// Write `path`: a USDZ package when the extension is `.usdz`, otherwise a `.usda` text
/// layer with its textures copied into a `textures` folder beside it.
pub fn write(scene: &UsdScene, path: &Path) -> Result<(), String> {
    let (text, textures) = usda(scene)?;
    let package = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("usdz"));
    if package {
        let stem = path
            .file_stem()
            .ok_or("The file needs a name")?
            .to_string_lossy()
            .into_owned();
        let mut files = vec![(format!("{stem}.usda"), text.into_bytes())];
        for (name, file) in textures {
            files.push((name, file.bytes()?));
        }
        std::fs::write(path, usdz_package(&files)?).map_err(|e| e.to_string())
    } else {
        let dir = path.parent().unwrap_or(Path::new("."));
        for (name, file) in textures {
            let target = dir.join(&name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            file.write(&target)?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }
}
