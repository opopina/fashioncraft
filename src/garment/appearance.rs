//! Authored optical properties. These never calibrate or modify cloth mechanics.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FabricAppearance {
    /// Display sRGB bytes. Rendering decodes these before optical shading.
    pub color_srgb: [u8; 3],
    /// Dimensionless perceptual roughness, in [0, 1].
    pub roughness: f64,
    /// Colour of the back face (the side toward the faces' winding normal, inside a placed
    /// garment); none shows the front colour on both sides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_color_srgb: Option<[u8; 3]>,
    /// Procedural weave drawn on the surface, along the fabric's grain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weave: Option<Weave>,
    /// An image repeated over both faces along the grain, tinted by their colours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<FabricTexture>,
    /// How thick the cloth looks, mm: its two faces are drawn this far apart with a rim along
    /// the outline. Zero draws one surface. Never changes the simulation.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub render_thickness_mm: f64,
    /// How metallic the surface is, 0 (cloth, plastic) to 1 (bare metal: foil, lamé). A
    /// texture's metalness map replaces it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub metalness: f64,
}

/// Thickest a fabric may look, mm.
pub const MAX_RENDER_THICKNESS_MM: f64 = 20.;

/// An image tiled over a fabric. Its pixels live in the project's image assets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FabricTexture {
    pub asset_id: String,
    /// Real width of one repeat of the image along the warp, mm.
    pub width_mm: f64,
    /// Real height of one repeat of the image along the weft, mm.
    pub height_mm: f64,
    /// Turn of the image from the grain, degrees.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation_degrees: f64,
    /// Relief drawn from the image's brightness: how far its brightest parts stand above
    /// its darkest, mm. Zero is flat.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub relief_mm: f64,
    /// Image of the surface's normals over one repeat, as normal maps are usually made: red
    /// along the repeat's width, green up it, blue out of the cloth. None keeps the surface
    /// smooth, or as the brightness relief draws it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_asset_id: Option<String>,
    /// Grey image of the roughness over one repeat (black glossy, white matt), in place of
    /// the fabric's single roughness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness_asset_id: Option<String>,
    /// The normal map is drawn the DirectX way (green pointing down the repeat): its green
    /// channel is flipped when it is read.
    #[serde(default, skip_serializing_if = "is_false")]
    pub normal_directx: bool,
    /// The roughness map holds glossiness (black matt, white glossy): it is inverted when read.
    #[serde(default, skip_serializing_if = "is_false")]
    pub roughness_is_glossiness: bool,
    /// Grey image of the metalness over one repeat (black cloth, white metal), in place of
    /// the fabric's single metalness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metalness_asset_id: Option<String>,
    /// Grey image of the opacity over one repeat (black clear, white solid), for lace and
    /// mesh: the cloth is cut away where it is dark. Its alpha channel counts too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity_asset_id: Option<String>,
    /// Grey height image over one repeat (black low, white high), drawn as displacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displacement_asset_id: Option<String>,
    /// Height between the displacement map's black and white, mm.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub displacement_mm: f64,
    /// Grey ambient-occlusion image over one repeat (black hidden, white open): darkens the
    /// light that comes from all around.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occlusion_asset_id: Option<String>,
}

/// A map of a fabric texture besides its colour image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapSlot {
    Normal,
    Roughness,
    Metalness,
    Opacity,
    Displacement,
    Occlusion,
}

impl MapSlot {
    pub const ALL: [Self; 6] = [
        Self::Normal,
        Self::Roughness,
        Self::Metalness,
        Self::Opacity,
        Self::Displacement,
        Self::Occlusion,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal map",
            Self::Roughness => "Roughness map",
            Self::Metalness => "Metalness map",
            Self::Opacity => "Opacity map",
            Self::Displacement => "Displacement map",
            Self::Occlusion => "Ambient occlusion map",
        }
    }
}

impl FabricTexture {
    /// Every image asset the texture draws on: its colours, then its maps when it has them.
    pub fn asset_ids(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.asset_id.as_str())
            .chain(MapSlot::ALL.into_iter().filter_map(|slot| self.map(slot)))
    }
    /// The asset of one of the texture's maps.
    pub fn map(&self, slot: MapSlot) -> Option<&str> {
        match slot {
            MapSlot::Normal => self.normal_asset_id.as_deref(),
            MapSlot::Roughness => self.roughness_asset_id.as_deref(),
            MapSlot::Metalness => self.metalness_asset_id.as_deref(),
            MapSlot::Opacity => self.opacity_asset_id.as_deref(),
            MapSlot::Displacement => self.displacement_asset_id.as_deref(),
            MapSlot::Occlusion => self.occlusion_asset_id.as_deref(),
        }
    }
    /// The asset slot of one of the texture's maps, to set or clear it.
    pub fn map_mut(&mut self, slot: MapSlot) -> &mut Option<String> {
        match slot {
            MapSlot::Normal => &mut self.normal_asset_id,
            MapSlot::Roughness => &mut self.roughness_asset_id,
            MapSlot::Metalness => &mut self.metalness_asset_id,
            MapSlot::Opacity => &mut self.opacity_asset_id,
            MapSlot::Displacement => &mut self.displacement_asset_id,
            MapSlot::Occlusion => &mut self.occlusion_asset_id,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Largest height a displacement map may draw, mm.
pub const MAX_DISPLACEMENT_MM: f64 = 10.;

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// Largest relief a fabric texture may draw, mm.
pub const MAX_TEXTURE_RELIEF_MM: f64 = 5.;

impl FabricTexture {
    /// Image coordinates of a point given in warp and weft metres: one repeat spans [0,1].
    pub fn image_uv(&self, warp_weft: glam::DVec2) -> glam::DVec2 {
        let (sin, cos) = self.rotation_degrees.to_radians().sin_cos();
        let p = glam::DVec2::new(
            warp_weft.x * cos + warp_weft.y * sin,
            -warp_weft.x * sin + warp_weft.y * cos,
        );
        p / glam::DVec2::new(self.width_mm, self.height_mm) * 1000.
    }
}

/// A woven or knitted surface drawn procedurally: thread relief and shading, no texture file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weave {
    pub kind: WeaveKind,
    /// Distance between neighbouring threads, mm.
    pub spacing_mm: f64,
    /// How much darker the sunken threads look, 0 to 1.
    pub contrast: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaveKind {
    Plain,
    Twill,
    Satin,
    Knit,
}

impl WeaveKind {
    pub const ALL: [Self; 4] = [Self::Plain, Self::Twill, Self::Satin, Self::Knit];
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Twill => "Twill",
            Self::Satin => "Satin",
            Self::Knit => "Knit",
        }
    }
    /// The shader's code for this weave.
    pub fn code(self) -> f32 {
        match self {
            Self::Plain => 1.,
            Self::Twill => 2.,
            Self::Satin => 3.,
            Self::Knit => 4.,
        }
    }
}

