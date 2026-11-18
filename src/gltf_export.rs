//! glTF 2.0 export of the garment, optionally with the avatar and a recorded take.
//!
//! Independently implemented from the Khronos glTF 2.0.1 specification:
//! https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
//!
//! The garment is one mesh with a primitive per fabric. Each fabric becomes a metallic-
//! roughness material with its colour and roughness, double-sided like cloth, and its image
//! texture (embedded) when it has one; texture coordinates then count image repeats along the
//! grain, otherwise they are the pattern's warp and weft in metres. Triangles are wound so
//! the fabric's front face (opposite the solver's winding normal, or along it on a flipped
//! piece) is glTF's front face, as the editor draws it. Vertices stay split along seams, each
//! piece keeping its own texture coordinates. Units are metres, Y up.
//!
//! A take becomes morph targets, one per frame after the first, and an animation of the
//! mesh's weights with one key per frame: blending linearly between neighbouring targets is
//! exactly the linear interpolation of the cloth between frames. The avatar, when included,
//! moves the same way.
use crate::{
    avatar::SurfaceMesh,
    avatar_animation::Take,
    garment::{CompiledGarment, GarmentTemplate},
    model::State,
};
use glam::{DVec2, DVec3};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

/// What to write.
pub struct GltfScene<'a> {
    pub template: &'a GarmentTemplate,
    pub garment: &'a CompiledGarment,
    /// The cloth as it is shown; with a take, its first frame is used instead.
    pub state: &'a State,
    /// Image files of fabric textures, by asset ID.
    pub images: &'a [(String, PathBuf)],
    /// The avatar (and scene objects) as one surface.
    pub avatar: Option<&'a SurfaceMesh>,
    /// A recorded take of the cloth, and the avatar's vertices at each of its frames.
    pub take: Option<(&'a Take, Option<Vec<Vec<DVec3>>>)>,
}

/// Binary payload and the JSON that indexes it.
struct Builder {
    json: Value,
    bin: Vec<u8>,
}

impl Builder {
    fn new() -> Self {
        Self {
            json: json!({
                "asset": {"version": "2.0", "generator": "ClothCraft"},
                "scene": 0,
                "scenes": [{"nodes": []}],
                "nodes": [],
                "meshes": [],
                "materials": [],
                "accessors": [],
                "bufferViews": [],
                "buffers": [],
            }),
            bin: Vec::new(),
        }
    }

    fn push(&mut self, key: &str, value: Value) -> usize {
        let list = self.json[key].as_array_mut().unwrap();
        list.push(value);
        list.len() - 1
    }

    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut view = json!({"buffer": 0, "byteOffset": offset, "byteLength": bytes.len()});
        if let Some(target) = target {
            view["target"] = json!(target);
        }
        self.push("bufferViews", view)
    }

    /// Float vectors of `width` components; positions get the bounds the format requires.
    fn floats(&mut self, data: &[f32], width: usize, bounds: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(ARRAY_BUFFER));
        let kind = ["SCALAR", "VEC2", "VEC3", "VEC4"][width - 1];
        let mut accessor = json!({
            "bufferView": view, "componentType": FLOAT, "count": data.len() / width, "type": kind,
        });
        if bounds {
            let mut lo = vec![f32::INFINITY; width];
            let mut hi = vec![f32::NEG_INFINITY; width];
            for chunk in data.chunks(width) {
                for (k, v) in chunk.iter().enumerate() {
                    lo[k] = lo[k].min(*v);
                    hi[k] = hi[k].max(*v);
                }
            }
            accessor["min"] = json!(lo);
            accessor["max"] = json!(hi);
        }
        self.push("accessors", accessor)
    }

    /// Animation inputs and outputs: scalars, not vertex data.
    fn scalars(&mut self, data: &[f32], bounds: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, None);
        let mut accessor = json!({
            "bufferView": view, "componentType": FLOAT, "count": data.len(), "type": "SCALAR",
        });
        if bounds {
            let lo = data.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            accessor["min"] = json!([lo]);
            accessor["max"] = json!([hi]);
        }
        self.push("accessors", accessor)
    }

    fn indices(&mut self, data: &[u32]) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(ELEMENT_ARRAY_BUFFER));
        self.push(
            "accessors",
            json!({"bufferView": view, "componentType": UNSIGNED_INT, "count": data.len(), "type": "SCALAR"}),
        )
    }

    /// Morph targets, one per frame after the first, displacing the base positions.
    fn targets(&mut self, base: &[DVec3], frames: &[Vec<DVec3>]) -> Vec<Value> {
        frames
            .iter()
            .skip(1)
            .map(|frame| {
                let delta: Vec<f32> = frame
                    .iter()
                    .zip(base)
                    .flat_map(|(p, b)| (*p - *b).as_vec3().to_array())
                    .collect();
                json!({"POSITION": self.floats(&delta, 3, true)})
            })
            .collect()
    }
}

