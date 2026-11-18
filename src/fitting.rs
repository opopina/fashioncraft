//! Small reproducible fitting project, distinct from legacy single-panel Project.
use crate::{
    avatar::{MeshCollider, SurfaceMesh},
    garment::{
        CompiledGarment, GarmentTemplate,
        presets::{self, GarmentKind},
    },
    model::State,
    solver::{Metrics, ReferenceSolver, SolverSettings},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FittingProject {
    pub version: u32,
    pub template: GarmentTemplate,
    /// Relative paths are resolved against the fitting-project JSON directory.
    pub avatar_path: PathBuf,
    pub contact_stiffness: f64,
    #[serde(default)]
    pub contact_clearance: f64,
    #[serde(default)]
    pub prearrange: bool,
    /// Assembly relaxation: each incremental minimization starts at zero velocity.
    #[serde(default)]
    pub quasistatic: bool,
    pub solver: SolverSettings,
    /// Interactive backend settings. Absent in historical v1 documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation: Option<crate::vbd::VbdSettings>,
    pub state: Option<State>,
    /// Detect stale vertex caches after remeshing or editing the template.
    #[serde(default)]
    pub mesh_signature: Option<String>,
    /// Reference image under the 2D board, for tracing. Absent in older documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_background: Option<BoardBackground>,
    /// Named 3D camera views saved with the project. Absent in older documents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved_views: Vec<SavedView>,
    /// Ruler origin and guidelines of the 2D board. Absent in older documents.
    #[serde(default, skip_serializing_if = "BoardGuides::is_empty")]
    pub board_guides: BoardGuides,
    /// Image files of the graphics on pieces and of fabric textures, by asset ID. Absent in
    /// older documents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_assets: Vec<ImageAsset>,
    /// Colours saved for reuse on fabrics, as sRGB bytes. Absent in older documents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub color_palette: Vec<[u8; 3]>,
}

/// An image a graphic placement or a fabric texture refers to by `id`. Relative paths resolve against the
/// project JSON directory; saving copies the image into the project's `.assets` folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageAsset {
    pub id: String,
    pub path: PathBuf,
}

/// Asset IDs must be unique and well formed, like the placements that use them.
fn validate_image_assets(assets: &[ImageAsset]) -> Result<(), String> {
    for (k, asset) in assets.iter().enumerate() {
        let well_formed = !asset.id.is_empty()
            && asset.id.len() <= 80
            && asset
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !well_formed {
            return Err("Image asset IDs must use 1–80 ASCII letters, digits, '_' or '-'".into());
        }
        if assets[..k].iter().any(|other| other.id == asset.id) {
            return Err(format!("Duplicate image asset ID {}", asset.id));
        }
    }
    Ok(())
}

/// Direction of a 2D board guideline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideAxis {
    /// A line of constant v, dragged from the top ruler.
    Horizontal,
    /// A line of constant u, dragged from the left ruler.
    Vertical,
}

/// One guideline on the 2D board: an infinite line pattern points snap to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuideLine {
    pub axis: GuideAxis,
    /// Board coordinate of the line (v for horizontal, u for vertical), in metres.
    pub position_m: f64,
    /// A locked guideline cannot be dragged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

/// Rulers and guidelines of the 2D board. Presentation only, like the board background.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoardGuides {
    /// Where the rulers read zero, on the board, in metres.
    pub origin_m: [f64; 2],
    pub lines: Vec<GuideLine>,
}

impl BoardGuides {
    /// At most this many guidelines per project.
    pub const MAX_LINES: usize = 256;

    pub fn is_empty(&self) -> bool {
        self.origin_m == [0., 0.] && self.lines.is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.origin_m.iter().all(|v| v.is_finite()) {
            return Err("The ruler origin must be finite".into());
        }
        if self.lines.len() > Self::MAX_LINES {
            return Err(format!("At most {} guidelines", Self::MAX_LINES));
        }
        if self.lines.iter().any(|l| !l.position_m.is_finite()) {
            return Err("Guideline positions must be finite".into());
        }
        Ok(())
    }
}

