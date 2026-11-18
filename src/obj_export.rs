//! OBJ export with a material library: the garment's pieces with their fabrics, optionally
//! thick, in one object or one per piece, with the pattern packed into the unit square or
//! kept at its real size, in the chosen units and up axis, and the avatar beside it.
//!
//! Vertices stay split along seams: ClothCraft's seams join pieces along their edges rather
//! than vertex to vertex, so there is no vertex pair to weld. Faces are wound
//! counter-clockwise seen from the fabric's front face.
use crate::{
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate, appearance::FabricAppearance},
    gltf_export::{front_triangles, texture_coordinates},
    model::State,
};
use glam::{DVec2, DVec3};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// Length units of the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Units {
    Millimetres,
    #[default]
    Centimetres,
    Metres,
    Inches,
}

impl Units {
    pub const ALL: [Self; 4] = [
        Self::Millimetres,
        Self::Centimetres,
        Self::Metres,
        Self::Inches,
    ];
    pub fn per_metre(self) -> f64 {
        match self {
            Self::Millimetres => 1000.,
            Self::Centimetres => 100.,
            Self::Metres => 1.,
            Self::Inches => 1000. / 25.4,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Millimetres => "mm",
            Self::Centimetres => "cm",
            Self::Metres => "m",
            Self::Inches => "inch",
        }
    }
}

/// How to write the file.
#[derive(Clone, Debug, Default)]
pub struct ObjOptions {
    /// Both faces of the cloth apart by the fabric's look thickness (1 mm when it has
    /// none), joined by a rim along each piece's outline.
    pub thick: bool,
    /// Every piece packed into the unit square of texture space, at one scale.
    pub unified_uv: bool,
    pub units: Units,
    /// Z up instead of Y up.
    pub z_up: bool,
    /// One object per piece instead of one object with a group per piece.
    pub per_piece: bool,
    /// Only these pieces; all of them when absent.
    pub pieces: Option<BTreeSet<String>>,
}

/// What to write.
pub struct ObjScene<'a> {
    pub template: &'a GarmentTemplate,
    pub garment: &'a CompiledGarment,
    pub state: &'a State,
    /// Image files of fabric textures, by asset ID.
    pub images: &'a [(String, PathBuf)],
    pub avatar: Option<&'a SurfaceMesh>,
}

/// What was written.
#[derive(Debug, PartialEq)]
pub struct ObjSummary {
    pub vertices: usize,
    pub faces: usize,
    pub textures: usize,
}

/// Thickness used for a fabric without a look thickness, metres.
const DEFAULT_THICKNESS_M: f64 = 0.001;

/// Each piece's texture coordinates moved into one unit square, shelf by shelf, all at the
/// same scale and slightly apart.
fn pack(boxes: &[(DVec2, DVec2)]) -> Vec<(DVec2, f64)> {
    let gap = boxes
        .iter()
        .map(|(lo, hi)| (*hi - *lo).max_element())
        .fold(0., f64::max)
        * 0.02;
    let area: f64 = boxes
        .iter()
        .map(|(lo, hi)| (hi.x - lo.x + gap) * (hi.y - lo.y + gap))
        .sum();
    let row = area.sqrt() * 1.2;
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|a, b| {
        let h = |k: usize| boxes[k].1.y - boxes[k].0.y;
        h(*b).total_cmp(&h(*a))
    });
    let mut offsets = vec![DVec2::ZERO; boxes.len()];
    let (mut x, mut y, mut shelf) = (0., 0., 0f64);
    for k in order {
        let size = boxes[k].1 - boxes[k].0;
        if x > 0. && x + size.x > row {
            x = 0.;
            y += shelf + gap;
            shelf = 0.;
        }
        offsets[k] = DVec2::new(x, y) - boxes[k].0;
        x += size.x + gap;
        shelf = shelf.max(size.y);
    }
    let extent = boxes
        .iter()
        .zip(&offsets)
        .map(|((_, hi), o)| (*hi + *o).max_element())
        .fold(0., f64::max)
        .max(1e-12);
    offsets.into_iter().map(|o| (o, 1. / extent)).collect()
}

fn mtl_color(rgb: [u8; 3]) -> String {
    format!(
        "{:.4} {:.4} {:.4}",
        rgb[0] as f64 / 255.,
        rgb[1] as f64 / 255.,
        rgb[2] as f64 / 255.
    )
}