impl Default for Weave {
    fn default() -> Self {
        Self {
            kind: WeaveKind::Plain,
            spacing_mm: 1.,
            contrast: 0.35,
        }
    }
}
impl Default for FabricAppearance {
    fn default() -> Self {
        Self {
            color_srgb: [232, 235, 240],
            roughness: 0.8,
            back_color_srgb: None,
            weave: None,
            texture: None,
            render_thickness_mm: 0.,
            metalness: 0.,
        }
    }
}
impl FabricAppearance {
    pub fn validate(&self) -> Result<(), String> {
        if !self.roughness.is_finite() || !(0.0..=1.0).contains(&self.roughness) {
            return Err("Optical roughness must be finite and between 0 and 1".into());
        }
        if !(0.0..=1.0).contains(&self.metalness) {
            return Err("Metalness must be between 0 and 1".into());
        }
        if let Some(w) = &self.weave
            && (!(0.05..=20.0).contains(&w.spacing_mm) || !(0.0..=1.0).contains(&w.contrast))
        {
            return Err(
                "A weave needs a thread spacing of 0.05 to 20 mm and a contrast of 0 to 1".into(),
            );
        }
        if !(0.0..=MAX_RENDER_THICKNESS_MM).contains(&self.render_thickness_mm) {
            return Err(format!(
                "A fabric's look thickness must be 0 to {MAX_RENDER_THICKNESS_MM} mm"
            ));
        }
        if let Some(t) = &self.texture {
            let id_ok = t.asset_ids().all(|id| {
                !id.is_empty()
                    && id.len() <= 80
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            });
            if !id_ok {
                return Err("A fabric texture needs an image asset ID".into());
            }
            if !(1.0..=5000.0).contains(&t.width_mm) || !(1.0..=5000.0).contains(&t.height_mm) {
                return Err("A fabric texture repeat must be 1 to 5000 mm on each side".into());
            }
            if !t.rotation_degrees.is_finite()
                || !(0.0..=MAX_TEXTURE_RELIEF_MM).contains(&t.relief_mm)
            {
                return Err(format!(
                    "A fabric texture needs a finite angle and a relief of 0 to {MAX_TEXTURE_RELIEF_MM} mm"
                ));
            }
            if !(0.0..=MAX_DISPLACEMENT_MM).contains(&t.displacement_mm) {
                return Err(format!(
                    "A displacement map's height must be 0 to {MAX_DISPLACEMENT_MM} mm"
                ));
            }
        }
        Ok(())
    }
    pub fn linear_color(&self) -> [f32; 3] {
        linear(self.color_srgb)
    }
    /// The back face colour, linear RGB (the front one when no back colour is set).
    pub fn linear_back_color(&self) -> [f32; 3] {
        linear(self.back_color_srgb.unwrap_or(self.color_srgb))
    }
    /// The shader's weave parameters: kind code (0 = none), thread spacing (m), contrast.
    pub fn weave_parameters(&self) -> [f32; 3] {
        self.weave.as_ref().map_or([0.; 3], |w| {
            [
                w.kind.code(),
                (w.spacing_mm / 1000.) as f32,
                w.contrast as f32,
            ]
        })
    }
}

fn linear(srgb: [u8; 3]) -> [f32; 3] {
    srgb.map(|v| {
        let s = f32::from(v) / 255.;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    })
}

impl FabricAppearance {}

/// Rebind current document optics to physical panel material slots without
/// modifying or rebuilding the compiled garment retained by the worker.
pub fn bind(
    template: &super::GarmentTemplate,
    garment: &super::CompiledGarment,
) -> Vec<Option<FabricAppearance>> {
    let mut result = vec![None; garment.materials.len()];
    for panel in &garment.panels {
        if let Some(source) = template.panels.iter().find(|p| p.id == panel.id) {
            result[panel.material] = template.fabric_appearance.get(&source.fabric).cloned();
        }
    }
    result
}
