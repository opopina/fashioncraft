//! A depth-buffered wgpu viewport. CPU FEM is entirely independent of this module.
use crate::{
    avatar::SurfaceMesh,
    contact::{Collider, Sphere},
    energy,
    garment::seam::CompiledSeam,
    model::{Material, Mesh, State},
    project::Project,
};
use eframe::{egui, egui_wgpu::RenderState, wgpu};
use glam::{Mat4, Vec3};

pub mod appearance;
mod environment;
mod fabric_maps;
mod graphics;
pub use graphics::{GraphicDraw, GraphicImage};

pub struct DrawModel<'a> {
    pub marked_faces: Option<&'a std::collections::BTreeSet<usize>>,
    pub mesh: &'a Mesh,
    pub state: &'a State,
    pub materials: &'a [Material],
    pub optics: Option<&'a [Option<crate::garment::appearance::FabricAppearance>]>,
    pub sphere: Option<&'a Sphere>,
    pub avatar: Option<&'a SurfaceMesh>,
    pub collider: Option<&'a dyn Collider>,
    pub seams: &'a [CompiledSeam],
    /// Trims (buttons, buttonholes, zippers, topstitching) as coloured triangles.
    pub trims: &'a [crate::garment::trims::TrimTriangle],
    /// Prints and logos lying on the cloth, drawn over it with their images.
    pub graphics: &'a [GraphicDraw<'a>],
    /// Images of each material's fabric texture, by material index: its colours, its normal
    /// map and its roughness map (none where it has none).
    pub fabric_images: &'a [[Option<&'a GraphicImage>; 3]],
    pub display: DisplayOptions<'a>,
}

/// Viewport display choices that never change the scene: which cloth triangles are shown,
/// per-triangle colours, the avatar colour and the strain-map range.
#[derive(Clone, Copy, Debug)]
pub struct DisplayOptions<'a> {
    /// Per-triangle visibility (hidden garment, isolated selection); `None` shows them all.
    pub visible_faces: Option<&'a [bool]>,
    /// Per-triangle base colour replacing the fabric colour (random piece colours, state
    /// tints); fit maps and issue marks still take precedence.
    pub face_colors: Option<&'a [Option<[f32; 3]>]>,
    /// Per-triangle flipped front face (the piece's normal is flipped): its front and back
    /// colours swap sides.
    pub flipped_faces: Option<&'a [bool]>,
    /// Avatar body colour, linear RGB.
    pub avatar_color: [f32; 3],
    /// Strain that reaches the strain map's last colour, as a fraction (0.2 = 20 %).
    pub strain_max: f64,
    /// Blend smoothly between the eight colours instead of showing discrete bands.
    pub strain_gradient: bool,
    /// What the strain map measures.
    pub strain_mode: StrainMode,
    /// Technical-flat rendering: plain garment colour and depth-tested outline and silhouette
    /// lines.
    pub schematic: Option<Schematic>,
    /// Contact map reach beyond the fabric's half thickness, in metres: cloth resting on a
    /// contact shell (a few millimetres off the skin) still counts as touching.
    pub contact_tolerance_m: f64,
}

/// What the strain map measures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrainMode {
    /// Deformation |stretch − 1| against one range for every fabric.
    Global,
    /// Estimated fibre tension (the fabric's tension modulus × elongation, in its most loaded
    /// direction) as a fraction of `reference_n_per_m`: 1 means that fabric reached the limit.
    /// A stiff fabric reaches it with little stretch, a supple one needs much more.
    Fabric { reference_n_per_m: f64 },
}

/// Technical-flat ("schematic") rendering options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Schematic {
    /// Plain garment colour, linear RGB.
    pub garment_color: [f32; 3],
    /// Line colour, linear RGB.
    pub line_color: [f32; 3],
    /// Line width on screen, in pixels.
    pub line_px: f32,
    /// Lines where the surface turns away from the viewer (outline against the background and
    /// folds).
    pub silhouette: bool,
    /// Free cloth edges: hems, necklines and both sides of every seam.
    pub outlines: bool,
}

/// Fit-map value of one triangle and the value at which the map saturates. `stretch` is the
/// warp and weft stretch from [`energy::stretches`].
pub fn fit_value(
    stretch: [f64; 2],
    material: &Material,
    mode: StrainMode,
    global_max: f64,
) -> (f64, f64) {
    match mode {
        StrainMode::Global => (
            stretch.iter().map(|s| (s - 1.).abs()).fold(0., f64::max),
            global_max,
        ),
        StrainMode::Fabric { reference_n_per_m } => {
            let [warp, weft] = fibre_tension(stretch, material);
            (warp.max(weft) / reference_n_per_m.max(1e-9), 1.)
        }
    }
}

/// Estimated tension in N/m along warp and weft: the fabric's tension modulus times the
/// elongation (zero when the direction is not stretched).
pub fn fibre_tension(stretch: [f64; 2], material: &Material) -> [f64; 2] {
    [
        material.warp * (stretch[0] - 1.).max(0.),
        material.weft * (stretch[1] - 1.).max(0.),
    ]
}

/// Default avatar colour, a light skin tone, linear RGB.
pub const AVATAR_COLOR: [f32; 3] = [0.95, 0.68, 0.55];
/// The neutral grey the avatar can be shown in instead, linear RGB.
pub const AVATAR_GREY: [f32; 3] = [0.46, 0.47, 0.49];
/// Default strain-map maximum: 20 % (stretched to 120 % of the rest length).
pub const STRAIN_MAX: f64 = 0.2;

impl Default for DisplayOptions<'_> {
    fn default() -> Self {
        Self {
            visible_faces: None,
            face_colors: None,
            flipped_faces: None,
            avatar_color: AVATAR_COLOR,
            strain_max: STRAIN_MAX,
            strain_gradient: true,
            strain_mode: StrainMode::Global,
            schematic: None,
            contact_tolerance_m: 0.,
        }
    }
}

/// Fit-map colour of a strain value: one of the eight bands, or a blend between neighbouring
/// colours along the 0..`max` range. Past `max` stays the last colour.
pub fn strain_color(strain: f64, max: f64, gradient: bool) -> [f32; 3] {
    if !gradient {
        return STRAIN_BANDS[strain_band(strain, max)];
    }
    if !strain.is_finite() || !max.is_finite() || max <= 0. {
        return STRAIN_BANDS[0];
    }
    let last = STRAIN_BANDS.len() - 1;
    let t = (strain / max).clamp(0., 1.) * last as f64;
    let i = (t.floor() as usize).min(last - 1);
    let f = (t - i as f64) as f32;
    let [a, b] = [STRAIN_BANDS[i], STRAIN_BANDS[i + 1]];
    [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * f)
}

/// Line segments of the technical-flat view, in world space: free cloth edges (an edge used by
/// one visible triangle) and silhouette edges (shared by a triangle facing the viewer and one
/// facing away).
pub fn schematic_lines(
    mesh: &Mesh,
    state: &State,
    visible: Option<&[bool]>,
    camera: &Camera,
    schematic: Schematic,
) -> Vec<[Vec3; 2]> {
    let eye = camera.eye();
    let facing = |t: &crate::model::Triangle| {
        let p = t.vertices.map(|i| state.x[i].as_vec3());
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        let toward = if camera.orthographic {
            camera.direction()
        } else {
            eye - (p[0] + p[1] + p[2]) / 3.
        };
        n.dot(toward) >= 0.
    };
    let mut edges: std::collections::HashMap<(usize, usize), (usize, bool, bool)> =
        std::collections::HashMap::new();
    for (face, t) in mesh.triangles.iter().enumerate() {
        if visible.is_some_and(|shown| !shown.get(face).copied().unwrap_or(true)) {
            continue;
        }
        let front = facing(t);
        for k in 0..3 {
            let (a, b) = (t.vertices[k], t.vertices[(k + 1) % 3]);
            let key = (a.min(b), a.max(b));
            let entry = edges.entry(key).or_insert((0, false, false));
            entry.0 += 1;
            if front {
                entry.1 = true;
            } else {
                entry.2 = true;
            }
        }
    }
    edges
        .into_iter()
        .filter(|(_, (count, front, back))| {
            (schematic.outlines && *count == 1)
                || (schematic.silhouette && *count >= 2 && *front && *back)
        })
        .map(|((a, b), _)| [state.x[a].as_vec3(), state.x[b].as_vec3()])
        .collect()
}

/// The strain map's eight colours, from no strain (blue) to the maximum and beyond (red), in
/// linear RGB. As bands they make a value readable against the legend.
pub const STRAIN_BANDS: [[f32; 3]; 8] = [
    [0.05, 0.17, 0.62],
    [0.03, 0.33, 0.75],
    [0.02, 0.52, 0.55],
    [0.10, 0.60, 0.18],
    [0.45, 0.68, 0.06],
    [0.90, 0.68, 0.04],
    [0.92, 0.33, 0.03],
    [0.80, 0.05, 0.04],
];