/// Edges of a piece's outline, directed as in the front-wound triangle that has them.
fn outline(triangles: &[[usize; 3]]) -> Vec<[usize; 2]> {
    let mut count: HashMap<(usize, usize), usize> = HashMap::new();
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    triangles
        .iter()
        .flat_map(|t| (0..3).map(move |k| [t[k], t[(k + 1) % 3]]))
        .filter(|[a, b]| count[&((*a).min(*b), (*a).max(*b))] == 1)
        .collect()
}

/// Area-weighted vertex normals of front-wound triangles.
fn vertex_normals(x: &[DVec3], triangles: &[[usize; 3]]) -> Vec<DVec3> {
    let mut normal = vec![DVec3::ZERO; x.len()];
    for t in triangles {
        let face = (x[t[1]] - x[t[0]]).cross(x[t[2]] - x[t[0]]);
        for i in t {
            normal[*i] += face;
        }
    }
    normal.iter().map(|n| n.normalize_or(DVec3::Y)).collect()
}

/// Write `path` and a material library beside it (same name, `.mtl`), with fabric texture
/// images copied next to them.
pub fn write(scene: &ObjScene, options: &ObjOptions, path: &Path) -> Result<ObjSummary, String> {
    let garment = scene.garment;
    if scene.state.x.len() != garment.mesh.uv.len() {
        return Err("The cloth does not match the garment".into());
    }
    let pieces: Vec<_> = garment
        .panels
        .iter()
        .filter(|p| options.pieces.as_ref().is_none_or(|s| s.contains(&p.id)))
        .collect();
    if pieces.is_empty() {
        return Err("No piece to export".into());
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .ok_or("The file needs a name")?
        .to_string_lossy()
        .into_owned();
    let mtl_name = format!("{stem}.mtl");
    let appearances = crate::garment::appearance::bind(scene.template, garment);
    let mut fabric_of: HashMap<usize, String> = HashMap::new();
    for panel in &garment.panels {
        if let Some(source) = scene.template.panels.iter().find(|p| p.id == panel.id) {
            fabric_of.insert(panel.material, source.fabric.clone());
        }
    }
    let scale = options.units.per_metre();
    let turn = |p: DVec3| {
        if options.z_up {
            DVec3::new(p.x, -p.z, p.y)
        } else {
            p
        }
    };
    // Texture coordinates, packed per piece when asked.
    let mut uv = texture_coordinates(scene.template, garment);
    if options.unified_uv {
        let boxes: Vec<(DVec2, DVec2)> = pieces
            .iter()
            .map(|p| {
                uv[p.vertices.clone()].iter().fold(
                    (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                    |(lo, hi), q| (lo.min(*q), hi.max(*q)),
                )
            })
            .collect();
        for (piece, (offset, s)) in pieces.iter().zip(pack(&boxes)) {
            for q in &mut uv[piece.vertices.clone()] {
                *q = (*q + offset) * s;
            }
        }
    }
    let triangles = front_triangles(scene.template, garment);
    let x = &scene.state.x;
    let normal = vertex_normals(x, &triangles);
    let mut obj = format!(
        "# ClothCraft garment; units: {}\nmtllib {mtl_name}\n",
        options.units.label()
    );
    // Lines written so far of each kind: OBJ indices count from one across the file.
    let (mut v_count, mut vt_count, mut vn_count, mut faces) = (0usize, 0usize, 0usize, 0usize);
    let mut used: BTreeMap<String, Option<FabricAppearance>> = BTreeMap::new();
    if !options.per_piece {
        obj.push_str("o garment\n");
    }
    for piece in &pieces {
        let fabric = fabric_of
            .get(&piece.material)
            .cloned()
            .unwrap_or_else(|| format!("fabric_{}", piece.material));
        used.insert(fabric.clone(), appearances[piece.material].clone());
        let half = appearances[piece.material]
            .as_ref()
            .map(|a| a.render_thickness_mm / 2000.)
            .filter(|h| *h > 0.)
            .unwrap_or(DEFAULT_THICKNESS_M / 2.);
        let local: Vec<usize> = piece.vertices.clone().collect();
        let index: HashMap<usize, usize> = local.iter().enumerate().map(|(k, i)| (*i, k)).collect();
        // The front copy, and the back copy when thick.
        let sides: &[f64] = if options.thick { &[1., -1.] } else { &[0.] };
        let count = local.len();
        let kind = if options.per_piece { "o" } else { "g" };
        writeln!(obj, "{kind} {}", piece.id).unwrap();
        for side in sides {
            for i in &local {
                let p = turn((x[*i] + normal[*i] * half * side) * scale);
                writeln!(obj, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z).unwrap();
            }
        }
        for i in &local {
            writeln!(obj, "vt {:.6} {:.6}", uv[*i].x, uv[*i].y).unwrap();
        }
        for side in sides {
            let sign = if *side < 0. { -1. } else { 1. };
            for i in &local {
                let n = turn(normal[*i] * sign);
                writeln!(obj, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z).unwrap();
            }
        }
        writeln!(obj, "usemtl {fabric}").unwrap();
        let corner = |s: usize, k: usize| {
            format!(
                "{}/{}/{}",
                v_count + s * count + k + 1,
                vt_count + k + 1,
                vn_count + s * count + k + 1
            )
        };
        for s in 0..sides.len() {
            for t in &triangles[piece.triangles.clone()] {
                let k = t.map(|i| index[&i]);
                // The back copy faces the other way.
                let k = if s == 1 { [k[0], k[2], k[1]] } else { k };
                writeln!(
                    obj,
                    "f {} {} {}",
                    corner(s, k[0]),
                    corner(s, k[1]),
                    corner(s, k[2])
                )
                .unwrap();
                faces += 1;
            }
        }
        if options.thick {
            // A rim along the outline joins the front copy to the back one, facing out.
            let rim = |s: usize, k: usize| {
                format!("{}/{}", v_count + s * count + k + 1, vt_count + k + 1)
            };
            for [a, b] in outline(&triangles[piece.triangles.clone()]) {
                let (a, b) = (index[&a], index[&b]);
                writeln!(obj, "f {} {} {}", rim(0, a), rim(1, a), rim(1, b)).unwrap();
                writeln!(obj, "f {} {} {}", rim(0, a), rim(1, b), rim(0, b)).unwrap();
                faces += 2;
            }
        }
        v_count += count * sides.len();
        vt_count += count;
        vn_count += count * sides.len();
    }
    if let Some(avatar) = scene.avatar {
        let normals = vertex_normals(&avatar.vertices, &avatar.triangles);
        obj.push_str("o avatar\n");
        for p in &avatar.vertices {
            let p = turn(*p * scale);
            writeln!(obj, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z).unwrap();
        }
        for n in &normals {
            let n = turn(*n);
            writeln!(obj, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z).unwrap();
        }
        obj.push_str("usemtl avatar\n");
        for t in &avatar.triangles {
            let [a, b, c] = t.map(|i| (v_count + i + 1, vn_count + i + 1));
            writeln!(obj, "f {}//{} {}//{} {}//{}", a.0, a.1, b.0, b.1, c.0, c.1).unwrap();
            faces += 1;
        }
        v_count += avatar.vertices.len();
    }
    let mut mtl = String::from("# ClothCraft fabrics\n");
    let mut textures = 0;
    for (fabric, appearance) in &used {
        writeln!(mtl, "\nnewmtl {fabric}").unwrap();
        let (color, roughness) = appearance
            .as_ref()
            .map_or(([232, 234, 240], 0.8), |a| (a.color_srgb, a.roughness));
        writeln!(mtl, "Kd {}", mtl_color(color)).unwrap();
        writeln!(mtl, "Ka 0 0 0\nKs 0.04 0.04 0.04").unwrap();
        writeln!(mtl, "Ns {:.1}", (1. - roughness) * 900. + 10.).unwrap();
        writeln!(mtl, "d 1\nillum 2").unwrap();
        // Packed coordinates no longer tile an image: the texture goes with real-size ones.
        if let Some(texture) = appearance.as_ref().and_then(|a| a.texture.as_ref())
            && !options.unified_uv
        {
            let source = scene
                .images
                .iter()
                .find(|(id, _)| *id == texture.asset_id)
                .map(|(_, p)| p)
                .ok_or_else(|| format!("Texture image {} is missing", texture.asset_id))?;
            let extension = source
                .extension()
                .map_or("png".into(), |e| e.to_string_lossy().to_ascii_lowercase());
            let name = format!("{stem}_{}.{extension}", texture.asset_id);
            std::fs::copy(source, dir.join(&name))
                .map_err(|e| format!("{}: {e}", source.display()))?;
            writeln!(mtl, "map_Kd {name}").unwrap();
            textures += 1;
        }
    }
    if scene.avatar.is_some() {
        mtl.push_str("\nnewmtl avatar\nKd 0.45 0.45 0.47\nKa 0 0 0\nKs 0.02 0.02 0.02\nNs 100\nd 1\nillum 2\n");
    }
    std::fs::write(path, obj).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&mtl_name), mtl).map_err(|e| e.to_string())?;
    Ok(ObjSummary {
        vertices: v_count,
        faces,
        textures,
    })
}