/// A named 3D camera view stored with the project: an orbit around `target_m`.
/// Presentation only, like the board background.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedView {
    pub name: String,
    /// Orbit angle around the vertical axis; 0 looks at the front (+z side).
    pub yaw_degrees: f64,
    /// Elevation, from −90 (from below) to 90 (from above).
    pub pitch_degrees: f64,
    /// Distance from the target to the eye, in metres.
    pub distance_m: f64,
    /// Orbit centre, in metres.
    pub target_m: [f64; 3],
    pub orthographic: bool,
    /// Vertical field of view, in degrees.
    pub fov_degrees: f64,
}

impl SavedView {
    /// At most this many views per project.
    pub const MAX_VIEWS: usize = 64;
    /// Longest view name, in characters.
    pub const MAX_NAME_CHARS: usize = 64;

    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > Self::MAX_NAME_CHARS {
            return Err("A saved view needs a name of 1 to 64 characters".into());
        }
        let numbers = [
            self.yaw_degrees,
            self.pitch_degrees,
            self.distance_m,
            self.fov_degrees,
        ];
        if !numbers.iter().chain(&self.target_m).all(|v| v.is_finite()) {
            return Err(format!("Saved view “{name}” has a non-finite value"));
        }
        if !(-90.0..=90.0).contains(&self.pitch_degrees) {
            return Err(format!("Saved view “{name}”: pitch must be within ±90°"));
        }
        if !(0.01..=1000.0).contains(&self.distance_m) {
            return Err(format!(
                "Saved view “{name}”: distance must be 0.01 to 1000 m"
            ));
        }
        if !(10.0..=120.0).contains(&self.fov_degrees) {
            return Err(format!(
                "Saved view “{name}”: field of view must be 10° to 120°"
            ));
        }
        Ok(())
    }
}

fn validate_views(views: &[SavedView]) -> Result<(), String> {
    if views.len() > SavedView::MAX_VIEWS {
        return Err(format!("At most {} saved views", SavedView::MAX_VIEWS));
    }
    let mut names = std::collections::BTreeSet::new();
    for view in views {
        view.validate()?;
        if !names.insert(view.name.trim().to_lowercase()) {
            return Err(format!("Two saved views are called “{}”", view.name.trim()));
        }
    }
    Ok(())
}

/// Reference image drawn under the 2D board, for tracing a scanned or photographed pattern.
/// Presentation only: never part of the garment, the mesh, the masses or the simulation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardBackground {
    /// Image file. Relative paths resolve against the project JSON directory; saving copies
    /// the image into the project's `.assets` folder.
    pub path: PathBuf,
    /// Board position of the image's top-left corner, in metres (v grows downward).
    pub origin_m: [f64; 2],
    /// Real width the image spans on the board, in metres; the height follows its aspect.
    pub width_m: f64,
    /// 0 is invisible, 1 is opaque.
    pub opacity: f64,
    /// Rotation on the board around the image centre, in degrees, in the same sense as a
    /// piece's board rotation.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation_degrees: f64,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

impl BoardBackground {
    /// Board widths from 1 cm to 20 m; anything else is a typo or a corrupt file.
    pub const WIDTH_RANGE_M: std::ops::RangeInclusive<f64> = 0.01..=20.0;

