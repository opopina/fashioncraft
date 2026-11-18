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
    /// The avatar's pose and the joints its skeleton was built from. Absent: at rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_pose: Option<AvatarPose>,
    /// Props the cloth also collides with (a chair, a table…). Absent in older documents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_objects: Vec<SceneObject>,
    /// The avatar's motion: keyed poses and markers. Absent in older documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_animation: Option<crate::avatar_animation::AvatarAnimation>,
    /// The last recorded take, as a point cache next to the project. Absent when none was
    /// recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation_take: Option<AnimationTake>,
}

/// Where a recorded take's point cache is kept and the frame rate it was recorded at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationTake {
    pub path: PathBuf,
    pub fps: f64,
}

impl AnimationTake {
    pub fn validate(&self) -> Result<(), String> {
        if !self.fps.is_finite() || !crate::avatar_animation::FPS_RANGE.contains(&self.fps) {
            return Err("The take's frame rate must be between 1 and 120".into());
        }
        Ok(())
    }
}

/// A static OBJ prop placed in the scene. Relative paths resolve against the project JSON
/// directory; saving copies the file into the project's `.assets` folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneObject {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// Where the OBJ's origin goes, metres.
    pub translation: [f64; 3],
    /// Turn about the world X, Y and Z axes, degrees.
    #[serde(default)]
    pub rotation_degrees: [f64; 3],
    /// Uniform scale from the OBJ's units to metres.
    pub scale: f64,
    /// Where the object is at given times of the avatar's animation, in time order. Without
    /// keys it stays where `translation` and `rotation_degrees` put it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<ObjectKey>,
}

/// A scene object's place at one time of the animation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectKey {
    pub time_s: f64,
    pub translation: [f64; 3],
    pub rotation_degrees: [f64; 3],
}

/// Most keys one scene object may have.
pub const MAX_OBJECT_KEYS: usize = 1000;

impl SceneObject {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 80 {
            return Err("A scene object needs an id".into());
        }
        if self
            .translation
            .iter()
            .chain(&self.rotation_degrees)
            .any(|v| !v.is_finite())
            || !(1e-4..=1e3).contains(&self.scale)
        {
            return Err(format!(
                "Scene object {} needs a finite position and angles and a scale of 0.0001 to 1000",
                self.name
            ));
        }
        if self.keys.len() > MAX_OBJECT_KEYS
            || self.keys.iter().any(|k| {
                !k.time_s.is_finite()
                    || k.time_s < 0.
                    || k.translation
                        .iter()
                        .chain(&k.rotation_degrees)
                        .any(|v| !v.is_finite())
            })
            || self.keys.windows(2).any(|w| w[1].time_s <= w[0].time_s)
        {
            return Err(format!(
                "Scene object {} needs at most {MAX_OBJECT_KEYS} keys at finite, increasing times",
                self.name
            ));
        }
        Ok(())
    }

    /// Where the object is at animation time `t`: between keys, its position runs straight
    /// and its turn the shortest way; before the first key and after the last it holds them.
    pub fn transform_at(&self, t: f64) -> (glam::DQuat, glam::DVec3) {
        let turn = |r: [f64; 3]| {
            let [x, y, z] = r.map(f64::to_radians);
            glam::DQuat::from_euler(glam::EulerRot::XYZ, x, y, z)
        };
        let at = |k: &ObjectKey| {
            (
                turn(k.rotation_degrees),
                glam::DVec3::from_array(k.translation),
            )
        };
        match self.keys.as_slice() {
            [] => (
                turn(self.rotation_degrees),
                glam::DVec3::from_array(self.translation),
            ),
            [first, ..] if t <= first.time_s => at(first),
            [.., last] if t >= last.time_s => at(last),
            keys => {
                let k = keys.partition_point(|k| k.time_s <= t);
                let (a, b) = (&keys[k - 1], &keys[k]);
                let s = (t - a.time_s) / (b.time_s - a.time_s);
                let ((qa, pa), (qb, pb)) = (at(a), at(b));
                (qa.slerp(qb, s), pa.lerp(pb, s))
            }
        }
    }

    /// The prop's mesh in its own units scaled to metres, before it is turned and placed.
    pub fn local_mesh(&self) -> Result<SurfaceMesh, String> {
        self.validate()?;
        let mut mesh = crate::mesh_import::load(&self.path)?;
        for v in &mut mesh.vertices {
            *v *= self.scale;
        }
        Ok(mesh)
    }

    /// The prop's mesh, placed in the scene.
    pub fn mesh(&self) -> Result<SurfaceMesh, String> {
        let mut mesh = self.local_mesh()?;
        let [x, y, z] = self.rotation_degrees.map(f64::to_radians);
        let rotation = glam::DQuat::from_euler(glam::EulerRot::XYZ, x, y, z);
        let offset = glam::DVec3::from_array(self.translation);
        for v in &mut mesh.vertices {
            *v = offset + rotation * *v;
        }
        mesh.validate()?;
        Ok(mesh)
    }
}

