//! Graphics (prints and logos) placed on a piece: material-space placement math, clipping
//! to the image rectangle and the 3D triangles a renderer draws. Image files live in the
//! fitting project (see `fitting::ImageAsset`); this module never loads pixels.
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// A face of the authored panel, not an instruction to mirror image pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphicSide {
    Front,
    Back,
}

/// Rectangle in panel-local material coordinates, in metres.
///
/// Image UV (0,0) maps to the local negative half-size corner before rotation;
/// (1,1) maps to the positive corner. Positive rotation follows the 2D material
/// coordinate axes. Identifiers are opaque IDs, not paths or URLs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicPlacement {
    pub id: String,
    pub panel_id: String,
    pub asset_id: String,
    pub center_m: DVec2,
    pub size_m: DVec2,
    pub rotation_degrees: f64,
    pub side: GraphicSide,
}

/// Transient image coordinates for one compiled triangle. Vertex IDs belong
/// only to that compiled mesh: discard and rebuild these records on remeshing.
/// Coordinates may lie outside [0,1]; a consumer must clip or discard them.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphicTriangleUv {
    pub vertices: [usize; 3],
    pub image_uv: [DVec2; 3],
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl GraphicPlacement {
    /// Validate numerical and identifier shape only. Each dimension must be
    /// at least 4096 f64 epsilon times the largest of 1 metre, the absolute
    /// center coordinates and the dimensions. This bounds extreme cancellation
    /// in ordinary [0,1] image UV; it is not an exact round-trip guarantee.
    /// The caller must resolve IDs and define containment, asset portability
    /// and topology policies.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) || !valid_id(&self.panel_id) || !valid_id(&self.asset_id) {
            return Err(
                "Graphic, panel and asset IDs must use 1–80 ASCII letters, digits, '_' or '-'"
                    .into(),
            );
        }
        if !self.center_m.is_finite()
            || self.center_m.abs().max_element() > 1e6
            || !self.size_m.is_finite()
            || self.size_m.min_element() < 1e-9
            || self.size_m.max_element() > 1e6
            || !self.rotation_degrees.is_finite()
        {
            return Err("Graphic placement requires finite metre coordinates, positive size and finite rotation".into());
        }
        let scale = self
            .center_m
            .abs()
            .max_element()
            .max(self.size_m.max_element())
            .max(1.);
        if self.size_m.min_element() < 4096. * f64::EPSILON * scale {
            return Err("Graphic dimensions are too small relative to their position or aspect ratio for reliable coordinates".into());
        }
        Ok(())
    }

    /// Check panel identity against an existing template. Asset resolution and
    /// the clipping policy for a placement crossing the contour remain the
    /// caller's responsibility.
    pub fn validate_panel(&self, template: &super::GarmentTemplate) -> Result<(), String> {
        self.validate()?;
        if !template
            .panels
            .iter()
            .any(|panel| panel.id == self.panel_id)
        {
            return Err("Graphic references a missing panel".into());
        }
        Ok(())
    }

    /// Build image UVs from the named panel's material rest coordinates.
    /// This does not inspect 3D positions, render, clip or load an asset.
    /// Rebuild the returned records after every remesh or placement change.
    pub fn triangle_uvs(
        &self,
        garment: &super::CompiledGarment,
    ) -> Result<Vec<GraphicTriangleUv>, String> {
        self.validate()?;
        let panel = garment
            .panels
            .iter()
            .find(|panel| panel.id == self.panel_id)
            .ok_or("Graphic references a missing compiled panel")?;
        let triangles = garment
            .mesh
            .triangles
            .get(panel.triangles.clone())
            .ok_or("Invalid compiled panel triangle range")?;
        let mut result = Vec::new();
        result
            .try_reserve(triangles.len())
            .map_err(|_| "Graphic triangle allocation failed")?;
        let (sin, cos) = self
            .rotation_degrees
            .rem_euclid(360.)
            .to_radians()
            .sin_cos();
        for triangle in triangles {
            if triangle
                .vertices
                .iter()
                .any(|v| !panel.vertices.contains(v))
            {
                return Err("Compiled triangle crosses panel vertex ranges".into());
            }
            let mut image_uv = [DVec2::ZERO; 3];
            for (i, &vertex) in triangle.vertices.iter().enumerate() {
                let material_uv = garment
                    .mesh
                    .uv
                    .get(vertex)
                    .ok_or("Missing compiled material vertex")?;
                image_uv[i] = self.inverse_with_rotation(*material_uv, sin, cos)?;
            }
            result.push(GraphicTriangleUv {
                vertices: triangle.vertices,
                image_uv,
            });
        }
        Ok(result)
    }

    /// Convert normalized image coordinates to panel-local material metres.
    /// Values outside [0,1] are allowed for mathematical extrapolation.
    pub fn uv_to_material(&self, uv: DVec2) -> Result<DVec2, String> {
        self.validate()?;
        if !uv.is_finite() {
            return Err("Graphic UV must be finite".into());
        }
        let angle = self.rotation_degrees.rem_euclid(360.).to_radians();
        let (sin, cos) = angle.sin_cos();
        let p = (uv - DVec2::splat(0.5)) * self.size_m;
        let result = self.center_m + DVec2::new(cos * p.x - sin * p.y, sin * p.x + cos * p.y);
        if !result.is_finite() {
            return Err("Graphic transform exceeds finite coordinates".into());
        }
        Ok(result)
    }

    /// Convert panel-local metres to normalized image coordinates.
    pub fn material_to_uv(&self, point_m: DVec2) -> Result<DVec2, String> {
        self.validate()?;
        let angle = self.rotation_degrees.rem_euclid(360.).to_radians();
        let (sin, cos) = angle.sin_cos();
        self.inverse_with_rotation(point_m, sin, cos)
    }

    fn inverse_with_rotation(&self, point_m: DVec2, sin: f64, cos: f64) -> Result<DVec2, String> {
        if !point_m.is_finite() {
            return Err("Material position must be finite".into());
        }
        let p = point_m - self.center_m;
        let result = DVec2::new(cos * p.x + sin * p.y, -sin * p.x + cos * p.y) / self.size_m
            + DVec2::splat(0.5);
        if !result.is_finite() {
            return Err("Graphic inverse transform exceeds finite coordinates".into());
        }
        Ok(result)
    }
}

