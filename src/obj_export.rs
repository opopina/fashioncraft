//! OBJ export with a material library: the garment's pieces with their fabrics, optionally
//! thick, in one object or one per piece, with the pattern packed into the unit square or
//! kept at its real size, in the chosen units and up axis, and the avatar beside it.
//!
//! Vertices stay split along seams unless welding is asked for: ClothCraft's seams join
//! pieces along their edges rather than vertex to vertex, so welding first adds each side's
//! vertices to the other side's edge. Faces are wound counter-clockwise seen from the
//! fabric's front face.
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
    /// Neighbouring triangles joined into quads where they make a flat, convex one.
    pub quads: bool,
    /// Each seam's two edges joined vertex to vertex (see [`crate::weld`]), so the exported
    /// pieces are one connected mesh.
    pub weld: bool,
    /// Each piece remeshed as quads of this size, metres (see [`crate::retopology`]); welding
    /// and quads then do not apply.
    pub retopology: Option<f64>,
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
    /// The garment's look baked into one image: every piece then uses it, at its coordinates.
    pub atlas: Option<&'a crate::bake::Atlas>,
}

/// The material of a baked garment.
const BAKED: &str = "baked";

/// What was written.
#[derive(Debug, PartialEq)]
pub struct ObjSummary {
    pub vertices: usize,
    pub faces: usize,
    pub textures: usize,
}

/// The exported pieces' mesh: each vertex's texture coordinates and position group, each
/// group's position, and per piece its vertices, its triangles (for normals and the outline)
/// and the faces written.
struct Surface {
    uv: Vec<DVec2>,
    group: Vec<usize>,
    position: Vec<DVec3>,
    local: Vec<Vec<usize>>,
    triangles: Vec<Vec<[usize; 3]>>,
    faces: Vec<Vec<Vec<usize>>>,
}

/// Thickness used for a fabric without a look thickness, metres.
const DEFAULT_THICKNESS_M: f64 = 0.001;