/// The scene objects that move, each with its mesh before placing, to place them quickly at
/// any time of the animation.
#[derive(Clone, Debug)]
pub struct MovingProps {
    parts: Vec<(SceneObject, SurfaceMesh)>,
}

impl MovingProps {
    pub fn new(objects: &[SceneObject]) -> Result<Option<Self>, String> {
        let parts = objects
            .iter()
            .map(|o| o.local_mesh().map(|m| (o.clone(), m)))
            .collect::<Result<Vec<_>, String>>()?;
        Ok((!parts.is_empty()).then_some(Self { parts }))
    }
    /// Whether any object has keys.
    pub fn moves(&self) -> bool {
        self.parts.iter().any(|(o, _)| !o.keys.is_empty())
    }
    /// Every object's vertices at animation time `t`, in order.
    pub fn vertices_at(&self, t: f64) -> Vec<glam::DVec3> {
        let mut out = Vec::new();
        for (object, mesh) in &self.parts {
            let (q, p) = object.transform_at(t);
            out.extend(mesh.vertices.iter().map(|v| p + q * *v));
        }
        out
    }
    /// The objects as one obstacle at animation time `t`.
    pub fn mesh_at(&self, t: f64) -> SurfaceMesh {
        let vertices = self.vertices_at(t);
        let mut triangles = Vec::new();
        let mut base = 0;
        for (_, mesh) in &self.parts {
            triangles.extend(mesh.triangles.iter().map(|tri| tri.map(|i| i + base)));
            base += mesh.vertices.len();
        }
        SurfaceMesh {
            vertices,
            triangles,
        }
    }
}

/// Every scene object's mesh as one obstacle, or `None` when there are none.
pub fn scene_objects_mesh(objects: &[SceneObject]) -> Result<Option<SurfaceMesh>, String> {
    let mut out: Option<SurfaceMesh> = None;
    for object in objects {
        let mesh = object.mesh()?;
        out = Some(match out {
            Some(all) => all.merged(&mesh),
            None => mesh,
        });
    }
    Ok(out)
}

/// A posed avatar: bone rotations and the joint positions (avatar metres) of its skeleton,
/// kept with the document so it does not depend on files next to the avatar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvatarPose {
    pub pose: crate::avatar_rig::Pose,
    pub joints: std::collections::BTreeMap<String, [f64; 3]>,
}

