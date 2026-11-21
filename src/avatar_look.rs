//! Avatar looks: a skin image on the body and parts fitted to it (eyes, eyebrows, eyelashes,
//! hair), from MakeHuman's CC0 assets in `assets/avatars` (see its `provenance.json`).
//!
//! A part is fitted as MakeHuman fits its proxies (`.mhclo`): each of its vertices is a base-mesh
//! vertex, or a weighted sum of three plus an offset scaled by the body's own size along each
//! axis, so it follows any body made from the same base mesh. A look is kept beside its avatar's
//! OBJ as `<stem>.look.json` (or `look.json`): the skin, and every part already fitted, with its
//! mesh, texture coordinates and image. Images are the bundled ones (`builtin:…`) or files beside
//! the look.
use crate::avatar::SurfaceMesh;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const BASE_OBJ: &str = include_str!("../assets/makehuman/base.obj");
/// The base mesh's body vertices.
const BODY: usize = crate::avatar_maker::BODY_VERTICES;
/// Tag of a look file.
pub const FORMAT: &str = "fashioncraft-avatar-look";

/// Images and fittings bundled with the program, by their path below `assets/avatars`.
const BUNDLED: &[(&str, &[u8])] = &[
    (
        "skins/young_caucasian_female.jpg",
        include_bytes!("../assets/avatars/skins/young_caucasian_female.jpg"),
    ),
    (
        "skins/young_caucasian_male.jpg",
        include_bytes!("../assets/avatars/skins/young_caucasian_male.jpg"),
    ),
    (
        "skins/young_african_female.jpg",
        include_bytes!("../assets/avatars/skins/young_african_female.jpg"),
    ),
    (
        "skins/young_african_male.jpg",
        include_bytes!("../assets/avatars/skins/young_african_male.jpg"),
    ),
    (
        "skins/young_asian_female.jpg",
        include_bytes!("../assets/avatars/skins/young_asian_female.jpg"),
    ),
    (
        "skins/young_asian_male.jpg",
        include_bytes!("../assets/avatars/skins/young_asian_male.jpg"),
    ),
    (
        "skins/middleage_caucasian_female.jpg",
        include_bytes!("../assets/avatars/skins/middleage_caucasian_female.jpg"),
    ),
    (
        "skins/middleage_caucasian_male.jpg",
        include_bytes!("../assets/avatars/skins/middleage_caucasian_male.jpg"),
    ),
    (
        "skins/middleage_african_male.jpg",
        include_bytes!("../assets/avatars/skins/middleage_african_male.jpg"),
    ),
    (
        "skins/middleage_asian_female.jpg",
        include_bytes!("../assets/avatars/skins/middleage_asian_female.jpg"),
    ),
    (
        "eyes/brown.png",
        include_bytes!("../assets/avatars/eyes/brown.png"),
    ),
    (
        "eyes/brownlight.png",
        include_bytes!("../assets/avatars/eyes/brownlight.png"),
    ),
    (
        "eyes/blue.png",
        include_bytes!("../assets/avatars/eyes/blue.png"),
    ),
    (
        "eyes/green.png",
        include_bytes!("../assets/avatars/eyes/green.png"),
    ),
    (
        "eyes/grey.png",
        include_bytes!("../assets/avatars/eyes/grey.png"),
    ),
    (
        "parts/eyes/low-poly/fit.mhclo",
        include_bytes!("../assets/avatars/parts/eyes/low-poly/fit.mhclo"),
    ),
    (
        "parts/eyes/low-poly/mesh.obj",
        include_bytes!("../assets/avatars/parts/eyes/low-poly/mesh.obj"),
    ),
    (
        "parts/eyebrows/eyebrow001/fit.mhclo",
        include_bytes!("../assets/avatars/parts/eyebrows/eyebrow001/fit.mhclo"),
    ),
    (
        "parts/eyebrows/eyebrow001/mesh.obj",
        include_bytes!("../assets/avatars/parts/eyebrows/eyebrow001/mesh.obj"),
    ),
    (
        "parts/eyebrows/eyebrow001/image.png",
        include_bytes!("../assets/avatars/parts/eyebrows/eyebrow001/image.png"),
    ),
    (
        "parts/eyebrows/eyebrow010/fit.mhclo",
        include_bytes!("../assets/avatars/parts/eyebrows/eyebrow010/fit.mhclo"),
    ),
    (
        "parts/eyebrows/eyebrow010/mesh.obj",
        include_bytes!("../assets/avatars/parts/eyebrows/eyebrow010/mesh.obj"),
    ),
    (
        "parts/eyebrows/eyebrow010/image.png",
        include_bytes!("../assets/avatars/parts/eyebrows/eyebrow010/image.png"),
    ),
    (
        "parts/eyelashes/eyelashes01/fit.mhclo",
        include_bytes!("../assets/avatars/parts/eyelashes/eyelashes01/fit.mhclo"),
    ),
    (
        "parts/eyelashes/eyelashes01/mesh.obj",
        include_bytes!("../assets/avatars/parts/eyelashes/eyelashes01/mesh.obj"),
    ),
    (
        "parts/eyelashes/eyelashes01/image.png",
        include_bytes!("../assets/avatars/parts/eyelashes/eyelashes01/image.png"),
    ),
    (
        "parts/eyelashes/eyelashes02/fit.mhclo",
        include_bytes!("../assets/avatars/parts/eyelashes/eyelashes02/fit.mhclo"),
    ),
    (
        "parts/eyelashes/eyelashes02/mesh.obj",
        include_bytes!("../assets/avatars/parts/eyelashes/eyelashes02/mesh.obj"),
    ),
    (
        "parts/eyelashes/eyelashes02/image.png",
        include_bytes!("../assets/avatars/parts/eyelashes/eyelashes02/image.png"),
    ),
    (
        "parts/hair/short02/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/short02/fit.mhclo"),
    ),
    (
        "parts/hair/short02/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/short02/mesh.obj"),
    ),
    (
        "parts/hair/short02/image.png",
        include_bytes!("../assets/avatars/parts/hair/short02/image.png"),
    ),
    (
        "parts/hair/short04/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/short04/fit.mhclo"),
    ),
    (
        "parts/hair/short04/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/short04/mesh.obj"),
    ),
    (
        "parts/hair/short04/image.png",
        include_bytes!("../assets/avatars/parts/hair/short04/image.png"),
    ),
    (
        "parts/hair/ponytail01/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/ponytail01/fit.mhclo"),
    ),
    (
        "parts/hair/ponytail01/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/ponytail01/mesh.obj"),
    ),
    (
        "parts/hair/ponytail01/image.png",
        include_bytes!("../assets/avatars/parts/hair/ponytail01/image.png"),
    ),
    (
        "parts/hair/bob02/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/bob02/fit.mhclo"),
    ),
    (
        "parts/hair/bob02/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/bob02/mesh.obj"),
    ),
    (
        "parts/hair/bob02/image.png",
        include_bytes!("../assets/avatars/parts/hair/bob02/image.png"),
    ),
    (
        "parts/hair/bob01/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/bob01/fit.mhclo"),
    ),
    (
        "parts/hair/bob01/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/bob01/mesh.obj"),
    ),
    (
        "parts/hair/bob01/image.png",
        include_bytes!("../assets/avatars/parts/hair/bob01/image.png"),
    ),
    (
        "parts/hair/long01/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/long01/fit.mhclo"),
    ),
    (
        "parts/hair/long01/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/long01/mesh.obj"),
    ),
    (
        "parts/hair/long01/image.png",
        include_bytes!("../assets/avatars/parts/hair/long01/image.png"),
    ),
    (
        "parts/hair/braid01/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/braid01/fit.mhclo"),
    ),
    (
        "parts/hair/braid01/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/braid01/mesh.obj"),
    ),
    (
        "parts/hair/braid01/image.png",
        include_bytes!("../assets/avatars/parts/hair/braid01/image.png"),
    ),
    (
        "parts/hair/afro01/fit.mhclo",
        include_bytes!("../assets/avatars/parts/hair/afro01/fit.mhclo"),
    ),
    (
        "parts/hair/afro01/mesh.obj",
        include_bytes!("../assets/avatars/parts/hair/afro01/mesh.obj"),
    ),
    (
        "parts/hair/afro01/image.png",
        include_bytes!("../assets/avatars/parts/hair/afro01/image.png"),
    ),
];