/// Each vertex's texture coordinates: image repeats along the grain on a textured fabric,
/// otherwise the pattern's warp and weft in metres.
pub fn texture_coordinates(template: &GarmentTemplate, garment: &CompiledGarment) -> Vec<DVec2> {
    let appearances = crate::garment::appearance::bind(template, garment);
    let mut uv = vec![DVec2::ZERO; garment.mesh.uv.len()];
    for panel in &garment.panels {
        let material = &garment.materials[panel.material];
        let (sin, cos) = material.grain_degrees.to_radians().sin_cos();
        let texture = appearances[panel.material]
            .as_ref()
            .and_then(|a| a.texture.as_ref());
        for i in panel.vertices.clone() {
            let p = garment.mesh.uv[i];
            let warp_weft = DVec2::new(p.x * cos + p.y * sin, -p.x * sin + p.y * cos);
            uv[i] = texture.map_or(warp_weft, |t| t.image_uv(warp_weft));
        }
    }
    uv
}

/// The garment's triangles wound counter-clockwise seen from the fabric's front face, which
/// is opposite the solver's winding normal except on flipped pieces.
pub fn front_triangles(template: &GarmentTemplate, garment: &CompiledGarment) -> Vec<[usize; 3]> {
    let mut flipped = vec![false; garment.mesh.triangles.len()];
    for panel in &garment.panels {
        if template.flipped_panels.contains(&panel.id) {
            flipped[panel.triangles.clone()].fill(true);
        }
    }
    garment
        .mesh
        .triangles
        .iter()
        .zip(&flipped)
        .map(|(t, &flip)| {
            let [a, b, c] = t.vertices;
            if flip { [a, b, c] } else { [a, c, b] }
        })
        .collect()
}

/// Outward vertex normals: area weighted, from triangles wound outwards.
fn normals(points: &[DVec3], triangles: &[[u32; 3]]) -> Vec<f32> {
    let mut n = vec![DVec3::ZERO; points.len()];
    for t in triangles {
        let [a, b, c] = t.map(|i| points[i as usize]);
        let face = (b - a).cross(c - a);
        for i in t {
            n[*i as usize] += face;
        }
    }
    n.iter()
        .flat_map(|v| v.normalize_or(DVec3::Y).as_vec3().to_array())
        .collect()
}

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "image/png",
    }
}