impl AvatarPose {
    /// The avatar's joints from the landmarks shipped next to it, with no rotation yet.
    pub fn from_sidecar(avatar_path: &Path) -> Result<Self, String> {
        let sidecar = crate::garment::arrangement::load_sidecar(avatar_path)?
            .ok_or("This avatar ships no joint landmarks, so it cannot be posed")?;
        let wanted = crate::avatar_rig::joint_ids();
        let joints: std::collections::BTreeMap<String, [f64; 3]> = sidecar
            .landmarks
            .into_iter()
            .filter(|l| wanted.contains(l.id.as_str()))
            .map(|l| (l.id, l.position_m.to_array()))
            .collect();
        if joints.len() != wanted.len() {
            return Err("This avatar lacks some joint landmarks, so it cannot be posed".into());
        }
        Ok(Self {
            pose: Default::default(),
            joints,
        })
    }
    /// The skeleton with joint `id` moved to `at` (avatar metres, at rest). Refused when the
    /// joint is unknown, the place is not finite, or a bone would shrink below 5 mm.
    pub fn with_joint(&self, id: &str, at: glam::DVec3) -> Result<Self, String> {
        if !self.joints.contains_key(id) {
            return Err(format!("The skeleton has no joint {id}"));
        }
        if !at.is_finite() {
            return Err("A joint needs a finite place".into());
        }
        let mut moved = self.clone();
        moved.joints.insert(id.to_string(), at.to_array());
        let empty = SurfaceMesh {
            vertices: vec![],
            triangles: vec![],
        };
        let rig = moved.rig(&empty)?;
        if let Some(bone) = rig.bones.iter().find(|b| b.head.distance(b.tail) < 0.005) {
            return Err(format!("The {} bone would be too short", bone.name));
        }
        Ok(moved)
    }

    /// The avatar's anchors as it stands in this pose.
    pub fn posed_sidecar(
        &self,
        avatar_path: &Path,
    ) -> Result<Option<crate::garment::arrangement::Sidecar>, String> {
        let Some(sidecar) = crate::garment::arrangement::load_sidecar(avatar_path)? else {
            return Ok(None);
        };
        if self.pose.is_empty() {
            return Ok(Some(sidecar));
        }
        let rest = SurfaceMesh::load_obj(avatar_path)?;
        Ok(Some(sidecar.posed(&self.rig(&rest)?, &self.pose)))
    }
    pub fn rig(&self, surface: &SurfaceMesh) -> Result<crate::avatar_rig::Rig, String> {
        self.pose.validate()?;
        let joints = self
            .joints
            .iter()
            .map(|(id, p)| (id.clone(), glam::DVec3::from_array(*p)))
            .collect();
        crate::avatar_rig::Rig::new(&joints, surface)
    }
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
            avatar_pose: None,
            scene_objects: Vec::new(),
            avatar_animation: None,
            animation_take: None,
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
        if let Some(animation) = &project.avatar_animation {
            animation.validate()?;
        }
        if let Some(take) = project.animation_take.as_mut() {
            take.validate()?;
            if take.path.is_relative() {
                take.path = directory.join(&take.path);
            }
        }
        for object in &mut project.scene_objects {
            object.validate()?;
            if object.path.is_relative() {
                object.path = directory.join(&object.path);
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
        let mut surface = SurfaceMesh::load_obj(&self.avatar_path)?;
        if let Some(posed) = self.avatar_pose.as_ref().filter(|p| !p.pose.is_empty()) {
            surface = posed.rig(&surface)?.posed(&surface, &posed.pose);
        }
        if let Some(props) = scene_objects_mesh(&self.scene_objects)? {
            surface = surface.merged(&props);
        }
        let surface = Arc::new(surface);
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
                || copy.template.fabric_appearance.values().any(|a| {
                    a.texture
                        .as_ref()
                        .is_some_and(|t| t.asset_ids().any(|used| used == id))
                })
        };
        copy.image_assets.retain(|asset| used(&asset.id));
        for asset in &mut copy.image_assets {
            asset.path = crate::document::portable_image(&asset.path, path)?;
        }
        for object in &mut copy.scene_objects {
            object.validate()?;
            object.path = crate::document::portable_object(&object.path, path)?;
        }
        if let Some(take) = copy.animation_take.as_mut() {
            take.validate()?;
            take.path = crate::document::portable_cache(&take.path, path)?;
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
    physical_template.pins_attached = false;
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