/// A bundled file by its path below `assets/avatars`.
pub fn bundled(path: &str) -> Option<&'static [u8]> {
    BUNDLED.iter().find(|(p, _)| *p == path).map(|(_, b)| *b)
}

/// The bytes an image reference of a look names: `builtin:<path>` or a file beside the look.
pub fn image_bytes(reference: &str, look_dir: &Path) -> Result<Vec<u8>, String> {
    match reference.strip_prefix("builtin:") {
        Some(path) => bundled(path)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| format!("No bundled image {path}")),
        None => {
            let path = look_dir.join(reference);
            std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
        }
    }
}

/// One vertex of a fitted part.
#[derive(Clone, Debug)]
enum FitRef {
    /// Exactly a base-mesh vertex.
    Exact(usize),
    /// A weighted sum of three base-mesh vertices and an offset (MakeHuman units) scaled by the
    /// body's size along each axis.
    Blend([usize; 3], [f64; 3], DVec3),
}

/// A part's fitting to MakeHuman's base mesh and its own mesh.
#[derive(Clone, Debug)]
pub struct Fitting {
    refs: Vec<FitRef>,
    /// Per axis: the two base-mesh vertices whose distance along it scales the offsets, and
    /// that distance on the base mesh.
    scales: [Option<(usize, usize, f64)>; 3],
    pub triangles: Vec<[usize; 3]>,
    /// Texture coordinates of each triangle's corners (image space: u right, v down).
    pub uvs: Vec<[[f32; 2]; 3]>,
}

impl Fitting {
    /// A part from its MakeHuman fitting (`.mhclo`) and mesh (`.obj` with texture coordinates).
    pub fn parse(mhclo: &str, obj: &str) -> Result<Self, String> {
        let mut refs = Vec::new();
        let mut scales = [None; 3];
        // Whether the lines are the fitted vertices; header lines may come among them.
        let mut inside = false;
        for line in mhclo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let Some(first) = parts.first() else { continue };
            if first.starts_with('#') {
                continue;
            }
            if first.parse::<f64>().is_err() {
                match *first {
                    "verts" => inside = true,
                    // Other lists (vertices to hide, weights…) are not used.
                    "delete_verts" | "weights" | "vertex_groups" => inside = false,
                    "x_scale" | "y_scale" | "z_scale" => {
                        let bad = || format!("Invalid {first} in the fitting");
                        let a = parts.get(1).and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                        let b = parts.get(2).and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                        let d: f64 = parts.get(3).and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                        let axis = usize::from(first.as_bytes()[0] - b'x');
                        scales[axis] = Some((a, b, d));
                    }
                    _ => {}
                }
                continue;
            }
            if !inside {
                continue;
            }
            let values = parts
                .iter()
                .map(|v| v.parse::<f64>())
                .collect::<Result<Vec<f64>, _>>()
                .map_err(|_| format!("Invalid fitted vertex: {line}"))?;
            refs.push(match values.len() {
                1 => FitRef::Exact(values[0] as usize),
                9.. => FitRef::Blend(
                    [values[0] as usize, values[1] as usize, values[2] as usize],
                    [values[3], values[4], values[5]],
                    DVec3::new(values[6], values[7], values[8]),
                ),
                _ => return Err("A fitted vertex needs 1 or 9 values".into()),
            });
        }
        let (positions, triangles, uvs) = obj_with_uvs(obj)?;
        if positions != refs.len() {
            return Err(format!(
                "The part's mesh has {positions} vertices but its fitting {}",
                refs.len()
            ));
        }
        Ok(Self {
            refs,
            scales,
            triangles,
            uvs,
        })
    }

    /// A bundled part by its folder below `assets/avatars/parts` (e.g. `hair/short02`).
    pub fn bundled(part: &str) -> Result<Self, String> {
        let text = |file: &str| {
            let path = format!("parts/{part}/{file}");
            bundled(&path)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .ok_or_else(|| format!("No bundled part {part}"))
        };
        Self::parse(&text("fit.mhclo")?, &text("mesh.obj")?)
    }

    /// The base-mesh vertices the fitting needs.
    pub fn references(&self) -> impl Iterator<Item = usize> + '_ {
        self.refs
            .iter()
            .flat_map(|r| match r {
                FitRef::Exact(i) => vec![*i],
                FitRef::Blend(ids, _, _) => ids.to_vec(),
            })
            .chain(self.scales.iter().flatten().flat_map(|(a, b, _)| [*a, *b]))
    }

    /// The part's vertices on a body whose base-mesh vertex `i` is at `base(i)` (MakeHuman
    /// units, before any scaling to metres).
    pub fn fit(&self, base: impl Fn(usize) -> Option<DVec3>) -> Result<Vec<DVec3>, String> {
        let at = |i: usize| base(i).ok_or_else(|| format!("The body lacks base vertex {i}"));
        let mut scale = DVec3::ONE;
        for (axis, s) in self.scales.iter().enumerate() {
            if let Some((a, b, d)) = s
                && *d > 0.
            {
                scale[axis] = (at(*a)?[axis] - at(*b)?[axis]).abs() / d;
            }
        }
        self.refs
            .iter()
            .map(|r| match r {
                FitRef::Exact(i) => at(*i),
                FitRef::Blend(ids, w, offset) => Ok(at(ids[0])? * w[0]
                    + at(ids[1])? * w[1]
                    + at(ids[2])? * w[2]
                    + *offset * scale),
            })
            .collect()
    }
}

/// An OBJ's vertex count, its fan-triangulated faces and their corners' texture coordinates.
pub type ObjWithUvs = (usize, Vec<[usize; 3]>, Vec<[[f32; 2]; 3]>);