fn build(scene: &GltfScene) -> Result<Builder, String> {
    let garment = scene.garment;
    let n = garment.mesh.uv.len();
    if scene.state.x.len() != n {
        return Err("The cloth does not match the garment".into());
    }
    let take = scene
        .take
        .as_ref()
        .map(|(take, bodies)| (*take, bodies.as_ref()));
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
    let cloth_frames: Option<Vec<Vec<DVec3>>> =
        take.map(|(t, _)| (0..t.frames.len()).map(|k| t.positions(k)).collect());
    let base: Vec<DVec3> = cloth_frames
        .as_ref()
        .map_or_else(|| scene.state.x.clone(), |f| f[0].clone());
    let appearances = crate::garment::appearance::bind(scene.template, garment);
    // Fabric of each material slot, from the pieces that use it.
    let mut fabric_of = vec![None; garment.materials.len()];
    for panel in &garment.panels {
        if let Some(source) = scene.template.panels.iter().find(|p| p.id == panel.id) {
            fabric_of[panel.material] = Some(source.fabric.clone());
        }
    }
    let mut b = Builder::new();
    let uv: Vec<[f32; 2]> = texture_coordinates(scene.template, garment)
        .iter()
        .map(|p| p.as_vec2().to_array())
        .collect();
    let triangles: Vec<[u32; 3]> = front_triangles(scene.template, garment)
        .into_iter()
        .map(|t| t.map(|i| i as u32))
        .collect();
    let positions: Vec<f32> = base.iter().flat_map(|p| p.as_vec3().to_array()).collect();
    let position = b.floats(&positions, 3, true);
    let normal = b.floats(&normals(&base, &triangles), 3, false);
    let texcoord = b.floats(&uv.concat(), 2, false);
    let targets = cloth_frames
        .as_ref()
        .map(|frames| b.targets(&base, frames))
        .unwrap_or_default();
    let mut image_index: Vec<(String, usize)> = Vec::new();
    let mut primitives = Vec::new();
    for (slot, fabric) in fabric_of.iter().enumerate() {
        let indices: Vec<u32> = garment
            .mesh
            .triangles
            .iter()
            .zip(&triangles)
            .filter(|(t, _)| t.material == slot)
            .flat_map(|(_, w)| *w)
            .collect();
        if indices.is_empty() {
            continue;
        }
        let appearance = appearances[slot].as_ref();
        let color = appearance.map_or([0.91, 0.92, 0.94], |a| a.linear_color());
        let mut pbr = json!({
            "baseColorFactor": [color[0], color[1], color[2], 1.0],
            "metallicFactor": 0.0,
            "roughnessFactor": appearance.map_or(0.8, |a| a.roughness),
        });
        if let Some(texture) = appearance.and_then(|a| a.texture.as_ref()) {
            let image = match image_index.iter().find(|(id, _)| *id == texture.asset_id) {
                Some((_, k)) => *k,
                None => {
                    let path = scene
                        .images
                        .iter()
                        .find(|(id, _)| *id == texture.asset_id)
                        .map(|(_, p)| p)
                        .ok_or_else(|| format!("Texture image {} is missing", texture.asset_id))?;
                    let bytes =
                        std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
                    let view = b.view(&bytes, None);
                    if b.json.get("samplers").is_none() {
                        b.json["samplers"] = json!([{"wrapS": 10497, "wrapT": 10497}]);
                        b.json["textures"] = json!([]);
                        b.json["images"] = json!([]);
                    }
                    let image = b.push(
                        "images",
                        json!({"bufferView": view, "mimeType": mime(path), "name": texture.asset_id}),
                    );
                    let k = b.push("textures", json!({"sampler": 0, "source": image}));
                    image_index.push((texture.asset_id.clone(), k));
                    k
                }
            };
            pbr["baseColorTexture"] = json!({"index": image});
        }
        let material = b.push(
            "materials",
            json!({
                "name": fabric.clone().unwrap_or_else(|| format!("fabric {slot}")),
                "pbrMetallicRoughness": pbr,
                "doubleSided": true,
            }),
        );
        let indices = b.indices(&indices);
        let mut primitive = json!({
            "attributes": {"POSITION": position, "NORMAL": normal, "TEXCOORD_0": texcoord},
            "indices": indices,
            "material": material,
            "mode": 4,
        });
        if !targets.is_empty() {
            primitive["targets"] = json!(targets);
        }
        primitives.push(primitive);
    }
    let mut garment_mesh = json!({"name": "garment", "primitives": primitives});
    if !targets.is_empty() {
        garment_mesh["weights"] = json!(vec![0.0; targets.len()]);
    }
    let mesh = b.push("meshes", garment_mesh);
    let node = b.push("nodes", json!({"name": "garment", "mesh": mesh}));
    b.json["scenes"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!(node));
    let mut animated = vec![(node, targets.len())];
    if let Some(avatar) = scene.avatar {
        let frames = take.and_then(|(_, bodies)| bodies);
        let base: Vec<DVec3> = frames.map_or_else(|| avatar.vertices.clone(), |f| f[0].clone());
        let triangles: Vec<[u32; 3]> = avatar
            .triangles
            .iter()
            .map(|t| t.map(|i| i as u32))
            .collect();
        let positions: Vec<f32> = base.iter().flat_map(|p| p.as_vec3().to_array()).collect();
        let position = b.floats(&positions, 3, true);
        let normal = b.floats(&normals(&base, &triangles), 3, false);
        let targets = frames.map(|f| b.targets(&base, f)).unwrap_or_default();
        let material = b.push(
            "materials",
            json!({
                "name": "avatar",
                "pbrMetallicRoughness": {"baseColorFactor": [0.45, 0.45, 0.47, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.7},
            }),
        );
        let indices = b.indices(&triangles.concat());
        let mut primitive = json!({
            "attributes": {"POSITION": position, "NORMAL": normal},
            "indices": indices, "material": material, "mode": 4,
        });
        let mut mesh = json!({"name": "avatar"});
        if !targets.is_empty() {
            primitive["targets"] = json!(targets);
            mesh["weights"] = json!(vec![0.0; targets.len()]);
        }
        mesh["primitives"] = json!([primitive]);
        let mesh = b.push("meshes", mesh);
        let node = b.push("nodes", json!({"name": "avatar", "mesh": mesh}));
        b.json["scenes"][0]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!(node));
        if frames.is_some() {
            animated.push((node, targets.len()));
        }
    }
    if let Some((take, _)) = take
        && take.frames.len() > 1
    {
        let frames = take.frames.len();
        let times: Vec<f32> = (0..frames).map(|k| (k as f64 / take.fps) as f32).collect();
        let input = b.scalars(&times, true);
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for (node, count) in animated {
            if count == 0 {
                continue;
            }
            // Frame k shows target k - 1 fully; frame 0 shows none.
            let mut weights = vec![0f32; frames * count];
            for k in 1..frames {
                weights[k * count + k - 1] = 1.;
            }
            let output = b.scalars(&weights, false);
            samplers.push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
            channels.push(
                json!({"sampler": samplers.len() - 1, "target": {"node": node, "path": "weights"}}),
            );
        }
        b.json["animations"] =
            json!([{"name": "take", "samplers": samplers, "channels": channels}]);
    }
    Ok(b)
}