/// Band of a strain value (|stretch − 1|, a fraction) for a map saturating at `max`: eight
/// equal steps from 0 to `max`; anything at or past `max` is the last band.
pub fn strain_band(strain: f64, max: f64) -> usize {
    if !strain.is_finite() || !max.is_finite() || max <= 0. {
        return 0;
    }
    ((strain / max * STRAIN_BANDS.len() as f64).floor().max(0.) as usize)
        .min(STRAIN_BANDS.len() - 1)
}

/// Position (4), normal (3), colour (3) and wire flag (1), barycentric (3), roughness (1), back
/// colour (3), material coordinates along warp and weft in metres (2), weave kind, thread
/// spacing and contrast (3), fabric texture coordinates, layer (-1 for none) and relief in
/// metres (4), normal-map and roughness-map layers (-1 for none) (2).
const VERTEX_FLOATS: usize = 29;

/// What a cloth triangle shows besides its colour: the back colour, the material coordinates
/// of its corners (warp, weft) and the weave parameters.
#[derive(Clone, Copy)]
struct Surface {
    back: [f32; 3],
    uv: [[f32; 2]; 3],
    weave: [f32; 3],
    /// Fabric texture coordinates of the corners (one repeat spans [0,1]).
    map_uv: [[f32; 2]; 3],
    /// Texture layer (-1 for none), relief height in metres, and the normal-map and
    /// roughness-map layers (-1 for none).
    map: [f32; 4],
}

/// A face of a fabric that looks thick, kept to draw the rim along its free edges.
#[derive(Clone, Copy)]
struct ThickFace {
    face: usize,
    /// Offset of the front face from the simulated surface along the smooth normal, m.
    towards_front: f32,
    color: [f32; 3],
    roughness: f32,
    look: Option<Surface>,
}

/// The vertex floats of a surface without a fabric texture.
const NO_MAP: [f32; 6] = [0., 0., -1., 0., -1., -1.];

fn cloth_surface(
    appearance: Option<&crate::garment::appearance::FabricAppearance>,
    marked: bool,
    contact: bool,
    fit_map: Option<[f32; 3]>,
    base: Option<[f32; 3]>,
) -> ([f32; 3], f32) {
    if marked {
        return ([1.0, 0.23, 0.10], -1.);
    }
    if contact {
        return ([1.0, 0.2, 0.25], -1.);
    }
    if let Some(color) = fit_map {
        return (color, -1.);
    }
    if let Some(color) = base {
        return (color, -1.);
    }
    appearance.map_or(([0.91, 0.92, 0.94], -1.), |a| {
        (a.linear_color(), a.roughness as f32)
    })
}

/// The front colour and the rest of a cloth face's look: its back colour (the two swap on a
/// flipped piece), its corners' material coordinates turned into warp and weft, the fabric's
/// weave, and its texture when it has one on `layer`.
fn face_surface(
    appearance: &crate::garment::appearance::FabricAppearance,
    grain_degrees: f64,
    uv: [glam::DVec2; 3],
    flipped: bool,
    layers: [Option<u32>; 3],
) -> ([f32; 3], Surface) {
    let layer = layers[0];
    let (front, back) = (appearance.linear_color(), appearance.linear_back_color());
    let (sin, cos) = grain_degrees.to_radians().sin_cos();
    let warp_weft = uv.map(|p| glam::DVec2::new(p.x * cos + p.y * sin, -p.x * sin + p.y * cos));
    let texture = appearance.texture.as_ref().zip(layer);
    (
        if flipped { back } else { front },
        Surface {
            back: if flipped { front } else { back },
            uv: warp_weft.map(|p| p.as_vec2().to_array()),
            weave: appearance.weave_parameters(),
            map_uv: texture.map_or([[0.; 2]; 3], |(t, _)| {
                warp_weft.map(|p| t.image_uv(p).as_vec2().to_array())
            }),
            map: texture.map_or([-1., 0., -1., -1.], |(t, layer)| {
                let detail = |k: usize| layers[k].map_or(-1., |l| l as f32);
                [
                    layer as f32,
                    (t.relief_mm / 1000.) as f32,
                    detail(1),
                    detail(2),
                ]
            }),
        },
    )
}

struct Timers {
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
}

/// Default vertical field of view of the perspective projection, in degrees. The
/// orthographic framing uses the same value so switching projection keeps the size.
pub const DEFAULT_FOV_DEGREES: f32 = 45.;
/// Vertical field of view a camera accepts, in degrees.
pub const FOV_RANGE_DEGREES: std::ops::RangeInclusive<f32> = 10.0..=120.0;
const NEAR: f32 = 0.01;
const FAR: f32 = 100.;
const MIN_DISTANCE: f32 = 0.15;
const MAX_DISTANCE: f32 = 30.;
/// Orbit sensitivity: radians of rotation per pixel of pointer drag.
const ORBIT_RADIANS_PER_PIXEL: f32 = 0.008;
/// Orbit stops exactly at the poles. [`Camera::up`] stays defined there, so the
/// Top and Bottom presets are exact instead of a near-pole approximation.
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2;

