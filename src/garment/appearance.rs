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
        }
    }
}
impl FabricAppearance {
    pub fn validate(&self) -> Result<(), String> {
        if !self.roughness.is_finite() || !(0.0..=1.0).contains(&self.roughness) {
            return Err("Optical roughness must be finite and between 0 and 1".into());
        }
        if let Some(w) = &self.weave
            && (!(0.05..=20.0).contains(&w.spacing_mm) || !(0.0..=1.0).contains(&w.contrast))
        {
            return Err(
                "A weave needs a thread spacing of 0.05 to 20 mm and a contrast of 0 to 1".into(),
            );
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