/// The vertex count, the fan-triangulated faces and their corners' texture coordinates of an
/// OBJ whose faces all name texture coordinates (image space: v down).
pub fn obj_with_uvs(obj: &str) -> Result<ObjWithUvs, String> {
    let (mut positions, mut uv) = (0usize, Vec::<[f32; 2]>::new());
    let (mut triangles, mut corners) = (Vec::new(), Vec::new());
    for line in obj.lines() {
        let tokens: Vec<&str> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        match tokens.first() {
            Some(&"v") => positions += 1,
            Some(&"vt") => {
                let u: f32 = tokens.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.);
                let v: f32 = tokens.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.);
                uv.push([u, 1. - v]);
            }
            Some(&"f") => {
                let mut ids = Vec::new();
                for t in &tokens[1..] {
                    let mut fields = t.split('/');
                    let v: usize = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("An OBJ face corner has no vertex")?;
                    let t: usize = fields
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("An OBJ face corner has no texture coordinate")?;
                    if v == 0 || v > positions || t == 0 || t > uv.len() {
                        return Err("An OBJ face refers past its vertices".into());
                    }
                    ids.push((v - 1, t - 1));
                }
                if ids.len() < 3 {
                    return Err("An OBJ face needs three corners".into());
                }
                for k in 1..ids.len() - 1 {
                    let c = [ids[0], ids[k], ids[k + 1]];
                    triangles.push(c.map(|(v, _)| v));
                    corners.push(c.map(|(_, t)| uv[t]));
                }
            }
            _ => {}
        }
    }
    Ok((positions, triangles, corners))
}

/// Texture coordinates of the mannequin's triangles, in its order (MakeHuman's UV layout): from
/// the body faces of `base.obj`, triangulated as `scripts/prepare-avatar.py` did.
pub fn body_uvs() -> Result<&'static [[[f32; 2]; 3]], String> {
    static UVS: OnceLock<Result<Vec<[[f32; 2]; 3]>, String>> = OnceLock::new();
    UVS.get_or_init(|| {
        let mut body = String::new();
        let mut group = "";
        for line in BASE_OBJ.lines() {
            if let Some(name) = line.strip_prefix("g ") {
                group = name.trim();
            }
            if line.starts_with('v') || (line.starts_with("f ") && group == "body") {
                body.push_str(line);
                body.push('\n');
            }
        }
        let (_, _, uvs) = obj_with_uvs(&body)?;
        Ok(uvs)
    })
    .as_deref()
    .map_err(Clone::clone)
}

/// An OBJ of `surface` whose triangles carry `uvs` (image space, as [`obj_with_uvs`] reads
/// them back).
pub fn surface_obj_with_uvs(surface: &SurfaceMesh, uvs: &[[[f32; 2]; 3]], comment: &str) -> String {
    use std::fmt::Write;
    let mut out = format!("# {comment}\n");
    for v in &surface.vertices {
        let _ = writeln!(out, "v {:.6} {:.6} {:.6}", v.x, v.y, v.z);
    }
    for t in uvs {
        for [u, v] in t {
            let _ = writeln!(out, "vt {:.6} {:.6}", u, 1. - v);
        }
    }
    for (k, t) in surface.triangles.iter().enumerate() {
        let _ = writeln!(
            out,
            "f {}/{} {}/{} {}/{}",
            t[0] + 1,
            3 * k + 1,
            t[1] + 1,
            3 * k + 2,
            t[2] + 1,
            3 * k + 3
        );
    }
    out
}

/// A fitted part of a look.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LookPart {
    /// The bundled part it was fitted from (e.g. `hair/short02`).
    pub name: String,
    /// Its image: `builtin:<path>` or a file beside the look.
    pub image: String,
    /// Drawn only where its image is opaque (hair, eyebrows, eyelashes).
    #[serde(default)]
    pub cutout: bool,
    /// A colour (sRGB) the image is shown in, keeping its shading; its own colours when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[f32; 3]>,
    /// Vertices, metres, in the avatar's frame.
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Texture coordinates of each triangle's corners (image space: v down).
    pub uvs: Vec<[[f32; 2]; 3]>,
}

/// A look as kept beside its avatar.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Look {
    pub format: String,
    pub version: u32,
    /// The skin on the body's texture coordinates (from the avatar OBJ).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    #[serde(default)]
    pub parts: Vec<LookPart>,
    /// Underwear painted on the body, so that a skin never shows it bare.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underwear: Option<Underwear>,
}

/// Underwear painted over the skin.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Underwear {
    /// sRGB.
    pub colour: [f32; 3],
    /// Per avatar vertex, metres: how far inside the underwear's edge it is (negative
    /// outside); the edge lies where this, spread over each triangle, crosses zero.
    pub coverage: Vec<f32>,
}