/// Canonical viewing directions offered by the 3D view bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetView {
    Isometric,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    /// Halfway between Front and Left.
    ThreeQuarterLeft,
    /// Halfway between Front and Right.
    ThreeQuarterRight,
}
impl PresetView {
    /// Every preset, in the order the view bar shows them.
    pub const ALL: [Self; 9] = [
        Self::Isometric,
        Self::Front,
        Self::ThreeQuarterLeft,
        Self::ThreeQuarterRight,
        Self::Back,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
    ];
    /// Short control label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Isometric => "Iso",
            Self::Front => "Front",
            Self::Back => "Back",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::ThreeQuarterLeft => "3/4 left",
            Self::ThreeQuarterRight => "3/4 right",
        }
    }
    /// Unit direction from the target towards the eye.
    pub fn direction(self) -> Vec3 {
        match self {
            Self::Isometric => Vec3::ONE.normalize(),
            Self::Front => Vec3::Z,
            Self::Back => Vec3::NEG_Z,
            Self::Left => Vec3::NEG_X,
            Self::Right => Vec3::X,
            Self::Top => Vec3::Y,
            Self::Bottom => Vec3::NEG_Y,
            Self::ThreeQuarterLeft => Vec3::new(-1., 0., 1.).normalize(),
            Self::ThreeQuarterRight => Vec3::new(1., 0., 1.).normalize(),
        }
    }
    /// Yaw and pitch that produce [`Self::direction`], exact at the poles.
    pub fn angles(self) -> (f32, f32) {
        let d = self.direction();
        (d.x.atan2(d.z), d.y.clamp(-1., 1.).asin())
    }
}
/// A camera pose the view bar can come back to.
#[derive(Clone, Copy)]
pub struct CameraPose {
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vec3,
    orthographic: bool,
    fov_degrees: f32,
}
/// Orbit camera for the 3D viewport.
///
/// Every field is public on purpose. The authoring adapters write the orbit
/// fields directly, and the view-history fields must stay reachable from the
/// binary targets: `cloth-fit` includes `atelier.rs` in the *bin* crate but links
/// `render` from this *library* crate, so `pub(crate)` would not cross that
/// boundary. Treat the history fields as internal state and use
/// [`Camera::save_pose`], [`Camera::restore_pose`] and [`Camera::request_frame`]
/// instead of writing them, and prefer [`Camera::new`] over a bare literal.
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    /// Orthographic projection matched to the perspective framing at the target.
    pub orthographic: bool,
    /// Vertical field of view in degrees; sizes the orthographic view as well.
    /// Values outside [`FOV_RANGE_DEGREES`] are clamped when used.
    pub fov_degrees: f32,
    /// Pose remembered by the view bar's Restore button.
    pub saved: Option<CameraPose>,
    /// Bounding sphere the next viewport interaction pass should frame. The
    /// request waits because the viewport rectangle is only known once the
    /// canvas has been allocated.
    pub pending_frame: Option<(Vec3, f32)>,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.55,
            pitch: 0.4,
            distance: 2.5,
            target: Vec3::new(0.0, 0.4, 0.0),
            orthographic: false,
            fov_degrees: DEFAULT_FOV_DEGREES,
            saved: None,
            pending_frame: None,
        }
    }
}
impl Camera {
    /// Build a camera from its orbit fields; the view history starts empty.
    /// Equivalent to `Camera { yaw, pitch, distance, target, ..Default::default() }`.
    pub fn new(yaw: f32, pitch: f32, distance: f32, target: Vec3) -> Self {
        Self {
            yaw,
            pitch,
            distance,
            target,
            ..Default::default()
        }
    }
    /// Half-height of the view volume per unit of camera distance.
    fn half_tan(&self) -> f32 {
        (self.fov_radians() * 0.5).tan()
    }
    /// The field of view actually used, clamped to [`FOV_RANGE_DEGREES`], in radians.
    pub fn fov_radians(&self) -> f32 {
        let degrees = if self.fov_degrees.is_finite() {
            self.fov_degrees
        } else {
            DEFAULT_FOV_DEGREES
        };
        degrees
            .clamp(*FOV_RANGE_DEGREES.start(), *FOV_RANGE_DEGREES.end())
            .to_radians()
    }
    fn pose(&self) -> CameraPose {
        CameraPose {
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            target: self.target,
            orthographic: self.orthographic,
            fov_degrees: self.fov_degrees,
        }
    }
    fn set_pose(&mut self, pose: CameraPose) {
        self.yaw = pose.yaw;
        self.pitch = pose.pitch;
        self.distance = pose.distance;
        self.target = pose.target;
        self.orthographic = pose.orthographic;
        self.fov_degrees = pose.fov_degrees;
    }
    /// Unit vector from the target towards the eye.
    pub fn direction(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(sy * cp, sp, cy * cp)
    }
    pub fn eye(&self) -> Vec3 {
        self.target + self.distance * self.direction()
    }
    /// View direction, from the eye towards the target.
    pub fn forward(&self) -> Vec3 {
        -self.direction()
    }
    /// Camera up: world Y made perpendicular to the view direction. The value is
    /// continuous across the poles, so Top and Bottom never degenerate.
    pub fn up(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sp * sy, cp, -sp * cy)
    }
    /// Screen-right vector, matching the rendered image.
    pub fn right(&self) -> Vec3 {
        self.forward().cross(self.up()).normalize_or_zero()
    }
    pub fn matrix(&self, aspect: f32) -> Mat4 {
        // WebGPU uses a [0,1] depth range and Y-up NDC on every backend.
        let view = glam::camera::rh::view::look_at_mat4(self.eye(), self.target, self.up());
        let projection = if self.orthographic {
            // Match the perspective framing at the target plane so switching
            // projection keeps the same apparent size.
            let half = self.distance * self.half_tan();
            glam::camera::rh::proj::directx::orthographic(
                -half * aspect,
                half * aspect,
                -half,
                half,
                NEAR,
                FAR,
            )
        } else {
            glam::camera::rh::proj::directx::perspective(self.fov_radians(), aspect, NEAR, FAR)
        };
        projection * view
    }
    /// Project a world point into window coordinates; `None` behind the camera.
    pub fn project(&self, world: Vec3, rect: egui::Rect) -> Option<egui::Pos2> {
        let width = rect.width().max(1.);
        let height = rect.height().max(1.);
        let clip = self.matrix(width / height) * world.extend(1.);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.left() + (ndc.x * 0.5 + 0.5) * width,
            rect.top() + (0.5 - ndc.y * 0.5) * height,
        ))
    }
    /// Orbit by a pointer drag in pixels.
    pub fn orbit_by(&mut self, delta: egui::Vec2) {
        self.yaw -= delta.x * ORBIT_RADIANS_PER_PIXEL;
        self.pitch = (self.pitch + delta.y * ORBIT_RADIANS_PER_PIXEL).clamp(-MAX_PITCH, MAX_PITCH);
    }
    /// Pan so the scene follows the pointer: the world point under the pointer
    /// moves with it, at the target plane's exact scale.
    pub fn pan_by(&mut self, delta: egui::Vec2, rect: egui::Rect) {
        let world_per_pixel = 2. * self.distance * self.half_tan() / rect.height().max(1.);
        self.target += (self.right() * -delta.x + self.up() * delta.y) * world_per_pixel;
    }
    /// Multiply the camera distance, clamped to the working range.
    pub fn zoom(&mut self, factor: f32) {
        // `!(factor > 0.)` rejected NaN as well as non-positive factors; spell
        // that out rather than negating a partial-order comparison.
        if factor <= 0. || factor.is_nan() {
            return;
        }
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
    /// Zoom while keeping the world point under the cursor exactly in place.
    pub fn zoom_at(&mut self, cursor: egui::Pos2, rect: egui::Rect, factor: f32) {
        if factor <= 0. || factor.is_nan() {
            return;
        }
        let anchor = self.focal_point(cursor, rect);
        let distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
        let scale = distance / self.distance;
        self.target = anchor + (self.target - anchor) * scale;
        self.distance = distance;
    }
    /// Intersection of the cursor ray with the focal plane through the target.
    fn focal_point(&self, cursor: egui::Pos2, rect: egui::Rect) -> Vec3 {
        let (origin, direction) = self.ray(cursor, rect);
        // `ray` is f64 for picking precision; the camera itself is f32.
        let (origin, direction) = (origin.as_vec3(), direction.as_vec3());
        let forward = self.forward();
        let denominator = direction.dot(forward);
        if denominator.abs() < 1e-6 {
            return self.target;
        }
        origin + direction * ((self.target - origin).dot(forward) / denominator)
    }
    /// Wheel zoom towards the cursor when it is over the viewport.
    fn wheel_zoom(&mut self, response: &egui::Response) {
        if !response.hovered() {
            return;
        }
        let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll == 0. {
            return;
        }
        let factor = (-scroll * 0.002).exp();
        match response.hover_pos() {
            Some(cursor) => self.zoom_at(cursor, response.rect, factor),
            None => self.zoom(factor),
        }
    }
    /// Ask the next interaction pass to frame this bounding sphere.
    pub fn request_frame(&mut self, center: Vec3, radius: f32) {
        self.pending_frame = Some((center, radius));
    }
    /// Apply a pending frame request with the real viewport rectangle.
    pub fn apply_pending_frame(&mut self, rect: egui::Rect) -> bool {
        match self.pending_frame.take() {
            Some((center, radius)) => {
                self.frame_in(center, radius, rect);
                true
            }
            None => false,
        }
    }
    /// Frame a bounding sphere, keeping the current orbit direction and fitting
    /// both viewport axes.
    pub fn frame_in(&mut self, center: Vec3, radius: f32, rect: egui::Rect) {
        self.save_pose();
        self.target = center;
        let aspect = (rect.width() / rect.height().max(1.)).max(1e-3);
        let half_v = self.fov_radians() * 0.5;
        let half_h = (half_v.tan() * aspect).atan();
        let half = half_v.min(half_h);
        self.distance = (radius.max(1e-3) * 1.2 / half.sin()).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
    /// Frame a bounding sphere without a viewport; assumes a square viewport.
    pub fn frame(&mut self, center: Vec3, radius: f32) {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(1.));
        self.frame_in(center, radius, rect);
    }
    /// Point the camera along a canonical direction, keeping target and distance.
    pub fn apply_preset(&mut self, view: PresetView) {
        self.save_pose();
        let (yaw, pitch) = view.angles();
        self.yaw = yaw;
        self.pitch = pitch;
    }
    /// Toggle between the matched perspective and orthographic projections.
    pub fn toggle_projection(&mut self) {
        self.orthographic = !self.orthographic;
    }
    /// Return to the default framing while keeping the projection mode.
    pub fn reset_view(&mut self) {
        let default = Self::default();
        self.yaw = default.yaw;
        self.pitch = default.pitch;
        self.distance = default.distance;
        self.target = default.target;
    }
    /// Remember the current pose for [`Self::restore_pose`].
    pub fn save_pose(&mut self) {
        self.saved = Some(self.pose());
    }
    pub fn has_saved_pose(&self) -> bool {
        self.saved.is_some()
    }
    /// Swap the current pose with the remembered one, so Restore toggles.
    pub fn restore_pose(&mut self) -> bool {
        match self.saved.take() {
            Some(saved) => {
                self.saved = Some(self.pose());
                self.set_pose(saved);
                true
            }
            None => false,
        }
    }
    pub fn interact(&mut self, response: &egui::Response) {
        self.apply_pending_frame(response.rect);
        let delta = response.ctx.input(|i| i.pointer.delta());
        if response.dragged_by(egui::PointerButton::Primary) {
            self.orbit_by(delta);
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.pan_by(delta, response.rect);
        }
        self.wheel_zoom(response);
        if response.double_clicked() {
            self.reset_view();
        }
    }
    pub fn garment_interact(&mut self, response: &egui::Response) {
        self.apply_pending_frame(response.rect);
        let delta = response.ctx.input(|i| i.pointer.delta());
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.orbit_by(delta);
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.pan_by(delta, response.rect);
        }
        self.wheel_zoom(response);
    }
    pub fn ray(&self, position: egui::Pos2, rect: egui::Rect) -> (glam::DVec3, glam::DVec3) {
        let inv = self.matrix(rect.width() / rect.height()).inverse();
        let u = 2. * (position.x - rect.left()) / rect.width() - 1.;
        let v = 1. - 2. * (position.y - rect.top()) / rect.height();
        let a = inv * glam::Vec4::new(u, v, 0., 1.);
        let b = inv * glam::Vec4::new(u, v, 1., 1.);
        let origin = (a.truncate() / a.w).as_dvec3();
        let far = (b.truncate() / b.w).as_dvec3();
        (origin, (far - origin).normalize())
    }
}