    pub fn validate(&self) -> Result<(), String> {
        if !self.origin_m.iter().all(|v| v.is_finite()) || !self.rotation_degrees.is_finite() {
            return Err("Background image position and rotation must be finite".into());
        }
        if !self.width_m.is_finite() || !Self::WIDTH_RANGE_M.contains(&self.width_m) {
            return Err("Background image width must be between 10 mm and 20 m".into());
        }
        if !(0.0..=1.0).contains(&self.opacity) {
            return Err("Background image opacity must be between 0 and 1".into());
        }
        Ok(())
    }
}
pub struct FittingScene {
    pub garment: Arc<CompiledGarment>,
    pub collider: Arc<MeshCollider>,
    pub state: State,
    pub quasistatic: bool,
    /// Conservative adaptive step cap for this run; reset when loading a project.
    pub next_dt: f64,
}
impl FittingScene {
    /// A failed Newton solve never silently advances time. Retry from the exact
    /// previous state with a smaller dt; the accepted dt is recorded in Metrics.
    pub fn step(&mut self, settings: &SolverSettings) -> Result<(Metrics, usize), String> {
        settings.validate()?;
        let previous = self.state.clone();
        let mut error = String::new();
        for attempt in 0..7 {
            self.state = previous.clone();
            if self.quasistatic {
                self.state.v.fill(glam::DVec3::ZERO);
            }
            let mut settings = settings.clone();
            settings.dt = settings.dt.min(self.next_dt) / 2.0_f64.powi(attempt as i32);
            match ReferenceSolver::step_garment_with_collider(
                &self.garment,
                &mut self.state,
                &settings,
                Some(self.collider.as_ref()),
            ) {
                Ok(mut m) if m.converged => {
                    self.next_dt = settings.dt;
                    if self.quasistatic {
                        self.state.v.fill(glam::DVec3::ZERO);
                        m.kinetic_j = 0.0;
                    }
                    return Ok((m, attempt));
                }
                Ok(m) => {
                    error = format!(
                        "Newton iteration limit: residual {} / {}",
                        m.residual_rms_n, m.tolerance_n
                    )
                }
                Err(e) => error = e,
            }
        }
        self.state = previous;
        Err(format!(
            "Fitting retries exhausted; state unchanged: {error}"
        ))
    }
}
impl FittingProject {
    pub fn basic(kind: GarmentKind, subdivisions: u32, avatar_path: PathBuf) -> Self {
        Self {
            version: 1,
            template: presets::basic(kind, subdivisions),
            avatar_path,
            contact_stiffness: 500_000.0,
            contact_clearance: 0.0,
            // Planar panels start outside the full avatar bounding box. The old
            // per-vertex silhouette warp could intersect face interiors.
            prearrange: false,
            quasistatic: true,
            solver: SolverSettings {
                dt: 1.0 / 240.0,
                damping: 20.0,
                newton_iterations: 30,
                pcg_iterations: 500,
                ..Default::default()
            },
            simulation: None,
            state: None,
            mesh_signature: None,
            board_background: None,
            saved_views: Vec::new(),
            board_guides: BoardGuides::default(),
            image_assets: Vec::new(),
            color_palette: Vec::new(),
        }
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut project: Self =
            serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        validate_views(&project.saved_views)?;
        project.board_guides.validate()?;
        let directory = path.parent().unwrap_or(Path::new("."));
        if project.avatar_path.is_relative() {
            project.avatar_path = directory.join(&project.avatar_path);
        }
        if let Some(background) = project.board_background.as_mut() {
            background.validate()?;
            if background.path.is_relative() {
                background.path = directory.join(&background.path);
            }
        }
        validate_image_assets(&project.image_assets)?;
        for asset in &mut project.image_assets {
            if asset.path.is_relative() {
                asset.path = directory.join(&asset.path);
            }
        }
        Ok(project)
    }
    pub fn compile(&self) -> Result<FittingScene, String> {
        if self.version != 1 {
            return Err("Unsupported fitting project version".into());
        }
        self.solver.validate()?;
        if let Some(settings) = &self.simulation {
            settings.validate()?;
        }
        let garment = Arc::new(self.template.compile()?);
        if self.state.is_some()
            && self.mesh_signature.as_ref() != Some(&mesh_signature(&self.template, &garment)?)
        {
            return Err("Stale fitting cache: clear state and mesh_signature, then rebuild".into());
        }
        let surface = Arc::new(SurfaceMesh::load_obj(&self.avatar_path)?);
        let collider = Arc::new(MeshCollider::with_clearance(
            surface,
            self.contact_stiffness,
            self.contact_clearance,
        )?);
        let mut state = self
            .state
            .clone()
            .unwrap_or_else(|| garment.initial_state.clone());
        if self.state.is_none() && self.prearrange {
            for (panel, range) in self.template.panels.iter().zip(&garment.panels) {
                let Some(polygon) = &panel.polygon else {
                    continue;
                };
                let names: Vec<_> = self
                    .template
                    .seams
                    .iter()
                    .flat_map(|s| s.references())
                    .filter(|r| r.panel == panel.id)
                    .filter_map(|r| r.edge.as_ref())
                    .collect();
                let curves: Vec<_> = names
                    .iter()
                    .map(|name| {
                        polygon.edge_points(polygon.edges.iter().position(|e| e == *name).unwrap())
                    })
                    .collect::<Result<_, _>>()?;
                let side = panel.placement.translation.z.signum();
                for i in range.vertices.clone() {
                    let uv = garment.mesh.uv[i];
                    let mut distance = f64::INFINITY;
                    for curve in &curves {
                        for pair in curve.windows(2) {
                            let (a, b) = (pair[0], pair[1]);
                            let d = b - a;
                            let t = ((uv - a).dot(d) / d.length_squared()).clamp(0., 1.);
                            distance = distance.min(uv.distance(a + t * d));
                        }
                    }
                    let s = (distance / 0.075).clamp(0.0, 1.0);
                    let smooth = s * s * (3.0 - 2.0 * s);
                    let mut z = side * (0.004 + panel.placement.translation.z.abs() * smooth);
                    if let Some([lo, hi]) = collider.z_bounds(state.x[i].x, state.x[i].y) {
                        z = if side > 0.0 {
                            z.max(hi + 0.008)
                        } else {
                            z.min(lo - 0.008)
                        };
                    }
                    state.x[i].z = z;
                }
            }
        }
        if state.x.len() != garment.mesh.uv.len()
            || state.v.len() != state.x.len()
            || !state.time.is_finite()
            || state.x.iter().chain(&state.v).any(|p| !p.is_finite())
        {
            return Err("Invalid cached fitting state".into());
        }
        Ok(FittingScene {
            garment,
            collider,
            state,
            quasistatic: self.quasistatic,
            next_dt: self.solver.dt,
        })
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported fitting project version".into());
        }
        self.solver.validate()?;
        if let Some(settings) = &self.simulation {
            settings.validate()?;
        }
        let mut copy = self.clone();
        let garment = copy.template.compile()?;
        copy.mesh_signature = if copy.state.is_some() {
            Some(mesh_signature(&copy.template, &garment)?)
        } else {
            None
        };
        // Validate cached data before replacing a previously valid document.
        if let Some(state) = &copy.state {
            let n = garment.mesh.uv.len();
            if state.x.len() != n
                || state.v.len() != n
                || !state.time.is_finite()
                || state.x.iter().chain(&state.v).any(|p| !p.is_finite())
            {
                return Err("Invalid fitting state; existing document was preserved".into());
            }
        }
        validate_views(&copy.saved_views)?;
        copy.board_guides.validate()?;
        copy.avatar_path = crate::document::portable_avatar(&self.avatar_path, path)?;
        if let Some(background) = copy.board_background.as_mut() {
            background.validate()?;
            background.path = crate::document::portable_image(&background.path, path)?;
        }
        validate_image_assets(&copy.image_assets)?;
        // Images no graphic or fabric uses any more are left out of the saved document.
        let used = |id: &str| {
            copy.template.graphics.iter().any(|g| g.asset_id == id)
                || copy
                    .template
                    .fabric_appearance
                    .values()
                    .any(|a| a.texture.as_ref().is_some_and(|t| t.asset_id == id))
        };
        copy.image_assets.retain(|asset| used(&asset.id));
        for asset in &mut copy.image_assets {
            asset.path = crate::document::portable_image(&asset.path, path)?;
        }
        let bytes = serde_json::to_vec_pretty(&copy).map_err(|e| e.to_string())?;
        crate::document::atomic_write(path, &bytes)
    }
}

fn mesh_signature(template: &GarmentTemplate, g: &CompiledGarment) -> Result<String, String> {
    // Stable FNV-1a fingerprint, not a security/integrity hash.
    let mut hash = 0xcbf29ce484222325_u64;
    let mut append = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    let mut physical_template = template.clone();
    physical_template.mirror_links.clear();
    physical_template.fabric_appearance.clear();
    physical_template.archive.clear();
    physical_template.graphics.clear();
    physical_template.trims.clear();
    physical_template.locked_panels.clear();
    physical_template.internal_lines_locked = false;
    for panel in &mut physical_template.panels {
        panel.layout_2d = None;
        panel.layout_rotation_degrees = 0.;
        if let Some(polygon) = panel.polygon.as_mut() {
            polygon.target_lengths.clear();
            polygon.seam_allowances.clear();
            polygon.notches.clear();
        }
    }
    append(&serde_json::to_vec(&physical_template).map_err(|e| e.to_string())?);
    for p in &g.mesh.uv {
        append(&p.x.to_le_bytes());
        append(&p.y.to_le_bytes());
    }
    for t in &g.mesh.triangles {
        for i in t.vertices {
            append(&(i as u64).to_le_bytes());
        }
    }
    Ok(format!("{hash:016x}"))
}