/// `triangles` (wound alike) as faces, neighbouring pairs joined into quads where the two make
/// a convex, nearly flat quad with every corner between 45° and 135°. The most nearly
/// rectangular quads are taken first; triangles left over stay triangles.
/// Every face keeps the triangles' winding.
pub fn quads(x: &[DVec3], triangles: &[[usize; 3]]) -> Vec<Vec<usize>> {
    let mut edges: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for (t, tri) in triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push((t, k));
        }
    }
    let normal = |tri: &[usize; 3]| {
        (x[tri[1]] - x[tri[0]])
            .cross(x[tri[2]] - x[tri[0]])
            .normalize_or_zero()
    };
    let mut candidates = vec![];
    for pair in edges.values() {
        let [(t1, k1), (t2, k2)] = pair[..] else {
            continue;
        };
        let (a, b) = (&triangles[t1], &triangles[t2]);
        // Wound alike: the shared edge runs opposite ways in the two.
        if a[k1] != b[(k2 + 1) % 3] {
            continue;
        }
        if normal(a).dot(normal(b)) < 30f64.to_radians().cos() {
            continue;
        }
        let (p, q, r) = (a[k1], a[(k1 + 1) % 3], a[(k1 + 2) % 3]);
        let s = b[(k2 + 2) % 3];
        let quad = [p, s, q, r];
        // Convex: every corner turns the same way about the mean normal; score by how far its
        // corners are from right angles.
        let n = normal(a) + normal(b);
        let mut convex = true;
        let mut score = 0.;
        for i in 0..4 {
            let (o, prev, next) = (x[quad[i]], x[quad[(i + 3) % 4]], x[quad[(i + 1) % 4]]);
            let (u, v) = (next - o, prev - o);
            let off = (u.angle_between(v) - std::f64::consts::FRAC_PI_2).abs();
            // Convex, and no corner sharper than 45° or flatter than 135°.
            convex &= u.cross(v).dot(n) > 0. && off <= std::f64::consts::FRAC_PI_4;
            score += off;
        }
        if convex {
            candidates.push((score, t1, t2, quad));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut used = vec![false; triangles.len()];
    let mut faces = vec![];
    for (_, t1, t2, quad) in candidates {
        if used[t1] || used[t2] {
            continue;
        }
        used[t1] = true;
        used[t2] = true;
        faces.push(quad.to_vec());
    }
    for (t, tri) in triangles.iter().enumerate() {
        if !used[t] {
            faces.push(tri.to_vec());
        }
    }
    faces
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
    let exported: Vec<bool> = garment
        .panels
        .iter()
        .map(|p| options.pieces.as_ref().is_none_or(|s| s.contains(&p.id)))
        .collect();
    let pieces: Vec<_> = garment
        .panels
        .iter()
        .enumerate()
        .filter(|(k, _)| exported[*k])
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
            .map(|(_, p)| {
                uv[p.vertices.clone()].iter().fold(
                    (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                    |(lo, hi), q| (lo.min(*q), hi.max(*q)),
                )
            })
            .collect();
        for ((_, piece), (offset, s)) in pieces.iter().zip(crate::bake::pack(&boxes)) {
            for q in &mut uv[piece.vertices.clone()] {
                *q = (*q + offset) * s;
            }
        }
    }
    // A baked look brings its own coordinates (OBJ counts v from the image's bottom).
    if let Some(atlas) = scene.atlas {
        if atlas.uv.len() != uv.len() {
            return Err("The baked image was made for another garment".into());
        }
        for (q, a) in uv.iter_mut().zip(&atlas.uv) {
            *q = DVec2::new(a.x, 1. - a.y);
        }
    }
    let triangles = front_triangles(scene.template, garment);
    // What the pieces become: vertices with a texture coordinate each, grouped into shared
    // positions (a welded vertex is written once), and faces.
    let surface = if let Some(size) = options.retopology {
        let retopo = crate::retopology::retopologize(garment, &triangles, size, &exported)?;
        let fan = |f: &Vec<usize>| -> Vec<[usize; 3]> {
            (1..f.len() - 1).map(|i| [f[0], f[i], f[i + 1]]).collect()
        };
        Surface {
            uv: retopo.sample(&uv),
            group: (0..retopo.points.len()).collect(),
            position: retopo.sample(&scene.state.x),
            local: retopo.pieces.iter().map(|p| p.vertices.clone()).collect(),
            triangles: retopo
                .pieces
                .iter()
                .map(|p| p.faces.iter().flat_map(fan).collect())
                .collect(),
            faces: retopo.pieces.into_iter().map(|p| p.faces).collect(),
        }
    } else {
        let welded = if options.weld {
            crate::weld::weld(garment, &triangles, &exported)
        } else {
            crate::weld::Welded::none(garment, &triangles)
        };
        let position = welded.positions(&welded.extend(&scene.state.x, |a, b, t| a.lerp(b, t)));
        let x: Vec<DVec3> = welded.group.iter().map(|g| position[*g]).collect();
        Surface {
            uv: welded.extend(&uv, |a, b, t| a.lerp(b, t)),
            local: garment
                .panels
                .iter()
                .zip(&welded.piece_added)
                .map(|(p, added)| p.vertices.clone().chain(added.iter().copied()).collect())
                .collect(),
            faces: welded
                .triangles
                .iter()
                .map(|t| {
                    if options.quads {
                        quads(&x, t)
                    } else {
                        t.iter().map(|t| t.to_vec()).collect()
                    }
                })
                .collect(),
            group: welded.group,
            position,
            triangles: welded.triangles,
        }
    };
    let (uv, group, position) = (&surface.uv, &surface.group, &surface.position);
    let corners: Vec<[usize; 3]> = pieces
        .iter()
        .flat_map(|(k, _)| surface.triangles[*k].iter().map(|t| t.map(|i| group[i])))
        .collect();
    let normal = vertex_normals(position, &corners);
    // Edges of the exported surface's outline, between groups: a welded seam is not one.
    let mut uses: HashMap<(usize, usize), usize> = HashMap::new();
    for t in &corners {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut obj = format!(
        "# FashionCraft garment; units: {}\nmtllib {mtl_name}\n",
        options.units.label()
    );
    // Lines written so far of each kind: OBJ indices count from one across the file.
    let (mut v_count, mut vt_count, mut vn_count, mut faces) = (0usize, 0usize, 0usize, 0usize);
    // Each group's position and normal lines, front and back, once written.
    let mut v_line = vec![[0usize; 2]; position.len()];
    let mut vn_line = vec![[0usize; 2]; position.len()];
    let mut used: BTreeMap<String, Option<FabricAppearance>> = BTreeMap::new();
    if !options.per_piece {
        obj.push_str("o garment\n");
    }
    for (k, piece) in &pieces {
        let fabric = if scene.atlas.is_some() {
            BAKED.to_string()
        } else {
            fabric_of
                .get(&piece.material)
                .cloned()
                .unwrap_or_else(|| format!("fabric_{}", piece.material))
        };
        if scene.atlas.is_none() {
            used.insert(fabric.clone(), appearances[piece.material].clone());
        }
        let half = appearances[piece.material]
            .as_ref()
            .map(|a| a.render_thickness_mm / 2000.)
            .filter(|h| *h > 0.)
            .unwrap_or(DEFAULT_THICKNESS_M / 2.);
        let local = &surface.local[*k];
        let index: HashMap<usize, usize> = local.iter().enumerate().map(|(k, i)| (*i, k)).collect();
        // The front copy, and the back copy when thick.
        let sides: &[f64] = if options.thick { &[1., -1.] } else { &[0.] };
        let kind = if options.per_piece { "o" } else { "g" };
        writeln!(obj, "{kind} {}", piece.id).unwrap();
        for (s, side) in sides.iter().enumerate() {
            for i in local {
                let g = group[*i];
                if v_line[g][s] == 0 {
                    let p = turn((position[g] + normal[g] * half * side) * scale);
                    writeln!(obj, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z).unwrap();
                    v_count += 1;
                    v_line[g][s] = v_count;
                }
            }
        }
        for i in local {
            writeln!(obj, "vt {:.6} {:.6}", uv[*i].x, uv[*i].y).unwrap();
        }
        for (s, side) in sides.iter().enumerate() {
            let sign = if *side < 0. { -1. } else { 1. };
            for i in local {
                let g = group[*i];
                if vn_line[g][s] == 0 {
                    let n = turn(normal[g] * sign);
                    writeln!(obj, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z).unwrap();
                    vn_count += 1;
                    vn_line[g][s] = vn_count;
                }
            }
        }
        writeln!(obj, "usemtl {fabric}").unwrap();
        let corner = |s: usize, k: usize| {
            let g = group[local[k]];
            format!("{}/{}/{}", v_line[g][s], vt_count + k + 1, vn_line[g][s])
        };
        for s in 0..sides.len() {
            for face in &surface.faces[*k] {
                let mut k: Vec<usize> = face.iter().map(|i| index[i]).collect();
                // The back copy faces the other way.
                if s == 1 {
                    k[1..].reverse();
                }
                let corners: Vec<String> = k.iter().map(|k| corner(s, *k)).collect();
                writeln!(obj, "f {}", corners.join(" ")).unwrap();
                faces += 1;
            }
        }
        if options.thick {
            // A rim along the outline joins the front copy to the back one, facing out.
            let rim =
                |s: usize, k: usize| format!("{}/{}", v_line[group[local[k]]][s], vt_count + k + 1);
            for [a, b] in outline(&surface.triangles[*k]) {
                if uses[&(group[a].min(group[b]), group[a].max(group[b]))] != 1 {
                    continue;
                }
                let (a, b) = (index[&a], index[&b]);
                writeln!(obj, "f {} {} {}", rim(0, a), rim(1, a), rim(1, b)).unwrap();
                writeln!(obj, "f {} {} {}", rim(0, a), rim(1, b), rim(0, b)).unwrap();
                faces += 2;
            }
        }
        vt_count += local.len();
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
    let mut mtl = String::from("# FashionCraft fabrics\n");
    let mut textures = 0;
    if let Some(atlas) = scene.atlas {
        let name = format!("{stem}_{BAKED}.png");
        std::fs::write(
            dir.join(&name),
            crate::bake::png(atlas.size, atlas.size, &atlas.rgba),
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            mtl,
            "\nnewmtl {BAKED}\nKd 1 1 1\nKa 0 0 0\nKs 0.04 0.04 0.04\nNs 200\nd 1\nillum 2\nmap_Kd {name}"
        )
        .unwrap();
        textures += 1;
    }
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