pub struct Viewport {
    pub appearance: appearance::Appearance,
    supports_msaa4: bool,
    samples: u32,
    msaa_color: Option<wgpu::TextureView>,
    environment: environment::Environment,
    render_state: RenderState,
    pipeline: wgpu::RenderPipeline,
    color: wgpu::TextureView,
    color_texture: wgpu::Texture,
    depth: wgpu::TextureView,
    size: [u32; 2],
    texture_id: egui::TextureId,
    vertices: wgpu::Buffer,
    capacity: u64,
    timers: Option<Timers>,
    view_uniform: wgpu::Buffer,
    view_bind: wgpu::BindGroup,
    avatar_buffer: Option<wgpu::Buffer>,
    avatar_key: (usize, usize, [u32; 3]),
    avatar_count: u32,
    graphics: graphics::GraphicLayer,
    fabric_maps: fabric_maps::FabricMaps,
}
impl Viewport {
    fn target(device: &wgpu::Device, size: [u32; 2], depth: bool, samples: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(if depth { "cloth-depth" } else { "cloth-color" }),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: if depth {
                wgpu::TextureFormat::Depth32Float
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: if depth || samples > 1 {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
            },
            view_formats: &[],
        })
    }
    fn mesh_pipeline(device: &wgpu::Device, samples: u32) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cloth-lit-wireframe"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cloth.wgsl").into()),
        });
        let attrs = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x3,2=>Float32x4,3=>Float32x3,4=>Float32,5=>Float32x3,6=>Float32x2,7=>Float32x3,8=>Float32x4,9=>Float32x2];
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cloth-viewport"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (VERTEX_FLOATS * std::mem::size_of::<f32>()) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
    }
    pub fn supports_msaa4(&self) -> bool {
        self.supports_msaa4
    }
    pub fn new(render_state: RenderState) -> Self {
        let device = &render_state.device;
        let supports_msaa4 = [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Depth32Float,
        ]
        .into_iter()
        .all(|format| {
            render_state
                .adapter
                .get_texture_format_features(format)
                .flags
                .sample_count_supported(4)
        });
        let appearance = appearance::Appearance::default();
        let samples = appearance.samples(supports_msaa4);
        let pipeline = Self::mesh_pipeline(device, samples);
        let environment = environment::Environment::new(device, samples);
        let msaa_color = (samples > 1)
            .then(|| Self::target(device, [1, 1], false, samples).create_view(&Default::default()));
        let color_texture = Self::target(device, [1, 1], false, 1);
        let view_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera matrix"),
            size: 112,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_uniform.as_entire_binding(),
            }],
        });
        let color = color_texture.create_view(&Default::default());
        let depth = Self::target(device, [1, 1], true, samples).create_view(&Default::default());
        let texture_id = render_state.renderer.write().register_native_texture(
            device,
            &color,
            wgpu::FilterMode::Linear,
        );
        let graphics = graphics::GraphicLayer::new(device, samples, &view_uniform);
        let fabric_maps = fabric_maps::FabricMaps::new(
            device,
            &render_state.queue,
            &pipeline.get_bind_group_layout(1),
        );
        let capacity = 1024;
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport-vertices"),
            size: capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            appearance,
            supports_msaa4,
            samples,
            msaa_color,
            environment,
            render_state,
            pipeline,
            color,
            color_texture,
            depth,
            size: [1, 1],
            texture_id,
            vertices,
            capacity,
            timers: None,
            view_uniform,
            view_bind,
            avatar_buffer: None,
            avatar_key: (0, 0, [0; 3]),
            avatar_count: 0,
            graphics,
            fabric_maps,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        size: [u32; 2],
        camera: &Camera,
        mesh: &Mesh,
        state: &State,
        p: &Project,
        wire: bool,
        heat: bool,
        contacts: bool,
    ) -> egui::TextureId {
        self.draw_scene(
            size,
            camera,
            DrawModel {
                marked_faces: None,
                mesh,
                state,
                materials: std::slice::from_ref(&p.material),
                optics: None,
                sphere: p.sphere.as_ref(),
                avatar: None,
                collider: p.sphere.as_ref().map(|s| s as &dyn Collider),
                seams: &[],
                trims: &[],
                graphics: &[],
                fabric_images: &[],
                display: DisplayOptions::default(),
            },
            wire,
            heat,
            contacts,
        )
    }
    pub fn draw_scene(
        &mut self,
        size: [u32; 2],
        camera: &Camera,
        model: DrawModel<'_>,
        wire: bool,
        heat: bool,
        contacts: bool,
    ) -> egui::TextureId {
        let DrawModel {
            marked_faces,
            mesh,
            state,
            materials,
            optics,
            sphere,
            avatar,
            collider,
            seams,
            trims,
            graphics,
            fabric_images,
            display,
        } = model;
        let device = &self.render_state.device;
        let max = device.limits().max_texture_dimension_2d.min(4096);
        let size = size.map(|s| s.clamp(1, max));
        let samples = self.appearance.samples(self.supports_msaa4);
        if samples != self.samples {
            self.pipeline = Self::mesh_pipeline(device, samples);
            // Auto pipeline layouts are exclusive: rebuild the matching bind group.
            self.view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("camera"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.view_uniform.as_entire_binding(),
                }],
            });
            self.environment = environment::Environment::new(device, samples);
            self.graphics = graphics::GraphicLayer::new(device, samples, &self.view_uniform);
            self.fabric_maps
                .rebind(device, &self.pipeline.get_bind_group_layout(1));
        }
        if size != self.size || samples != self.samples {
            self.color_texture = Self::target(device, size, false, 1);
            self.color = self.color_texture.create_view(&Default::default());
            self.depth = Self::target(device, size, true, samples).create_view(&Default::default());
            self.msaa_color = (samples > 1).then(|| {
                Self::target(device, size, false, samples).create_view(&Default::default())
            });
            self.size = size;
            self.samples = samples;
            self.render_state
                .renderer
                .write()
                .update_egui_texture_from_wgpu_texture(
                    device,
                    &self.color,
                    wgpu::FilterMode::Linear,
                    self.texture_id,
                );
        }
        let matrix = camera.matrix(size[0] as f32 / size[1] as f32);
        self.render_state.queue.write_buffer(
            &self.view_uniform,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        let eye = if camera.orthographic {
            camera.direction().extend(0.)
        } else {
            camera.eye().extend(1.)
        };
        self.render_state.queue.write_buffer(
            &self.view_uniform,
            64,
            bytemuck::cast_slice(&eye.to_array()),
        );
        self.render_state.queue.write_buffer(
            &self.view_uniform,
            80,
            bytemuck::cast_slice(&self.appearance.lighting.uniform()),
        );
        self.environment.update(
            &self.render_state.queue,
            matrix,
            &self.appearance,
            avatar,
            state,
        );
        let layers = self.fabric_maps.update(
            device,
            &self.render_state.queue,
            &self.pipeline.get_bind_group_layout(1),
            fabric_images,
        );
        let mut data = Vec::<f32>::new();
        let mut smooth = vec![Vec3::ZERO; state.x.len()];
        for t in &mesh.triangles {
            let p = t.vertices.map(|i| state.x[i].as_vec3());
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            for i in t.vertices {
                smooth[i] += n;
            }
        }
        for n in &mut smooth {
            *n = n.normalize_or_zero();
        }
        let mut triangle = |points: [Vec3; 3],
                            color: [f32; 3],
                            wire: bool,
                            normals: Option<[Vec3; 3]>,
                            roughness: f32,
                            surface: Option<Surface>| {
            let normal = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize_or_zero();
            if normal.length_squared() < 0.5 {
                return;
            }
            for (i, p) in points.into_iter().enumerate() {
                data.extend_from_slice(&p.extend(1.0).to_array());
                data.extend_from_slice(&normals.map_or(normal, |n| n[i]).to_array());
                data.extend_from_slice(&color);
                data.push(if wire { 1.0 } else { 0.0 });
                data.extend_from_slice(&[
                    if i == 0 { 1.0 } else { 0.0 },
                    if i == 1 { 1.0 } else { 0.0 },
                    if i == 2 { 1.0 } else { 0.0 },
                ]);
                data.push(roughness);
                let surface = surface.unwrap_or(Surface {
                    back: color,
                    uv: [[0.; 2]; 3],
                    weave: [0.; 3],
                    map_uv: [[0.; 2]; 3],
                    map: [-1., 0., -1., -1.],
                });
                data.extend_from_slice(&surface.back);
                data.extend_from_slice(&surface.uv[i]);
                data.extend_from_slice(&surface.weave);
                data.extend_from_slice(&surface.map_uv[i]);
                data.extend_from_slice(&surface.map);
            }
        };
        let mut rims: Vec<ThickFace> = vec![];
        for (face, t) in mesh.triangles.iter().enumerate() {
            if display
                .visible_faces
                .is_some_and(|shown| !shown.get(face).copied().unwrap_or(true))
            {
                continue;
            }
            let material = &materials[t.material];
            let (strain, strain_max) = if heat {
                fit_value(
                    energy::stretches(&state.x, t, material),
                    material,
                    display.strain_mode,
                    display.strain_max,
                )
            } else {
                (0., 1.)
            };
            let contact = contacts
                && collider.is_some_and(|s| {
                    t.vertices.iter().any(|i| {
                        s.penetration(
                            state.x[*i],
                            material.thickness + 2. * display.contact_tolerance_m.max(0.),
                        ) > 0.0
                    })
                });
            let appearance = optics
                .and_then(|values| values.get(t.material))
                .and_then(Option::as_ref);
            let (color, roughness) = cloth_surface(
                appearance,
                marked_faces.is_some_and(|faces| faces.contains(&face)),
                contact,
                heat.then(|| strain_color(strain, strain_max, display.strain_gradient)),
                // The technical view is one flat colour; otherwise piece colours and tints apply.
                display.schematic.map(|s| s.garment_color).or(display
                    .face_colors
                    .and_then(|colors| colors.get(face).copied().flatten())),
            );
            // A fabric's own look (not a diagnostic colour): its back face, flipped pieces
            // swapping the two, and its weave along the grain.
            let flipped = display
                .flipped_faces
                .is_some_and(|f| f.get(face).copied().unwrap_or(false));
            let surface = (roughness >= 0.).then(|| {
                face_surface(
                    appearance.expect("custom optics have an appearance"),
                    material.grain_degrees,
                    t.vertices.map(|i| mesh.uv[i]),
                    flipped,
                    layers.get(t.material).copied().unwrap_or([None; 3]),
                )
            });
            let points = t.vertices.map(|i| state.x[i].as_vec3());
            let normals = t.vertices.map(|i| smooth[i]);
            let front_color = surface.map_or(color, |(front, _)| front);
            // A fabric that looks thick: its faces sit half the thickness either side of the
            // simulated surface, the front one towards the front.
            let half = surface
                .and(appearance)
                .map_or(0., |a| (a.render_thickness_mm / 2000.) as f32);
            if half > 0. {
                let towards_front = if flipped { half } else { -half };
                for side in [1., -1.] {
                    triangle(
                        [0, 1, 2].map(|k| points[k] + normals[k] * (towards_front * side)),
                        front_color,
                        wire,
                        Some(normals),
                        roughness,
                        surface.map(|(_, s)| s),
                    );
                }
                rims.push(ThickFace {
                    face,
                    towards_front,
                    color: front_color,
                    roughness,
                    look: surface.map(|(_, s)| s),
                });
            } else {
                triangle(
                    points,
                    front_color,
                    wire,
                    Some(normals),
                    roughness,
                    surface.map(|(_, s)| s),
                );
            }
        }
        // The rim of thick fabrics: a band joining their two faces along each free edge.
        let mut edges = std::collections::HashMap::<(usize, usize), (u32, usize)>::new();
        for (k, thick) in rims.iter().enumerate() {
            let v = mesh.triangles[thick.face].vertices;
            for e in 0..3 {
                let (a, b) = (v[e], v[(e + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_insert((0, k)).0 += 1;
            }
        }
        for (&(a, b), &(count, k)) in &edges {
            if count != 1 {
                continue;
            }
            let ThickFace {
                face,
                towards_front,
                color: rim_color,
                roughness,
                look,
            } = rims[k];
            let v = mesh.triangles[face].vertices;
            let corner = |i: usize| v.iter().position(|&c| c == i).unwrap_or(0);
            let (ca, cb) = (corner(a), corner(b));
            let third = v.into_iter().find(|&i| i != a && i != b).unwrap_or(a);
            let (pa, pb) = (state.x[a].as_vec3(), state.x[b].as_vec3());
            let along = pb - pa;
            let mut outward = along.cross(smooth[a] + smooth[b]).normalize_or_zero();
            if outward.dot(state.x[third].as_vec3() - pa) > 0. {
                outward = -outward;
            }
            let shift =
                |i: usize, side: f32| state.x[i].as_vec3() + smooth[i] * (towards_front * side);
            // The rim shows the fabric's front colour, weave and texture, stretched across
            // its width: each corner takes the material coordinates of its outline point.
            let rim = |corners: [usize; 3]| {
                look.map(|s| Surface {
                    back: rim_color,
                    uv: corners.map(|c| s.uv[c]),
                    map_uv: corners.map(|c| s.map_uv[c]),
                    ..s
                })
            };
            let n = Some([outward; 3]);
            triangle(
                [shift(a, 1.), shift(b, 1.), shift(b, -1.)],
                rim_color,
                false,
                n,
                roughness,
                rim([ca, cb, cb]),
            );
            triangle(
                [shift(a, 1.), shift(b, -1.), shift(a, -1.)],
                rim_color,
                false,
                n,
                roughness,
                rim([ca, cb, ca]),
            );
        }
        if let Some(schematic) = display.schematic {
            let eye = camera.eye();
            let world_per_px = |p: Vec3| {
                let depth = if camera.orthographic {
                    camera.distance
                } else {
                    p.distance(eye)
                };
                2. * depth * (camera.fov_radians() * 0.5).tan() / size[1].max(1) as f32
            };
            for [a, b] in schematic_lines(mesh, state, display.visible_faces, camera, schematic) {
                let mid = (a + b) * 0.5;
                let toward = if camera.orthographic {
                    camera.direction()
                } else {
                    (eye - mid).normalize_or_zero()
                };
                // Lift the ribbon a little towards the viewer so it wins the depth test against
                // the cloth it lies on, and give it a constant on-screen width.
                let lift = toward * world_per_px(mid) * 2.;
                let side = (b - a).cross(toward).normalize_or_zero()
                    * world_per_px(mid)
                    * schematic.line_px
                    * 0.5;
                let (a, b) = (a + lift, b + lift);
                let n = Some([toward; 3]);
                triangle(
                    [a - side, a + side, b + side],
                    schematic.line_color,
                    false,
                    n,
                    -1.,
                    None,
                );
                triangle(
                    [a - side, b + side, b - side],
                    schematic.line_color,
                    false,
                    n,
                    -1.,
                    None,
                );
            }
        }
        for t in trims {
            triangle(
                t.points.map(|p| p.as_vec3()),
                t.color,
                false,
                None,
                0.45,
                None,
            );
        }
        for seam in seams {
            for side in [1.0, -1.0] {
                let points: Vec<_> = seam
                    .probes
                    .iter()
                    .map(|p| {
                        p.terms
                            .iter()
                            .filter(|(_, w)| w.signum() == side)
                            .map(|(i, w)| state.x[*i] * w.abs())
                            .sum::<glam::DVec3>()
                            .as_vec3()
                    })
                    .collect();
                for pair in points.windows(2) {
                    let [a, b] = [pair[0], pair[1]];
                    let d = (b - a).cross(Vec3::Z).normalize_or_zero() * 0.0013;
                    triangle(
                        [a - d, a + d, b + d],
                        [1.0, 0.72, 0.22],
                        false,
                        None,
                        -1.,
                        None,
                    );
                    triangle(
                        [a - d, b + d, b - d],
                        [1.0, 0.72, 0.22],
                        false,
                        None,
                        -1.,
                        None,
                    );
                }
            }
        }
        if let Some(s) = sphere {
            let point = |i: usize, j: usize| {
                let theta = std::f32::consts::TAU * i as f32 / 40.0;
                let phi = std::f32::consts::PI * j as f32 / 20.0;
                s.center.as_vec3()
                    + s.radius as f32
                        * Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin())
            };
            for j in 0..20 {
                for i in 0..40 {
                    let [a, b, c, d] = [
                        point(i, j),
                        point(i + 1, j),
                        point(i, j + 1),
                        point(i + 1, j + 1),
                    ];
                    triangle([a, c, b], [0.38, 0.4, 0.46], false, None, -1., None);
                    triangle([b, c, d], [0.38, 0.4, 0.46], false, None, -1., None);
                }
            }
        }
        // Static avatar stays resident; camera transforms now happen in WGSL.
        if let Some(avatar) = avatar {
            let key = (
                avatar.vertices.as_ptr() as usize,
                avatar.triangles.as_ptr() as usize,
                display.avatar_color.map(f32::to_bits),
            );
            if key != self.avatar_key {
                use wgpu::util::DeviceExt;
                let mut body = Vec::<f32>::new();
                let mut normals = vec![Vec3::ZERO; avatar.vertices.len()];
                for t in &avatar.triangles {
                    let p = t.map(|i| avatar.vertices[i].as_vec3());
                    let n = (p[1] - p[0]).cross(p[2] - p[0]);
                    for i in t {
                        normals[*i] += n;
                    }
                }
                for n in &mut normals {
                    *n = n.normalize_or_zero();
                }
                for t in &avatar.triangles {
                    let p = t.map(|i| avatar.vertices[i].as_vec3());
                    for (i, v) in p.into_iter().enumerate() {
                        body.extend_from_slice(&v.extend(1.).to_array());
                        body.extend_from_slice(&normals[t[i]].to_array());
                        body.extend_from_slice(&display.avatar_color);
                        body.extend_from_slice(&[0., 0., 0., 0., -1.]);
                        body.extend_from_slice(&display.avatar_color);
                        body.extend_from_slice(&[0.; 5]);
                        body.extend_from_slice(&NO_MAP);
                    }
                }
                self.avatar_count = (body.len() / VERTEX_FLOATS) as u32;
                self.avatar_key = key;
                self.avatar_buffer = Some(device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("static avatar"),
                        contents: bytemuck::cast_slice(&body),
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                ));
            }
        } else {
            self.avatar_count = 0;
            self.avatar_key = (0, 0, [0; 3]);
            self.avatar_buffer = None;
        }
        let bytes = bytemuck::cast_slice(&data);
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two();
            self.vertices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("viewport-vertices"),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.render_state
            .queue
            .write_buffer(&self.vertices, 0, bytes);
        // Prints follow the weave and relief of the fabric they lie on.
        let looks: Vec<Vec<Option<Surface>>> = graphics
            .iter()
            .map(|draw| {
                draw.triangles
                    .iter()
                    .map(|t| {
                        let appearance = optics.and_then(|v| v.get(t.fabric)?.as_ref())?;
                        let grain = materials.get(t.fabric)?.grain_degrees;
                        let layers = layers.get(t.fabric).copied().unwrap_or([None; 3]);
                        Some(face_surface(appearance, grain, t.material, false, layers).1)
                    })
                    .collect()
            })
            .collect();
        let maps = self
            .fabric_maps
            .bind_for(device, &self.graphics.maps_layout());
        self.graphics
            .prepare(device, &self.render_state.queue, graphics, &looks, maps);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cloth-viewport"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cloth-viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.msaa_color.as_ref().unwrap_or(&self.color),
                    depth_slice: None,
                    resolve_target: self.msaa_color.as_ref().map(|_| &self.color),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self
                    .timers
                    .as_ref()
                    .map(|t| wgpu::RenderPassTimestampWrites {
                        query_set: &t.query,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }),
                ..Default::default()
            });
            pass.set_pipeline(&self.environment.pipeline);
            pass.set_bind_group(0, &self.environment.bind, &[]);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_bind_group(1, &self.fabric_maps.bind, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..(data.len() / VERTEX_FLOATS) as u32, 0..1);
            if let Some(body) = &self.avatar_buffer {
                pass.set_vertex_buffer(0, body.slice(..));
                pass.draw(0..self.avatar_count, 0..1);
            }
            self.graphics.draw(&mut pass);
        }
        if let Some(t) = &self.timers {
            encoder.resolve_query_set(&t.query, 0..2, &t.resolve, 0);
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, 16);
        }
        self.render_state.queue.submit([encoder.finish()]);
        self.texture_id
    }
    pub fn enable_timing(&mut self) -> bool {
        let d = &self.render_state.device;
        if !d.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return false;
        }
        self.timers = Some(Timers {
            query: d.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("viewport-timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolve: d.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: d.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        });
        true
    }
    pub fn gpu_time_ms(&self) -> Result<Option<f64>, String> {
        let Some(t) = &self.timers else {
            return Ok(None);
        };
        let bytes = read_buffer(&self.render_state.device, &t.read)?;
        let start = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let end = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        Ok(Some(
            end.wrapping_sub(start) as f64 * self.render_state.queue.get_timestamp_period() as f64
                / 1e6,
        ))
    }
    pub fn capture(&self, path: &std::path::Path) -> Result<(), String> {
        let row = (self.size[0] * 4).div_ceil(256) * 256;
        let d = &self.render_state.device;
        let buffer = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport-image"),
            size: row as u64 * self.size[1] as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = d.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.size[0],
                height: self.size[1],
                depth_or_array_layers: 1,
            },
        );
        self.render_state.queue.submit([encoder.finish()]);
        let data = read_buffer(d, &buffer)?;
        let pixels: Vec<_> = data
            .chunks_exact(row as usize)
            .flat_map(|r| r[..(self.size[0] * 4) as usize].iter().copied())
            .collect();
        image::save_buffer(
            path,
            &pixels,
            self.size[0],
            self.size[1],
            image::ColorType::Rgba8,
        )
        .map_err(|e| e.to_string())
    }
}
fn read_buffer(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    rx.recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let data = buffer
        .get_mapped_range(..)
        .map_err(|e| e.to_string())?
        .to_vec();
    buffer.unmap();
    Ok(data)
}
impl Drop for Viewport {
    fn drop(&mut self) {
        self.render_state
            .renderer
            .write()
            .free_texture(&self.texture_id);
    }
}