/// The scene as a single binary GLB file.
pub fn glb(scene: &GltfScene) -> Result<Vec<u8>, String> {
    let mut b = build(scene)?;
    while !b.bin.len().is_multiple_of(4) {
        b.bin.push(0);
    }
    b.json["buffers"] = json!([{"byteLength": b.bin.len()}]);
    let mut text = serde_json::to_vec(&b.json).map_err(|e| e.to_string())?;
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + b.bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(b.bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&b.bin);
    Ok(out)
}

/// The scene as glTF JSON referring to a binary buffer file named `bin_name`, and that
/// buffer's bytes.
pub fn gltf(scene: &GltfScene, bin_name: &str) -> Result<(String, Vec<u8>), String> {
    let mut b = build(scene)?;
    b.json["buffers"] = json!([{"byteLength": b.bin.len(), "uri": bin_name}]);
    let text = serde_json::to_string_pretty(&b.json).map_err(|e| e.to_string())?;
    Ok((text, b.bin))
}

/// Write the scene to `path`: GLB unless the extension is `.gltf`, in which case the buffer
/// goes next to it with the same name and `.bin`.
pub fn write(scene: &GltfScene, path: &Path) -> Result<(), String> {
    let gltf_json = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gltf"));
    if gltf_json {
        let bin = path.with_extension("bin");
        let name = bin
            .file_name()
            .ok_or("The file needs a name")?
            .to_string_lossy()
            .into_owned();
        let (text, bytes) = gltf(scene, &name)?;
        std::fs::write(&bin, bytes).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    } else {
        std::fs::write(path, glb(scene)?).map_err(|e| e.to_string())
    }
}