/// How far a graphic floats off the cloth surface, so it never fights the fabric for depth
/// (trims sit higher, on top of graphics).
pub const GRAPHIC_LIFT_M: f64 = 0.0008;

/// One triangle of a graphic on the 3D cloth: lifted points, the normals of the side it lies
/// on, and normalized image coordinates inside [0,1].
#[derive(Clone, Debug, PartialEq)]
pub struct GraphicTriangle {
    pub points: [DVec3; 3],
    pub normals: [DVec3; 3],
    pub uv: [DVec2; 3],
}

/// The parts of a triangle with image coordinates `uv` that lie inside the image square
/// [0,1]², as triangles of barycentric weights over the original corners. Empty when the
/// triangle misses the image; the original triangle when it lies wholly inside.
pub fn clip_to_image(uv: [DVec2; 3]) -> Vec<[DVec3; 3]> {
    let inside = |p: DVec2| (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y);
    if uv.iter().all(|p| inside(*p)) {
        return vec![[DVec3::X, DVec3::Y, DVec3::Z]];
    }
    let (lo, hi) = (uv[0].min(uv[1]).min(uv[2]), uv[0].max(uv[1]).max(uv[2]));
    if hi.x < 0. || hi.y < 0. || lo.x > 1. || lo.y > 1. {
        return vec![];
    }
    // Sutherland–Hodgman against the four sides, carrying barycentric weights.
    let mut polygon: Vec<(DVec2, DVec3)> =
        uv.into_iter().zip([DVec3::X, DVec3::Y, DVec3::Z]).collect();
    for (axis, keep_above, bound) in [(0, true, 0.), (0, false, 1.), (1, true, 0.), (1, false, 1.)]
    {
        let distance = |p: DVec2| {
            let d = p[axis] - bound;
            if keep_above { d } else { -d }
        };
        let mut next = Vec::with_capacity(polygon.len() + 2);
        for k in 0..polygon.len() {
            let (a, wa) = polygon[k];
            let (b, wb) = polygon[(k + 1) % polygon.len()];
            let (da, db) = (distance(a), distance(b));
            if da >= 0. {
                next.push((a, wa));
            }
            if (da >= 0.) != (db >= 0.) {
                let t = da / (da - db);
                next.push((a + (b - a) * t, wa + (wb - wa) * t));
            }
        }
        polygon = next;
        if polygon.len() < 3 {
            return vec![];
        }
    }
    (1..polygon.len() - 1)
        .map(|k| [polygon[0].1, polygon[k].1, polygon[k + 1].1])
        .collect()
}

impl GraphicPlacement {
    /// The 3D triangles of this graphic on the cloth at positions `x`, clipped to the image
    /// and lifted [`GRAPHIC_LIFT_M`] off the side it is printed on. `flipped` says whether
    /// the piece's normals are flipped, which swaps its front and back.
    pub fn graphic_triangles(
        &self,
        garment: &super::CompiledGarment,
        x: &[DVec3],
        flipped: bool,
    ) -> Result<Vec<GraphicTriangle>, String> {
        let triangles = self.triangle_uvs(garment)?;
        if x.len() != garment.mesh.uv.len() {
            return Err("Graphic positions do not match the compiled mesh".into());
        }
        let panel = garment
            .panels
            .iter()
            .find(|panel| panel.id == self.panel_id)
            .ok_or("Graphic references a missing compiled panel")?;
        // Smooth winding normals of the piece; its front faces away from them (towards them
        // when flipped), and a back print sits on the other side.
        let mut normals = vec![DVec3::ZERO; panel.vertices.len()];
        for t in &triangles {
            let [a, b, c] = t.vertices.map(|v| x[v]);
            let n = (b - a).cross(c - a);
            for v in t.vertices {
                normals[v - panel.vertices.start] += n;
            }
        }
        let front = if flipped { 1. } else { -1. };
        let side = match self.side {
            GraphicSide::Front => front,
            GraphicSide::Back => -front,
        };
        let normal = |v: usize| normals[v - panel.vertices.start].normalize_or_zero() * side;
        let mut out = Vec::new();
        for t in &triangles {
            let corners = t
                .vertices
                .map(|v| (x[v] + normal(v) * GRAPHIC_LIFT_M, normal(v)));
            for weights in clip_to_image(t.image_uv) {
                let blend = |w: DVec3| {
                    let p = corners[0].0 * w.x + corners[1].0 * w.y + corners[2].0 * w.z;
                    let n = corners[0].1 * w.x + corners[1].1 * w.y + corners[2].1 * w.z;
                    let uv = t.image_uv[0] * w.x + t.image_uv[1] * w.y + t.image_uv[2] * w.z;
                    (p, n.normalize_or_zero(), uv.clamp(DVec2::ZERO, DVec2::ONE))
                };
                let [a, b, c] = weights.map(blend);
                out.push(GraphicTriangle {
                    points: [a.0, b.0, c.0],
                    normals: [a.1, b.1, c.1],
                    uv: [a.2, b.2, c.2],
                });
            }
        }
        Ok(out)
    }
}