impl Look {
    pub fn new(skin: Option<String>) -> Self {
        Self {
            format: FORMAT.into(),
            version: 1,
            skin,
            parts: vec![],
            underwear: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.version != 1 {
            return Err("Not a FashionCraft avatar look of a known version".into());
        }
        for reference in self.skin.iter().chain(self.parts.iter().map(|p| &p.image)) {
            if !reference.starts_with("builtin:") && !is_beside(reference) {
                return Err(format!("The look's image {reference} is not beside it"));
            }
        }
        for p in &self.parts {
            let n = p.vertices.len() as u32;
            if p.triangles.len() != p.uvs.len()
                || p.triangles.iter().flatten().any(|i| *i >= n)
                || p.vertices.iter().flatten().any(|v| !v.is_finite())
            {
                return Err(format!("The look's part {} is malformed", p.name));
            }
        }
        Ok(())
    }
}

/// Whether `reference` names a file in the look's folder or below it.
fn is_beside(reference: &str) -> bool {
    let path = Path::new(reference);
    !reference.is_empty()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Copy the look of the avatar at `avatar_obj`, with the image files it names, to `to` (a
/// look file beside another copy of the avatar); false when it has none.
pub fn copy_look(avatar_obj: &Path, to: &Path) -> Result<bool, String> {
    let Some(from) = look_file(avatar_obj) else {
        return Ok(false);
    };
    let text = std::fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
    let look: Look = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", from.display()))?;
    look.validate()?;
    let (from_dir, to_dir) = (
        from.parent().unwrap_or(Path::new(".")),
        to.parent().unwrap_or(Path::new(".")),
    );
    for reference in look.skin.iter().chain(look.parts.iter().map(|p| &p.image)) {
        if reference.starts_with("builtin:") {
            continue;
        }
        let target = to_dir.join(reference);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(from_dir.join(reference), &target)
            .map_err(|e| format!("{reference}: {e}"))?;
    }
    std::fs::write(to, text).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(true)
}

/// Where the look of the avatar at `avatar_obj` is kept: `<stem>.look.json`, or the shared
/// `look.json` beside it.
pub fn look_file(avatar_obj: &Path) -> Option<PathBuf> {
    let dir = avatar_obj.parent().unwrap_or(Path::new("."));
    let own = avatar_obj
        .file_stem()
        .map(|s| dir.join(format!("{}.look.json", s.to_string_lossy())));
    own.filter(|p| p.exists())
        .or_else(|| Some(dir.join("look.json")).filter(|p| p.exists()))
}

/// The look of the avatar at `avatar_obj`, if it has one.
pub fn load(avatar_obj: &Path) -> Result<Option<Look>, String> {
    let Some(file) = look_file(avatar_obj) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let look: Look = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    look.validate()?;
    Ok(Some(look))
}

/// The texture coordinates of an avatar OBJ's triangles, when all its faces name them.
pub fn avatar_uvs(avatar_obj: &Path) -> Result<Option<Vec<[[f32; 2]; 3]>>, String> {
    let text = std::fs::read_to_string(avatar_obj)
        .map_err(|e| format!("{}: {e}", avatar_obj.display()))?;
    Ok(obj_with_uvs(&text).ok().map(|(_, _, uvs)| uvs))
}

/// A part fitted to a made body, ready for a look.
pub fn fitted_part(
    name: &str,
    image: &str,
    cutout: bool,
    vertices: &[DVec3],
    fitting: &Fitting,
) -> LookPart {
    LookPart {
        name: name.into(),
        image: image.into(),
        cutout,
        tint: None,
        vertices: vertices.iter().map(|v| v.as_vec3().to_array()).collect(),
        triangles: fitting
            .triangles
            .iter()
            .map(|t| t.map(|i| i as u32))
            .collect(),
        uvs: fitting.uvs.clone(),
    }
}

/// For each vertex of `points`, the nearest vertex of `body` (searched cell by cell round it,
/// all of them when none is near): parts follow the body vertex they are bound to when it moves.
pub fn nearest_body_vertices(body: &[DVec3], points: &[DVec3]) -> Vec<usize> {
    const CELL: f64 = 0.03;
    const REACH: i64 = 6;
    let key = |p: DVec3| {
        (
            (p.x / CELL).floor() as i64,
            (p.y / CELL).floor() as i64,
            (p.z / CELL).floor() as i64,
        )
    };
    let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<usize>> = Default::default();
    for (i, p) in body.iter().enumerate() {
        grid.entry(key(*p)).or_default().push(i);
    }
    let nearest_of = |p: DVec3, candidates: &mut dyn Iterator<Item = usize>| {
        candidates
            .map(|i| (body[i].distance_squared(p), i))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    };
    points
        .iter()
        .map(|p| {
            let (x, y, z) = key(*p);
            for reach in 1..=REACH {
                let mut cells = (-reach..=reach).flat_map(|dx| {
                    (-reach..=reach)
                        .flat_map(move |dy| (-reach..=reach).map(move |dz| (dx, dy, dz)))
                });
                let mut candidates = std::iter::from_fn(|| cells.next())
                    .filter_map(|(dx, dy, dz)| grid.get(&(x + dx, y + dy, z + dz)))
                    .flatten()
                    .copied();
                // Every vertex within `reach` cells of the point has been seen.
                if let Some((d, i)) = nearest_of(*p, &mut candidates)
                    && d.sqrt() <= reach as f64 * CELL
                {
                    return i;
                }
            }
            nearest_of(*p, &mut (0..body.len())).map_or(0, |(_, i)| i)
        })
        .collect()
}

/// The number of the base mesh's body vertices (the mannequin's).
pub const fn body_vertices() -> usize {
    BODY
}

/// Skins: the bundled image's name and what it shows.
pub const SKINS: [(&str, &str); 10] = [
    ("young_caucasian_female", "Light skin, woman"),
    ("young_caucasian_male", "Light skin, man"),
    ("young_african_female", "Dark skin, woman"),
    ("young_african_male", "Dark skin, man"),
    ("young_asian_female", "East Asian skin, woman"),
    ("young_asian_male", "East Asian skin, man"),
    ("middleage_caucasian_female", "Light skin, mature woman"),
    ("middleage_caucasian_male", "Light skin, mature man"),
    ("middleage_african_male", "Dark skin, mature man"),
    ("middleage_asian_female", "East Asian skin, mature woman"),
];
pub const EYE_COLOURS: [(&str, &str); 5] = [
    ("brown", "Brown"),
    ("brownlight", "Light brown"),
    ("blue", "Blue"),
    ("green", "Green"),
    ("grey", "Grey"),
];
pub const EYEBROWS: [(&str, &str); 2] = [("eyebrow001", "Natural"), ("eyebrow010", "Thin")];
pub const EYELASHES: [(&str, &str); 2] =
    [("eyelashes01", "Natural"), ("eyelashes02", "Long lashes")];
pub const HAIR: [(&str, &str); 8] = [
    ("short02", "Short"),
    ("short04", "Cropped"),
    ("ponytail01", "Ponytail"),
    ("bob01", "Bob"),
    ("bob02", "Side-swept bob"),
    ("long01", "Long"),
    ("braid01", "Braid"),
    ("afro01", "Afro"),
];
/// Hair colours, sRGB; the hair's own colour when none is chosen.
pub const HAIR_COLOURS: [(&str, &str, [f32; 3]); 7] = [
    ("black", "Black", [0.05, 0.045, 0.04]),
    ("dark_brown", "Dark brown", [0.17, 0.11, 0.07]),
    ("brown", "Medium brown", [0.36, 0.23, 0.14]),
    ("auburn", "Auburn", [0.48, 0.2, 0.09]),
    ("blonde", "Blonde", [0.8, 0.64, 0.4]),
    ("platinum", "Platinum", [0.88, 0.85, 0.78]),
    ("grey", "Grey", [0.6, 0.6, 0.6]),
];
/// Colour of the underwear every skinned body wears, sRGB.
pub const UNDERWEAR_COLOUR: [f32; 3] = [0.27, 0.27, 0.29];

/// What a look is made of, by the names in [`SKINS`], [`EYE_COLOURS`], [`EYEBROWS`],
/// [`EYELASHES`] and [`HAIR`]; none leaves it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LookChoice {
    pub skin: Option<&'static str>,
    pub eyes: Option<&'static str>,
    pub eyebrows: Option<&'static str>,
    pub eyelashes: Option<&'static str>,
    pub hair: Option<&'static str>,
    /// One of [`HAIR_COLOURS`]; the hair's own colour when none.
    pub hair_colour: Option<&'static str>,
}

impl LookChoice {
    /// A complete look.
    pub const fn of(
        skin: &'static str,
        eyes: &'static str,
        eyebrows: &'static str,
        eyelashes: &'static str,
        hair: Option<&'static str>,
    ) -> Self {
        Self {
            skin: Some(skin),
            eyes: Some(eyes),
            eyebrows: Some(eyebrows),
            eyelashes: Some(eyelashes),
            hair,
            hair_colour: None,
        }
    }

    /// This look with its hair in `colour`, one of [`HAIR_COLOURS`].
    pub const fn hair_coloured(self, colour: &'static str) -> Self {
        Self {
            hair_colour: Some(colour),
            ..self
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The look `choice` makes on `body`, its parts fitted to it.
pub fn make_look(
    body: &crate::avatar_maker::MadeBody,
    choice: &LookChoice,
) -> Result<Look, String> {
    let mut look = Look::new(choice.skin.map(|s| format!("builtin:skins/{s}.jpg")));
    let mut add = |part: String, image: String, cutout: bool| -> Result<(), String> {
        let fitting = Fitting::bundled(&part)?;
        let vertices = body.fit(&fitting)?;
        look.parts
            .push(fitted_part(&part, &image, cutout, &vertices, &fitting));
        Ok(())
    };
    if let Some(colour) = choice.eyes {
        add(
            "eyes/low-poly".into(),
            format!("builtin:eyes/{colour}.png"),
            false,
        )?;
    }
    for (kind, name) in [
        ("eyebrows", choice.eyebrows),
        ("eyelashes", choice.eyelashes),
        ("hair", choice.hair),
    ] {
        if let Some(name) = name {
            let part = format!("{kind}/{name}");
            add(
                part.clone(),
                format!("builtin:parts/{part}/image.png"),
                true,
            )?;
        }
    }
    let tint = choice
        .hair_colour
        .and_then(|c| HAIR_COLOURS.iter().find(|(name, _, _)| *name == c))
        .map(|(_, _, rgb)| *rgb);
    if let Some(hair) = look.parts.iter_mut().find(|p| p.name.starts_with("hair/")) {
        hair.tint = tint;
    }
    if let Some(skin) = choice.skin {
        // A top as well for women and girls, and for any body with a woman's skin.
        let top = body.settings.gender < 0.75 || skin.contains("female");
        look.underwear = Some(Underwear {
            colour: UNDERWEAR_COLOUR,
            coverage: underwear_coverage(&body.surface, top),
        });
    }
    Ok(look)
}

/// Half the width (largest |x|) of the cross-section of `body` at height `y` that passes
/// nearest the point straight in front of the body's middle; none when the height misses it.
fn front_section_half_width(body: &SurfaceMesh, y: f64) -> Option<f64> {
    let v = &body.vertices;
    // Each straddling triangle's crossing points, joined to the others across shared edges.
    let mut edges: std::collections::HashMap<(usize, usize), Vec<usize>> = Default::default();
    let mut crossings: Vec<Vec<DVec3>> = Vec::new();
    for t in &body.triangles {
        let mut points = Vec::new();
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            let (pa, pb) = (v[a], v[b]);
            if (pa.y < y) != (pb.y < y) {
                let f = (y - pa.y) / (pb.y - pa.y);
                points.push(pa.lerp(pb, f));
                edges
                    .entry((a.min(b), a.max(b)))
                    .or_default()
                    .push(crossings.len());
            }
        }
        if !points.is_empty() {
            crossings.push(points);
        }
    }
    let mut parent: Vec<usize> = (0..crossings.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for shared in edges.values() {
        for pair in shared.windows(2) {
            let (a, b) = (root(&mut parent, pair[0]), root(&mut parent, pair[1]));
            parent[a] = b;
        }
    }
    let front = DVec3::new(0., y, 10.);
    let seed = (0..crossings.len()).min_by(|a, b| {
        let d = |k: &usize| {
            crossings[*k]
                .iter()
                .map(|p| p.distance_squared(front))
                .fold(f64::INFINITY, f64::min)
        };
        d(a).total_cmp(&d(b))
    })?;
    let seed = root(&mut parent, seed);
    let mut width: f64 = 0.;
    for (k, points) in crossings.iter().enumerate() {
        if root(&mut parent, k) == seed {
            for p in points {
                width = width.max(p.x.abs());
            }
        }
    }
    Some(width)
}

/// How far inside underwear each vertex of `body` (standing on y = 0, facing +z, arms away
/// from the torso as MakeHuman's are) is, metres, negative outside: briefs, and a top when
/// `top`. Each is a band of height round the body grown along the mesh from one vertex, so it
/// never reaches the arms; the top stops below the armpits.
pub fn underwear_coverage(body: &SurfaceMesh, top: bool) -> Vec<f32> {
    let v = &body.vertices;
    let mut coverage = vec![-1f64; v.len()];
    let mut neighbours = vec![Vec::new(); v.len()];
    for t in &body.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            neighbours[a].push(b);
            neighbours[b].push(a);
        }
    }
    let height = v.iter().map(|p| p.y).fold(0., f64::max);
    let s = height / 1.75;
    // Vertices between heights `low` and `high` joined to `seed` through such vertices get
    // their distance to the nearer of the two; others above or below get theirs, negative.
    let mut band = |seed: usize, low: f64, high: f64| {
        let mut inside = vec![false; v.len()];
        let mut stack = vec![seed];
        inside[seed] = true;
        while let Some(i) = stack.pop() {
            for &j in &neighbours[i] {
                if !inside[j] && (low..=high).contains(&v[j].y) {
                    inside[j] = true;
                    stack.push(j);
                }
            }
        }
        for (i, p) in v.iter().enumerate() {
            let within = (p.y - low).min(high - p.y);
            if inside[i] || within < 0. {
                coverage[i] = coverage[i].max(within);
            }
        }
    };
    // The crotch: the lowest vertex on the body's middle line.
    let Some(crotch) = (0..v.len())
        .filter(|i| v[*i].x.abs() < 0.004 && v[*i].y > 0.25 * height)
        .min_by(|a, b| v[*a].y.total_cmp(&v[*b].y))
    else {
        return vec![-1.; v.len()];
    };
    let cy = v[crotch].y;
    band(crotch, cy - 0.025 * s, cy + 0.12 * s);
    // The bust's front, off the middle line.
    let nipple = (0..v.len())
        .filter(|i| {
            let p = v[*i];
            (0.03..0.16).contains(&p.x.abs()) && (cy + 0.3 * s..cy + 0.55 * s).contains(&p.y)
        })
        .max_by(|a, b| v[*a].z.total_cmp(&v[*b].z));
    if let (true, Some(nipple)) = (top, nipple) {
        let ny = v[nipple].y;
        let low = ny - 0.075 * s;
        // Up from below the bust until the arms join the torso's cross-section.
        let mut high = ny + 0.1 * s;
        let mut y = low;
        let mut last = front_section_half_width(body, y);
        while y < ny + 0.1 * s {
            y += 0.004 * s;
            let width = front_section_half_width(body, y);
            if let (Some(a), Some(b)) = (last, width)
                && b > a + 0.025 * s
            {
                high = high.min(y - 0.015 * s);
                break;
            }
            last = width;
        }
        if high > low + 0.03 * s {
            band(nipple, low, high);
        }
    }
    coverage
        .into_iter()
        .map(|c| ((c * 1e4).round() / 1e4) as f32)
        .collect()
}

/// Paint `underwear` over `skin` on a body of `triangles` whose corners have `uvs` (image
/// space): every texel where the coverage, spread over its triangle, is positive.
pub fn paint_underwear(
    skin: &mut DecodedImage,
    triangles: &[[usize; 3]],
    uvs: &[[[f32; 2]; 3]],
    underwear: &Underwear,
) {
    let [w, h] = skin.size;
    let colour = underwear
        .colour
        .map(|c| (c.clamp(0., 1.) * 255.).round() as u8);
    for (t, uv) in triangles.iter().zip(uvs) {
        let Some(c) = t
            .iter()
            .map(|i| underwear.coverage.get(*i).copied())
            .collect::<Option<Vec<f32>>>()
        else {
            continue;
        };
        if c.iter().all(|c| *c <= 0.) {
            continue;
        }
        let p = uv.map(|[u, v]| [u * w as f32, v * h as f32]);
        let area =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if area.abs() < 1e-9 {
            continue;
        }
        // A texel and a half beyond each edge, so that filtering finds no skin at the seams.
        let altitude =
            |a: [f32; 2], b: [f32; 2]| area.abs() / ((b[0] - a[0]).hypot(b[1] - a[1])).max(1e-6);
        let margin = [
            altitude(p[1], p[2]),
            altitude(p[2], p[0]),
            altitude(p[0], p[1]),
        ]
        .map(|a| 1.5 / a);
        let x0 = p.iter().map(|q| q[0]).fold(f32::INFINITY, f32::min).floor() - 2.;
        let x1 = p
            .iter()
            .map(|q| q[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            + 2.;
        let y0 = p.iter().map(|q| q[1]).fold(f32::INFINITY, f32::min).floor() - 2.;
        let y1 = p
            .iter()
            .map(|q| q[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            + 2.;
        for y in (y0.max(0.) as u32)..=(y1.min(h as f32 - 1.).max(0.) as u32) {
            for x in (x0.max(0.) as u32)..=(x1.min(w as f32 - 1.).max(0.) as u32) {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let edge = |a: [f32; 2], b: [f32; 2]| {
                    ((b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])) / area
                };
                let b = [edge(p[1], p[2]), edge(p[2], p[0]), edge(p[0], p[1])];
                if b.iter().zip(&margin).any(|(b, m)| *b < -m) {
                    continue;
                }
                if b[0] * c[0] + b[1] * c[1] + b[2] * c[2] > 0. {
                    let k = 4 * (y * w + x) as usize;
                    skin.rgba[k..k + 3].copy_from_slice(&colour);
                }
            }
        }
    }
}

/// `image` shown in `tint` (sRGB): each texel's brightness against the image's mean, times
/// the colour; transparency kept.
pub fn tinted(image: &DecodedImage, tint: [f32; 3]) -> DecodedImage {
    let luminance = |c: &[u8]| {
        0.2126 * to_linear(c[0] as f32 / 255.)
            + 0.7152 * to_linear(c[1] as f32 / 255.)
            + 0.0722 * to_linear(c[2] as f32 / 255.)
    };
    let (mut sum, mut count) = (0f64, 0usize);
    let (pixels, _) = image.rgba.as_chunks::<4>();
    for c in pixels.iter().filter(|c| c[3] > 128) {
        sum += luminance(c) as f64;
        count += 1;
    }
    let mean = if count == 0 {
        1.
    } else {
        (sum / count as f64).max(1e-4) as f32
    };
    let tint = tint.map(to_linear);
    let mut rgba = image.rgba.clone();
    let (pixels, _) = rgba.as_chunks_mut::<4>();
    for c in pixels {
        let l = luminance(c) / mean;
        for k in 0..3 {
            c[k] = (to_srgb(tint[k] * l) * 255.).round() as u8;
        }
    }
    DecodedImage {
        size: image.size,
        rgba,
    }
}

/// A ready-made avatar of the library.
pub struct Preset {
    /// English name, translated where shown.
    pub name: &'static str,
    pub body: fn() -> crate::avatar_maker::MacroBody,
    pub look: LookChoice,
}

fn adult(gender: f64, age: f64) -> crate::avatar_maker::MacroBody {
    crate::avatar_maker::MacroBody::typical(gender, age)
}

/// The library's avatars: women, men and children of several builds, ages and ancestries.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Woman · medium",
        body: || crate::avatar_maker::MacroBody {
            european: 1.,
            african: 0.,
            asian: 0.,
            ..adult(0., 28.)
        },
        look: LookChoice::of(
            "young_caucasian_female",
            "brown",
            "eyebrow001",
            "eyelashes01",
            Some("ponytail01"),
        ),
    },
    Preset {
        name: "Woman · petite",
        body: || crate::avatar_maker::MacroBody {
            height: 0.3,
            weight: 0.4,
            asian: 1.,
            african: 0.,
            european: 0.,
            ..adult(0., 26.)
        },
        look: LookChoice::of(
            "young_asian_female",
            "brown",
            "eyebrow010",
            "eyelashes02",
            Some("long01"),
        )
        .hair_coloured("black"),
    },
    Preset {
        name: "Woman · tall",
        body: || crate::avatar_maker::MacroBody {
            height: 0.8,
            weight: 0.45,
            african: 1.,
            asian: 0.,
            european: 0.,
            ..adult(0., 25.)
        },
        look: LookChoice::of(
            "young_african_female",
            "brownlight",
            "eyebrow001",
            "eyelashes02",
            Some("afro01"),
        ),
    },
    Preset {
        name: "Woman · curvy",
        body: || {
            let mut body = crate::avatar_maker::MacroBody {
                weight: 0.7,
                cupsize: 0.75,
                european: 0.6,
                african: 0.2,
                asian: 0.2,
                ..adult(0., 32.)
            };
            body.measures[3] = 0.5; // hips
            body
        },
        look: LookChoice::of(
            "young_caucasian_female",
            "green",
            "eyebrow001",
            "eyelashes01",
            Some("bob01"),
        )
        .hair_coloured("auburn"),
    },
    Preset {
        name: "Woman · 55",
        body: || crate::avatar_maker::MacroBody {
            european: 0.8,
            asian: 0.2,
            african: 0.,
            ..adult(0., 55.)
        },
        look: LookChoice::of(
            "middleage_caucasian_female",
            "blue",
            "eyebrow010",
            "eyelashes01",
            Some("bob02"),
        ),
    },
    Preset {
        name: "Man · medium",
        body: || crate::avatar_maker::MacroBody {
            european: 1.,
            african: 0.,
            asian: 0.,
            ..adult(1., 28.)
        },
        look: LookChoice::of(
            "young_caucasian_male",
            "blue",
            "eyebrow001",
            "eyelashes01",
            Some("short02"),
        ),
    },
    Preset {
        name: "Man · slim",
        body: || crate::avatar_maker::MacroBody {
            weight: 0.3,
            muscle: 0.35,
            asian: 1.,
            african: 0.,
            european: 0.,
            ..adult(1., 24.)
        },
        look: LookChoice::of(
            "young_asian_male",
            "brown",
            "eyebrow001",
            "eyelashes01",
            Some("short04"),
        ),
    },
    Preset {
        name: "Man · athletic",
        body: || crate::avatar_maker::MacroBody {
            muscle: 0.85,
            weight: 0.55,
            height: 0.65,
            african: 1.,
            asian: 0.,
            european: 0.,
            ..adult(1., 27.)
        },
        look: LookChoice::of(
            "young_african_male",
            "brown",
            "eyebrow001",
            "eyelashes01",
            Some("short04"),
        ),
    },
    Preset {
        name: "Man · large",
        body: || crate::avatar_maker::MacroBody {
            weight: 0.8,
            muscle: 0.5,
            african: 0.7,
            european: 0.3,
            asian: 0.,
            ..adult(1., 40.)
        },
        look: LookChoice::of(
            "middleage_african_male",
            "brown",
            "eyebrow001",
            "eyelashes01",
            Some("short04"),
        ),
    },
    Preset {
        name: "Man · 55",
        body: || crate::avatar_maker::MacroBody {
            european: 1.,
            african: 0.,
            asian: 0.,
            ..adult(1., 55.)
        },
        look: LookChoice::of(
            "middleage_caucasian_male",
            "grey",
            "eyebrow001",
            "eyelashes01",
            Some("short02"),
        )
        .hair_coloured("grey"),
    },
    Preset {
        name: "Girl · 8",
        body: || crate::avatar_maker::MacroBody {
            european: 0.5,
            asian: 0.5,
            african: 0.,
            cupsize: 0.,
            ..adult(0., 8.)
        },
        look: LookChoice::of(
            "young_caucasian_female",
            "brownlight",
            "eyebrow010",
            "eyelashes01",
            Some("braid01"),
        )
        .hair_coloured("brown"),
    },
    Preset {
        name: "Boy · 8",
        body: || crate::avatar_maker::MacroBody {
            african: 0.5,
            european: 0.5,
            asian: 0.,
            ..adult(1., 8.)
        },
        look: LookChoice::of(
            "young_african_male",
            "brown",
            "eyebrow001",
            "eyelashes01",
            Some("short04"),
        ),
    },
];

/// The rotation and translation that best carry `from` onto `to` (least squares; Horn's
/// closed form with unit quaternions), none for fewer than three points.
pub fn rigid_motion(from: &[DVec3], to: &[DVec3]) -> Option<(glam::DQuat, DVec3)> {
    let n = from.len().min(to.len());
    if n < 3 {
        return None;
    }
    let ca = from[..n].iter().sum::<DVec3>() / n as f64;
    let cb = to[..n].iter().sum::<DVec3>() / n as f64;
    let mut s = [[0f64; 3]; 3];
    for (a, b) in from[..n].iter().zip(&to[..n]) {
        let (a, b) = ((*a - ca).to_array(), (*b - cb).to_array());
        for (i, row) in s.iter_mut().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value += a[i] * b[j];
            }
        }
    }
    let [[xx, xy, xz], [yx, yy, yz], [zx, zy, zz]] = s;
    let mut m = [
        [xx + yy + zz, yz - zy, zx - xz, xy - yx],
        [yz - zy, xx - yy - zz, xy + yx, zx + xz],
        [zx - xz, xy + yx, -xx + yy - zz, yz + zy],
        [xy - yx, zx + xz, yz + zy, -xx - yy + zz],
    ];
    // Jacobi rotations until the matrix is diagonal; its columns gather in `v`.
    let mut v = [[0f64; 4]; 4];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.;
    }
    for _ in 0..60 {
        let off: f64 = (0..4)
            .flat_map(|i| (0..4).filter(move |j| *j != i).map(move |j| (i, j)))
            .map(|(i, j)| m[i][j] * m[i][j])
            .sum();
        if off < 1e-24 {
            break;
        }
        for p in 0..3 {
            for q in p + 1..4 {
                if m[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2. * m[p][q]);
                let t = if theta == 0. {
                    1.
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.).sqrt())
                };
                let c = 1. / (t * t + 1.).sqrt();
                let sn = t * c;
                for row in &mut m {
                    let (mkp, mkq) = (row[p], row[q]);
                    row[p] = c * mkp - sn * mkq;
                    row[q] = sn * mkp + c * mkq;
                }
                let (above, below) = m.split_at_mut(q);
                for (a, b) in above[p].iter_mut().zip(below[0].iter_mut()) {
                    let (mpk, mqk) = (*a, *b);
                    *a = c * mpk - sn * mqk;
                    *b = sn * mpk + c * mqk;
                }
                for row in &mut v {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - sn * vq;
                    row[q] = sn * vp + c * vq;
                }
            }
        }
    }
    let best = (0..4).max_by(|a, b| m[*a][*a].total_cmp(&m[*b][*b]))?;
    let q = glam::DQuat::from_xyzw(v[1][best], v[2][best], v[3][best], v[0][best]).normalize();
    if !q.is_finite() {
        return None;
    }
    Some((q, cb - q * ca))
}