/// Camera navigation is pure math: these tests need no GPU and no window.
#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(width: f32, height: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(width, height))
    }

    fn close(actual: f32, expected: f32, tolerance: f32, what: &str) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{what}: {actual} vs {expected}"
        );
    }

    /// World point under a window position, on the focal plane through the
    /// target. This is the inverse of `Camera::project` for that plane.
    fn unproject(camera: &Camera, position: egui::Pos2, rect: egui::Rect) -> Vec3 {
        let (origin, direction) = camera.ray(position, rect);
        let (origin, direction) = (origin.as_vec3(), direction.as_vec3());
        let forward = camera.forward();
        let denominator = direction.dot(forward);
        assert!(
            denominator.abs() > 1e-6,
            "cursor ray parallel to the view axis"
        );
        origin + direction * ((camera.target - origin).dot(forward) / denominator)
    }

    #[test]
    fn presets_point_along_their_canonical_direction() {
        for preset in PresetView::ALL {
            let mut camera = Camera::default();
            let (distance, target) = (camera.distance, camera.target);
            camera.apply_preset(preset);
            let expected = preset.direction();
            let actual = camera.direction();
            assert!(
                (actual - expected).length() < 1e-5,
                "{}: {actual:?} vs {expected:?}",
                preset.label()
            );
            close(camera.distance, distance, 0., preset.label());
            assert!(
                (camera.target - target).length() == 0.,
                "{}",
                preset.label()
            );
        }
    }

    #[test]
    fn up_is_defined_and_perpendicular_at_every_preset() {
        for preset in PresetView::ALL {
            let mut camera = Camera::default();
            camera.apply_preset(preset);
            let up = camera.up();
            close(up.length(), 1., 1e-5, preset.label());
            assert!(
                up.dot(camera.direction()).abs() < 1e-5,
                "{} up is not perpendicular to the view direction",
                preset.label()
            );
            assert!(
                camera.right().length() > 0.99,
                "{} has a degenerate right vector",
                preset.label()
            );
            for orthographic in [false, true] {
                camera.orthographic = orthographic;
                assert!(
                    camera.matrix(1.6).is_finite(),
                    "{} ({orthographic}) produced a non-finite matrix",
                    preset.label()
                );
            }
        }
    }

    #[test]
    fn screen_and_world_round_trip_in_both_projections() {
        let rect = viewport(1280., 720.);
        for orthographic in [false, true] {
            let camera = Camera {
                orthographic,
                ..Default::default()
            };
            let focal = camera.target;
            for world in [
                focal,
                focal + camera.right() * 0.3 + camera.up() * 0.2,
                focal - camera.right() * 0.4 - camera.up() * 0.1,
            ] {
                let screen = camera.project(world, rect).expect("in front of the camera");
                let back = unproject(&camera, screen, rect);
                assert!(
                    (back - world).length() < 1e-3,
                    "{orthographic}: {world:?} -> {screen:?} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn zoom_keeps_the_world_point_under_the_cursor() {
        let rect = viewport(1200., 700.);
        for orthographic in [false, true] {
            let mut camera = Camera {
                orthographic,
                ..Default::default()
            };
            let cursor = egui::pos2(
                rect.left() + 0.18 * rect.width(),
                rect.top() + 0.72 * rect.height(),
            );
            let anchor = unproject(&camera, cursor, rect);
            let before = camera.project(anchor, rect).unwrap();
            let (yaw, pitch) = (camera.yaw, camera.pitch);
            camera.zoom_at(cursor, rect, 0.5);
            close(
                camera.distance,
                Camera::default().distance * 0.5,
                1e-5,
                "distance",
            );
            let after = camera.project(anchor, rect).unwrap();
            assert!(
                (after - before).length() < 0.05,
                "{orthographic}: anchor moved from {before:?} to {after:?}"
            );
            close(camera.yaw, yaw, 0., "yaw");
            close(camera.pitch, pitch, 0., "pitch");
        }
    }

    #[test]
    fn zoom_towards_the_centre_only_changes_the_distance() {
        let rect = viewport(900., 600.);
        let mut camera = Camera::default();
        let target = camera.target;
        let centre = camera.project(target, rect).unwrap();
        camera.zoom_at(rect.center(), rect, 2.);
        println!("centre drift: {:?}", (camera.target - target).length());
        // The centre ray is the view axis, so the target may only drift by the
        // f32 inverse-projection noise, and never along the screen plane.
        assert!((camera.target - target).length() < 1e-3);
        let after = camera.project(target, rect).unwrap();
        assert!((after - centre).length() < 0.05, "{after:?} vs {centre:?}");
        close(
            camera.distance,
            Camera::default().distance * 2.,
            1e-5,
            "distance",
        );
    }

    #[test]
    fn zoom_clamps_and_keeps_the_anchor() {
        let rect = viewport(800., 800.);
        let cursor = egui::pos2(rect.left() + 40., rect.top() + 660.);
        let mut camera = Camera::default();
        let anchor = unproject(&camera, cursor, rect);
        camera.zoom_at(cursor, rect, 1e6);
        close(camera.distance, MAX_DISTANCE, 1e-6, "clamped out");
        let projected = camera.project(anchor, rect).unwrap();
        assert!((projected - cursor).length() < 0.05, "{projected:?}");
        let mut camera = Camera::default();
        let anchor = unproject(&camera, cursor, rect);
        camera.zoom_at(cursor, rect, 1e-6);
        close(camera.distance, MIN_DISTANCE, 1e-6, "clamped in");
        let projected = camera.project(anchor, rect).unwrap();
        assert!((projected - cursor).length() < 0.05, "{projected:?}");
        // A non-positive factor is not a zoom and must not flip the distance.
        camera.zoom_at(cursor, rect, -1.);
        close(camera.distance, MIN_DISTANCE, 1e-6, "distance unchanged");
        camera.zoom(-4.);
        close(camera.distance, MIN_DISTANCE, 1e-6, "distance unchanged");
    }

    #[test]
    fn pan_moves_the_scene_with_the_pointer_pixel_for_pixel() {
        let rect = viewport(1000., 800.);
        let mut camera = Camera::default();
        let world = camera.target + camera.right() * 0.25 - camera.up() * 0.15;
        let delta = egui::Vec2::new(37., -21.);
        let before = camera.project(world, rect).unwrap();
        camera.pan_by(delta, rect);
        let after = camera.project(world, rect).unwrap();
        assert!(
            (after - before - delta).length() < 0.05,
            "moved {:?} instead of {delta:?}",
            after - before
        );
    }

    #[test]
    fn orbit_clamps_at_the_poles_and_keeps_the_model_upright_below_them() {
        let mut camera = Camera::default();
        camera.orbit_by(egui::Vec2::new(0., 100_000.));
        close(camera.pitch, MAX_PITCH, 1e-6, "pitch");
        camera.orbit_by(egui::Vec2::new(0., -200_000.));
        close(camera.pitch, -MAX_PITCH, 1e-6, "pitch");
        // Below the poles the camera up stays the world Y axis on screen.
        let mut camera = Camera::default();
        camera.orbit_by(egui::Vec2::new(20., 10.));
        close(camera.up().y, camera.pitch.cos(), 1e-5, "up.y");
        assert!(camera.up().y > 0., "model flipped after a small orbit");
        // The poles themselves are still upright and finite.
        for pitch in [MAX_PITCH, -MAX_PITCH] {
            camera.pitch = pitch;
            assert!(camera.up().y.abs() < 1e-5, "pole up is not horizontal");
            assert!(camera.right().length() > 0.99);
        }
    }

    #[test]
    fn field_of_view_scales_both_projections_and_is_clamped() {
        let rect = viewport(1000., 1000.);
        let offset = |camera: &Camera| {
            let p = camera.target + camera.up() * 0.1;
            (camera.project(p, rect).unwrap().y - rect.center().y).abs()
        };
        let mut camera = Camera::default();
        let wide = offset(&camera);
        camera.fov_degrees = 22.5;
        let narrow = offset(&camera);
        // Halving the angle roughly doubles the on-screen size (tan(22.5°/2) vs tan(45°/2)).
        let expected = (45f32.to_radians() * 0.5).tan() / (22.5f32.to_radians() * 0.5).tan();
        close(narrow / wide, expected, 0.01, "perspective zoom");
        camera.orthographic = true;
        let ortho_narrow = offset(&camera);
        close(
            ortho_narrow,
            narrow,
            0.5,
            "orthographic follows the same field of view",
        );
        camera.fov_degrees = 500.;
        close(
            camera.fov_radians().to_degrees(),
            120.,
            1e-4,
            "clamped high",
        );
        camera.fov_degrees = f32::NAN;
        close(
            camera.fov_radians().to_degrees(),
            DEFAULT_FOV_DEGREES,
            1e-4,
            "NaN falls back",
        );
    }

    #[test]
    fn three_quarter_views_sit_between_front_and_the_sides() {
        let front = PresetView::Front.direction();
        for (view, side) in [
            (PresetView::ThreeQuarterLeft, PresetView::Left),
            (PresetView::ThreeQuarterRight, PresetView::Right),
        ] {
            let d = view.direction();
            close(d.length(), 1., 1e-6, "unit direction");
            close(
                d.angle_between(front),
                45f32.to_radians(),
                1e-5,
                "45° from the front",
            );
            close(
                d.angle_between(side.direction()),
                45f32.to_radians(),
                1e-5,
                "45° from the side",
            );
        }
        assert_eq!(PresetView::ALL.len(), 9);
    }

    #[test]
    fn restore_brings_back_the_field_of_view() {
        let mut camera = Camera::default();
        camera.save_pose();
        camera.fov_degrees = 70.;
        assert!(camera.restore_pose());
        close(
            camera.fov_degrees,
            DEFAULT_FOV_DEGREES,
            1e-6,
            "restored field of view",
        );
    }

    #[test]
    fn orthographic_matches_the_perspective_scale_at_the_focal_plane() {
        let rect = viewport(1024., 768.);
        let mut camera = Camera::default();
        let focal_half_height = camera.distance * (camera.fov_radians() * 0.5).tan();
        let top = camera.target + camera.up() * focal_half_height;
        let perspective_top = camera.project(top, rect).unwrap();
        close(perspective_top.y, rect.top(), 0.01, "perspective framing");
        camera.orthographic = true;
        let orthographic_top = camera.project(top, rect).unwrap();
        close(orthographic_top.y, rect.top(), 0.01, "orthographic framing");
        let centre = camera.project(camera.target, rect).unwrap();
        close(centre.x, rect.center().x, 0.01, "target stays centred");
        close(centre.y, rect.center().y, 0.01, "target stays centred");
    }

    #[test]
    fn orthographic_projects_parallel_lines_onto_the_same_screen_point() {
        let rect = viewport(900., 700.);
        let mut camera = Camera {
            orthographic: true,
            ..Default::default()
        };
        let lateral = camera.right() * 0.2;
        let near = camera.target + lateral - camera.forward() * 0.4;
        let far = camera.target + lateral + camera.forward() * 0.4;
        let a = camera.project(near, rect).unwrap();
        let b = camera.project(far, rect).unwrap();
        assert!(
            (a - b).length() < 0.01,
            "orthographic is not parallel: {a:?} vs {b:?}"
        );
        // Perspective is not: the same offset shrinks with depth.
        camera.orthographic = false;
        let a = camera.project(near, rect).unwrap();
        let b = camera.project(far, rect).unwrap();
        assert!((a.x - b.x).abs() > 1., "perspective should foreshorten");
    }

    #[test]
    fn frame_fits_the_bounding_sphere_in_both_projections() {
        let rect = viewport(900., 600.);
        let center = Vec3::new(0.1, 0.9, -0.2);
        let radius = 0.6;
        for orthographic in [false, true] {
            let mut camera = Camera {
                orthographic,
                ..Default::default()
            };
            camera.frame_in(center, radius, rect);
            assert!((camera.target - center).length() < 1e-6);
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                for sign in [1., -1.] {
                    let extreme = center + axis * (sign * radius);
                    let screen = camera.project(extreme, rect).unwrap();
                    assert!(
                        rect.contains(screen),
                        "{orthographic}: {screen:?} outside {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn pending_frame_waits_for_the_viewport_rectangle() {
        let mut camera = Camera::default();
        let rect = viewport(400., 900.);
        camera.request_frame(Vec3::new(0., 1., 0.), 0.5);
        assert!(camera.apply_pending_frame(rect));
        assert!(
            !camera.apply_pending_frame(rect),
            "the request is consumed once"
        );
        // A narrow viewport needs more distance than a square one.
        let mut square = Camera::default();
        square.frame(Vec3::new(0., 1., 0.), 0.5);
        assert!(camera.distance > square.distance);
    }

    #[test]
    fn presets_and_frames_can_be_undone_with_restore() {
        let mut camera = Camera::default();
        let original = (camera.yaw, camera.pitch, camera.distance, camera.target);
        camera.apply_preset(PresetView::Top);
        assert!(camera.has_saved_pose());
        let (yaw, pitch) = PresetView::Top.angles();
        close(camera.yaw, yaw, 1e-6, "yaw");
        close(camera.pitch, pitch, 1e-6, "pitch");
        assert!(camera.restore_pose());
        close(camera.yaw, original.0, 0., "restored yaw");
        close(camera.pitch, original.1, 0., "restored pitch");
        close(camera.distance, original.2, 0., "restored distance");
        assert!((camera.target - original.3).length() == 0.);
        // Restore toggles back to the preset it replaced.
        assert!(camera.restore_pose());
        close(camera.pitch, pitch, 1e-6, "toggled pitch");
        assert!(!Camera::default().restore_pose(), "nothing saved yet");
    }

    #[test]
    fn reset_view_keeps_the_projection_mode() {
        let mut camera = Camera::default();
        camera.apply_preset(PresetView::Bottom);
        camera.toggle_projection();
        camera.frame_in(Vec3::new(1., 1., 1.), 2., viewport(800., 600.));
        camera.reset_view();
        let default = Camera::default();
        close(camera.yaw, default.yaw, 0., "yaw");
        close(camera.distance, default.distance, 0., "distance");
        assert!(
            camera.orthographic,
            "the projection choice survives a reset"
        );
    }
    #[test]
    fn diagnostic_overlays_keep_precedence_over_custom_optics() {
        let optics = crate::garment::appearance::FabricAppearance {
            color_srgb: [255, 0, 0],
            roughness: 0.3,
            ..Default::default()
        };
        assert_eq!(
            cloth_surface(None, false, false, None, None),
            ([0.91, 0.92, 0.94], -1.)
        );
        assert_eq!(
            cloth_surface(Some(&optics), false, false, None, None),
            ([1., 0., 0.], 0.3)
        );
        assert_eq!(
            cloth_surface(Some(&optics), true, true, Some(STRAIN_BANDS[7]), None),
            ([1., 0.23, 0.10], -1.)
        );
        assert_eq!(
            cloth_surface(Some(&optics), false, true, Some(STRAIN_BANDS[7]), None),
            ([1., 0.2, 0.25], -1.)
        );
        assert_eq!(
            cloth_surface(Some(&optics), false, false, Some(STRAIN_BANDS[7]), None).1,
            -1.
        );
    }
    #[test]
    fn piece_colours_replace_the_fabric_but_not_the_fit_map() {
        let optics = crate::garment::appearance::FabricAppearance {
            color_srgb: [255, 0, 0],
            roughness: 0.3,
            ..Default::default()
        };
        let tint = Some([0.2, 0.4, 0.9]);
        assert_eq!(
            cloth_surface(Some(&optics), false, false, None, tint),
            ([0.2, 0.4, 0.9], -1.)
        );
        assert_eq!(
            cloth_surface(Some(&optics), false, false, Some(STRAIN_BANDS[0]), tint).0,
            STRAIN_BANDS[0]
        );
    }
    #[test]
    fn strain_bands_split_the_range_in_eight_equal_steps() {
        assert_eq!(strain_band(0.0, 0.1), 0);
        assert_eq!(strain_band(0.0124, 0.1), 0);
        assert_eq!(strain_band(0.0126, 0.1), 1);
        assert_eq!(strain_band(0.05, 0.1), 4);
        assert_eq!(strain_band(0.0999, 0.1), 7);
        assert_eq!(
            strain_band(0.5, 0.1),
            7,
            "past the maximum stays in the last band"
        );
        assert_eq!(strain_band(0.05, 0.2), 2, "a wider range lowers the band");
        assert_eq!(strain_band(f64::NAN, 0.1), 0);
        assert_eq!(strain_band(0.05, 0.0), 0);
    }
    #[test]
    fn schematic_lines_trace_free_edges_and_folds() {
        use crate::model::{Mesh, State, Triangle};
        use glam::{DMat2, DVec2, DVec3};
        let triangle = |vertices: [usize; 3]| Triangle {
            vertices,
            rest: [DVec2::ZERO; 3],
            inv_dm: DMat2::IDENTITY,
            area: 1.,
            material: 0,
        };
        let mesh = |x: Vec<DVec3>| {
            (
                Mesh {
                    uv: vec![DVec2::ZERO; x.len()],
                    triangles: vec![triangle([0, 1, 2]), triangle([0, 2, 3])],
                    hinges: vec![],
                    rest_area: vec![],
                    masses: vec![],
                    pins: vec![],
                },
                State {
                    v: vec![DVec3::ZERO; x.len()],
                    x,
                    time: 0.,
                },
            )
        };
        let options = Schematic {
            garment_color: [1.; 3],
            line_color: [0.; 3],
            line_px: 1.5,
            silhouette: true,
            outlines: true,
        };
        // A flat square facing the default camera: four free edges, no fold.
        let camera = Camera::new(0., 0., 3., Vec3::ZERO);
        let flat = vec![
            DVec3::new(-0.5, -0.5, 0.),
            DVec3::new(0.5, -0.5, 0.),
            DVec3::new(0.5, 0.5, 0.),
            DVec3::new(-0.5, 0.5, 0.),
        ];
        let (m, s) = mesh(flat.clone());
        assert_eq!(schematic_lines(&m, &s, None, &camera, options).len(), 4);
        // Folded over the diagonal (behind the first half) so the second half faces away: the
        // diagonal becomes a silhouette line.
        let mut folded = flat;
        folded[3] = DVec3::new(0.5, -0.5, -0.2);
        let (m, s) = mesh(folded);
        let lines = schematic_lines(&m, &s, None, &camera, options);
        assert_eq!(lines.len(), 5, "four edges plus the fold");
        let only_folds = Schematic {
            outlines: false,
            ..options
        };
        assert_eq!(schematic_lines(&m, &s, None, &camera, only_folds).len(), 1);
        // Hidden triangles contribute nothing; the remaining one shows three free edges.
        assert_eq!(
            schematic_lines(&m, &s, Some(&[true, false]), &camera, options).len(),
            3
        );
    }
    #[test]
    fn strain_gradient_runs_through_the_eight_colours() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(close(strain_color(0.0, 0.2, true), STRAIN_BANDS[0]));
        assert!(close(strain_color(0.2, 0.2, true), STRAIN_BANDS[7]));
        assert!(close(strain_color(0.9, 0.2, true), STRAIN_BANDS[7]));
        // Each colour sits at k/7 of the range; halfway between two is their mean.
        let mid = strain_color(0.2 * 2.5 / 7., 0.2, true);
        let mean = [0, 1, 2].map(|k| (STRAIN_BANDS[2][k] + STRAIN_BANDS[3][k]) * 0.5);
        assert!(close(mid, mean), "{mid:?} vs {mean:?}");
        assert_eq!(strain_color(0.05, 0.2, false), STRAIN_BANDS[2]);
        assert_eq!(strain_color(f64::NAN, 0.2, true), STRAIN_BANDS[0]);
    }
}

#[cfg(test)]
mod face_tests {
    use super::*;
    use crate::garment::appearance::{FabricAppearance, Weave, WeaveKind};

    #[test]
    fn faces_show_front_and_back_colours_and_flipping_swaps_them() {
        let a = FabricAppearance {
            color_srgb: [200, 30, 30],
            back_color_srgb: Some([30, 30, 200]),
            ..Default::default()
        };
        let uv = [glam::DVec2::ZERO, glam::DVec2::X, glam::DVec2::Y];
        let (front, s) = face_surface(&a, 0., uv, false, [None; 3]);
        assert!(
            front[0] > front[2] && s.back[2] > s.back[0],
            "red front, blue back"
        );
        let (front, s) = face_surface(&a, 0., uv, true, [None; 3]);
        assert!(
            front[2] > front[0] && s.back[0] > s.back[2],
            "flipped: blue front, red back"
        );
        // Without a back colour both faces are the front colour.
        let plain = FabricAppearance::default();
        let (front, s) = face_surface(&plain, 0., uv, true, [None; 3]);
        assert_eq!(front, s.back);
    }

    #[test]
    fn the_weave_follows_the_grain() {
        let a = FabricAppearance {
            weave: Some(Weave {
                kind: WeaveKind::Twill,
                spacing_mm: 2.,
                contrast: 0.5,
            }),
            ..Default::default()
        };
        let uv = [
            glam::DVec2::ZERO,
            glam::DVec2::new(0.01, 0.),
            glam::DVec2::Y,
        ];
        let (_, s) = face_surface(&a, 90., uv, false, [None; 3]);
        assert_eq!(s.weave, [2., 0.002, 0.5]);
        // A quarter-turned grain: the material x direction runs across the warp.
        assert!(
            s.uv[1][0].abs() < 1e-9 && (s.uv[1][1] + 0.01).abs() < 1e-7,
            "{:?}",
            s.uv
        );
    }

    #[test]
    fn a_fabric_texture_repeats_along_the_grain_on_its_layer() {
        use crate::garment::appearance::FabricTexture;
        let a = FabricAppearance {
            texture: Some(FabricTexture {
                asset_id: "image_1".into(),
                width_mm: 50.,
                height_mm: 25.,
                rotation_degrees: 0.,
                relief_mm: 0.5,
                normal_asset_id: None,
                roughness_asset_id: None,
            }),
            ..Default::default()
        };
        let uv = [
            glam::DVec2::ZERO,
            glam::DVec2::new(0.1, 0.),
            glam::DVec2::new(0., 0.05),
        ];
        let (_, s) = face_surface(&a, 0., uv, false, [Some(2), None, Some(3)]);
        assert_eq!(s.map, [2., 0.0005, -1., 3.]);
        // 10 cm along the warp is two 5 cm repeats; 5 cm along the weft is two 2.5 cm ones.
        assert_eq!(s.map_uv[1], [2., 0.]);
        assert_eq!(s.map_uv[2], [0., 2.]);
        // Without a loaded image (no layer) the face shows no texture.
        let (_, s) = face_surface(&a, 0., uv, false, [None; 3]);
        assert_eq!(s.map, [-1., 0., -1., -1.]);
        // A quarter-turned grain turns the repeat with it.
        let (_, s) = face_surface(&a, 90., uv, false, [Some(0), None, None]);
        assert!((s.map_uv[1][0]).abs() < 1e-6 && (s.map_uv[1][1] + 4.).abs() < 1e-5);
    }
}