/// A look's part ready to follow its avatar: where it is on the avatar at rest, and the
/// avatar vertices it moves with.
pub struct BoundPart {
    pub rest: Vec<DVec3>,
    /// The nearest avatar vertex of each of its vertices, without repeats.
    pub anchors: Vec<usize>,
}

impl BoundPart {
    pub fn new(rest_body: &[DVec3], part: &LookPart) -> Self {
        let rest: Vec<DVec3> = part
            .vertices
            .iter()
            .map(|v| glam::Vec3::from_array(*v).as_dvec3())
            .collect();
        let mut anchors = nearest_body_vertices(rest_body, &rest);
        anchors.sort_unstable();
        anchors.dedup();
        Self { rest, anchors }
    }

    /// The part on the avatar posed as `body` (whose first vertices are the avatar's, in
    /// its rest order), moved rigidly as its anchors are; none when `body` lacks them.
    pub fn posed(&self, rest_body: &[DVec3], body: &[DVec3]) -> Option<Vec<DVec3>> {
        if body.len() < rest_body.len() {
            return None;
        }
        let from: Vec<DVec3> = self.anchors.iter().map(|i| rest_body[*i]).collect();
        let to: Vec<DVec3> = self.anchors.iter().map(|i| body[*i]).collect();
        let (q, t) = rigid_motion(&from, &to)?;
        Some(self.rest.iter().map(|p| q * *p + t).collect())
    }
}

/// A decoded image: RGBA rows, top first.
pub struct DecodedImage {
    pub size: [u32; 2],
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    /// The colour at texture coordinates `uv` (image space, repeating), RGBA 0..1.
    fn at(&self, uv: [f32; 2]) -> [f32; 4] {
        let [w, h] = self.size;
        if w == 0 || h == 0 {
            return [1.; 4];
        }
        let x = ((uv[0].rem_euclid(1.) * w as f32) as u32).min(w - 1);
        let y = ((uv[1].rem_euclid(1.) * h as f32) as u32).min(h - 1);
        let k = 4 * (y * w + x) as usize;
        let c = &self.rgba[k..k + 4];
        [c[0], c[1], c[2], c[3]].map(|v| v as f32 / 255.)
    }
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(v: f32) -> f32 {
    let v = v.clamp(0., 1.);
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}

/// A small picture of `body` with its look, seen from the front and a little from its right,
/// on a clear background: RGBA rows of `size`, top first. `body_uvs` places the skin; images
/// are found by their references in the look.
pub fn thumbnail(
    body: &SurfaceMesh,
    body_uvs: Option<&[[[f32; 2]; 3]]>,
    look: &Look,
    images: &std::collections::BTreeMap<String, DecodedImage>,
    size: [u32; 2],
) -> Vec<u8> {
    const SUPER: u32 = 2;
    const SKIN: [f32; 3] = [0.95, 0.68, 0.55];
    let [w, h] = size.map(|s| s.max(1) * SUPER);
    let (sin, cos) = 0.35f64.sin_cos();
    let view = |p: DVec3| DVec3::new(cos * p.x + sin * p.z, p.y, -sin * p.x + cos * p.z);
    // Each mesh: its vertices as seen, smooth normals, triangles with texture coordinates,
    // its image and whether clear texels are cut out.
    struct Shape<'a> {
        points: Vec<DVec3>,
        normals: Vec<DVec3>,
        triangles: Vec<[usize; 3]>,
        uvs: Option<&'a [[[f32; 2]; 3]]>,
        image: Option<&'a DecodedImage>,
        cutout: bool,
    }
    let shape = |points: Vec<DVec3>, triangles: Vec<[usize; 3]>| {
        let mut normals = vec![DVec3::ZERO; points.len()];
        for t in &triangles {
            let [a, b, c] = t.map(|i| points[i]);
            let n = (b - a).cross(c - a);
            for i in t {
                normals[*i] += n;
            }
        }
        for n in &mut normals {
            *n = n.normalize_or_zero();
        }
        (points, normals, triangles)
    };
    let mut shapes = Vec::new();
    let tinted: Vec<Option<DecodedImage>> = look
        .parts
        .iter()
        .map(|p| Some(tinted(images.get(&p.image)?, p.tint?)))
        .collect();
    let painted = match (
        look.skin.as_ref().and_then(|s| images.get(s)),
        body_uvs,
        &look.underwear,
    ) {
        (Some(skin), Some(uvs), Some(underwear)) => {
            let mut skin = DecodedImage {
                size: skin.size,
                rgba: skin.rgba.clone(),
            };
            paint_underwear(&mut skin, &body.triangles, uvs, underwear);
            Some(skin)
        }
        _ => None,
    };
    let (points, normals, triangles) = shape(
        body.vertices.iter().map(|p| view(*p)).collect(),
        body.triangles.clone(),
    );
    let skin = painted
        .as_ref()
        .or_else(|| look.skin.as_ref().and_then(|s| images.get(s)));
    shapes.push(Shape {
        points,
        normals,
        uvs: body_uvs.filter(|u| u.len() >= triangles.len() && skin.is_some()),
        triangles,
        image: skin,
        cutout: false,
    });
    for (part, tinted) in look.parts.iter().zip(&tinted) {
        let (points, normals, triangles) = shape(
            part.vertices
                .iter()
                .map(|v| view(glam::Vec3::from_array(*v).as_dvec3()))
                .collect(),
            part.triangles
                .iter()
                .map(|t| t.map(|i| i as usize))
                .collect(),
        );
        shapes.push(Shape {
            points,
            normals,
            triangles,
            uvs: Some(&part.uvs),
            image: tinted.as_ref().or_else(|| images.get(&part.image)),
            cutout: part.cutout,
        });
    }
    let (mut low, mut high) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for p in &shapes[0].points {
        low = low.min(*p);
        high = high.max(*p);
    }
    let scale = 0.92 * h as f64 / (high.y - low.y).max(1e-6);
    let centre_x = 0.5 * (low.x + high.x);
    let screen = |p: DVec3| {
        [
            (0.5 * w as f64 + (p.x - centre_x) * scale) as f32,
            (0.04 * h as f64 + (high.y - p.y) * scale) as f32,
        ]
    };
    let light = DVec3::new(-0.45, 0.55, 0.75).normalize();
    let mut depth = vec![f32::NEG_INFINITY; (w * h) as usize];
    let mut colour = vec![[0f32; 4]; (w * h) as usize];
    for s in &shapes {
        for (k, t) in s.triangles.iter().enumerate() {
            let p = t.map(|i| screen(s.points[i]));
            let z = t.map(|i| s.points[i].z as f32);
            let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            if area.abs() < 1e-12 {
                continue;
            }
            let x0 = p
                .iter()
                .map(|q| q[0])
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.) as u32;
            let x1 = p
                .iter()
                .map(|q| q[0])
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .min(w as f32 - 1.);
            let y0 = p
                .iter()
                .map(|q| q[1])
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.) as u32;
            let y1 = p
                .iter()
                .map(|q| q[1])
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .min(h as f32 - 1.);
            if x1 < 0. || y1 < 0. {
                continue;
            }
            for y in y0..=y1 as u32 {
                for x in x0..=x1 as u32 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let edge = |a: [f32; 2], b: [f32; 2]| {
                        ((b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])) / area
                    };
                    let b = [edge(p[1], p[2]), edge(p[2], p[0]), edge(p[0], p[1])];
                    if b.iter().any(|v| *v < 0.) {
                        continue;
                    }
                    let d = b[0] * z[0] + b[1] * z[1] + b[2] * z[2];
                    let at = (y * w + x) as usize;
                    if d <= depth[at] {
                        continue;
                    }
                    let texel = match (s.image, s.uvs.and_then(|u| u.get(k))) {
                        (Some(image), Some(uv)) => image.at([
                            b[0] * uv[0][0] + b[1] * uv[1][0] + b[2] * uv[2][0],
                            b[0] * uv[0][1] + b[1] * uv[1][1] + b[2] * uv[2][1],
                        ]),
                        _ => [SKIN[0], SKIN[1], SKIN[2], 1.].map(to_srgb),
                    };
                    if s.cutout && texel[3] < 0.4 {
                        continue;
                    }
                    let mut n = (s.normals[t[0]] * b[0] as f64
                        + s.normals[t[1]] * b[1] as f64
                        + s.normals[t[2]] * b[2] as f64)
                        .normalize_or_zero();
                    if n.z < 0. {
                        n = -n;
                    }
                    let lit = (0.32 + 0.78 * n.dot(light).max(0.)) as f32;
                    depth[at] = d;
                    colour[at] = [
                        to_srgb(to_linear(texel[0]) * lit),
                        to_srgb(to_linear(texel[1]) * lit),
                        to_srgb(to_linear(texel[2]) * lit),
                        1.,
                    ];
                }
            }
        }
    }
    // Averaged down for smooth edges.
    let [ow, oh] = size.map(|s| s.max(1));
    let mut out = Vec::with_capacity((4 * ow * oh) as usize);
    for y in 0..oh {
        for x in 0..ow {
            let mut sum = [0f32; 4];
            for dy in 0..SUPER {
                for dx in 0..SUPER {
                    let c = colour[((y * SUPER + dy) * w + x * SUPER + dx) as usize];
                    for k in 0..3 {
                        sum[k] += c[k] * c[3];
                    }
                    sum[3] += c[3];
                }
            }
            let alpha = sum[3] / (SUPER * SUPER) as f32;
            let rgb = [0, 1, 2].map(|k| if sum[3] > 0. { sum[k] / sum[3] } else { 0. });
            out.extend(
                [rgb[0], rgb[1], rgb[2], alpha].map(|v| (v * 255.).round().clamp(0., 255.) as u8),
            );
        }
    }
    out
}
