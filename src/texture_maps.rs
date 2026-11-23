//! Fabric texture maps as exported files.
//!
//! Exchange formats read maps their own way: glTF wants OpenGL normal maps, roughness in the
//! green channel and metalness in the blue channel of one image, and opacity in the colour
//! image's alpha; OBJ and FBX readers expect plain grey images. A fabric keeps its maps as the
//! texture tool exported them (a DirectX normal map, a glossiness map) and says so, so the
//! exporters ask here for the file each format needs: the original when it fits, a converted
//! PNG otherwise.
use crate::garment::appearance::{FabricAppearance, FabricTexture, MapSlot};
use std::path::{Path, PathBuf};

/// An image to write beside an export or into it.
#[derive(Clone, Debug, PartialEq)]
pub enum TextureFile {
    /// The original file, as it is.
    Original(PathBuf),
    /// A PNG made for the export; `tag` names what was done ("gl", "rough"), for its file name.
    Converted { tag: &'static str, png: Vec<u8> },
}

impl TextureFile {
    /// The file's bytes.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Original(path) => {
                std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
            }
            Self::Converted { png, .. } => Ok(png.clone()),
        }
    }
    /// The extension the file should keep, lower case.
    pub fn extension(&self) -> String {
        match self {
            Self::Original(path) => path
                .extension()
                .map_or("png".into(), |e| e.to_string_lossy().to_ascii_lowercase()),
            Self::Converted { .. } => "png".into(),
        }
    }
    /// A file name for it beside an export: `{stem}_{asset}` and the conversion's tag.
    pub fn name(&self, stem: &str, asset: &str) -> String {
        match self {
            Self::Original(_) => format!("{stem}_{asset}.{}", self.extension()),
            Self::Converted { tag, .. } => format!("{stem}_{asset}_{tag}.png"),
        }
    }
    /// Write it to `path`.
    pub fn write(&self, path: &Path) -> Result<(), String> {
        match self {
            Self::Original(source) => std::fs::copy(source, path)
                .map(|_| ())
                .map_err(|e| format!("{}: {e}", source.display())),
            Self::Converted { png, .. } => {
                std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))
            }
        }
    }
    /// Whether it is a PNG or a JPEG, the formats glTF and USDZ accept.
    pub fn portable(&self) -> bool {
        matches!(self.extension().as_str(), "png" | "jpg" | "jpeg")
    }
}

/// The file of an image asset.
pub fn source<'a>(images: &'a [(String, PathBuf)], asset: &str) -> Result<&'a Path, String> {
    images
        .iter()
        .find(|(id, _)| id == asset)
        .map(|(_, p)| p.as_path())
        .ok_or_else(|| format!("Texture image {asset} is missing"))
}

/// Decoded RGBA8 pixels of an image file.
pub fn load(path: &Path) -> Result<image::RgbaImage, String> {
    Ok(image::open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .to_rgba8())
}

fn png(image: &image::RgbaImage) -> Vec<u8> {
    crate::bake::png(image.width(), image.height(), image.as_raw())
}

/// The file as it is when `portable` is not asked or it is already a PNG or JPEG; otherwise
/// re-encoded as a PNG (a TGA or BMP export from a texture tool).
pub fn plain(path: &Path, portable: bool) -> Result<TextureFile, String> {
    let original = TextureFile::Original(path.to_path_buf());
    if !portable || original.portable() {
        return Ok(original);
    }
    Ok(TextureFile::Converted {
        tag: "png",
        png: png(&load(path)?),
    })
}

/// Flip the green channel of normal-map pixels (DirectX ↔ OpenGL).
pub fn flip_green(image: &mut image::RgbaImage) {
    for p in image.pixels_mut() {
        p.0[1] = 255 - p.0[1];
    }
}

/// Invert the colour channels of grey-map pixels (glossiness ↔ roughness).
pub fn invert(image: &mut image::RgbaImage) {
    for p in image.pixels_mut() {
        for c in &mut p.0[..3] {
            *c = 255 - *c;
        }
    }
}

/// The texture's normal map drawn the OpenGL way (glTF's, Blender's, and what OBJ and FBX
/// readers assume).
pub fn opengl_normal(
    texture: &FabricTexture,
    images: &[(String, PathBuf)],
    portable: bool,
) -> Result<Option<TextureFile>, String> {
    let Some(asset) = texture.normal_asset_id.as_deref() else {
        return Ok(None);
    };
    let path = source(images, asset)?;
    if !texture.normal_directx {
        return plain(path, portable).map(Some);
    }
    let mut image = load(path)?;
    flip_green(&mut image);
    Ok(Some(TextureFile::Converted {
        tag: "normal_gl",
        png: png(&image),
    }))
}

/// The texture's roughness map as roughness (black glossy) or, with `glossiness`, as
/// glossiness (black matt), converting when the file holds the other one.
pub fn grey_roughness(
    texture: &FabricTexture,
    images: &[(String, PathBuf)],
    glossiness: bool,
    portable: bool,
) -> Result<Option<TextureFile>, String> {
    let Some(asset) = texture.roughness_asset_id.as_deref() else {
        return Ok(None);
    };
    let path = source(images, asset)?;
    if texture.roughness_is_glossiness == glossiness {
        return plain(path, portable).map(Some);
    }
    let mut image = load(path)?;
    invert(&mut image);
    Ok(Some(TextureFile::Converted {
        tag: if glossiness { "gloss" } else { "rough" },
        png: png(&image),
    }))
}

/// A grey map of the texture as it is (metalness, opacity, displacement, occlusion).
pub fn grey(
    texture: &FabricTexture,
    slot: MapSlot,
    images: &[(String, PathBuf)],
    portable: bool,
) -> Result<Option<TextureFile>, String> {
    texture
        .map(slot)
        .map(|asset| plain(source(images, asset)?, portable))
        .transpose()
}

/// glTF's metallic-roughness image (green roughness, blue metalness), when the texture has a
/// roughness or a metalness map: the fabric's single value fills the channel without one.
/// It is as large as the larger map; the other is stretched to it.
pub fn gltf_metallic_roughness(
    appearance: &FabricAppearance,
    images: &[(String, PathBuf)],
) -> Result<Option<TextureFile>, String> {
    let Some(texture) = appearance.texture.as_ref() else {
        return Ok(None);
    };
    let read = |asset: Option<&str>| -> Result<Option<image::RgbaImage>, String> {
        asset.map(|a| load(source(images, a)?)).transpose()
    };
    let roughness = read(texture.roughness_asset_id.as_deref())?;
    let metalness = read(texture.metalness_asset_id.as_deref())?;
    if roughness.is_none() && metalness.is_none() {
        return Ok(None);
    }
    let (w, h) = [&roughness, &metalness]
        .into_iter()
        .flatten()
        .map(|i| i.dimensions())
        .fold((1, 1), |a, b| (a.0.max(b.0), a.1.max(b.1)));
    let fit = |image: Option<image::RgbaImage>| {
        image.map(|i| {
            if i.dimensions() == (w, h) {
                i
            } else {
                image::imageops::resize(&i, w, h, image::imageops::FilterType::Triangle)
            }
        })
    };
    let (roughness, metalness) = (fit(roughness), fit(metalness));
    let constant = |v: f64| (v.clamp(0., 1.) * 255.).round() as u8;
    let mut out = image::RgbaImage::new(w, h);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let r = roughness
            .as_ref()
            .map_or(constant(appearance.roughness), |i| {
                let v = i.get_pixel(x, y).0[0];
                if texture.roughness_is_glossiness {
                    255 - v
                } else {
                    v
                }
            });
        let m = metalness
            .as_ref()
            .map_or(constant(appearance.metalness), |i| i.get_pixel(x, y).0[0]);
        *p = image::Rgba([255, r, m, 255]);
    }
    Ok(Some(TextureFile::Converted {
        tag: "metal_rough",
        png: png(&out),
    }))
}

/// The opacity of an opacity-map pixel: its grey times its own alpha.
pub fn opacity_of(pixel: image::Rgba<u8>) -> u8 {
    ((u16::from(pixel.0[0]) * u16::from(pixel.0[3]) + 127) / 255) as u8
}

/// The texture's colour image with the opacity map in its alpha, for formats that read
/// opacity there (glTF); none when the texture has no opacity map.
pub fn color_with_opacity(
    texture: &FabricTexture,
    images: &[(String, PathBuf)],
) -> Result<Option<TextureFile>, String> {
    let Some(asset) = texture.opacity_asset_id.as_deref() else {
        return Ok(None);
    };
    let mut color = load(source(images, &texture.asset_id)?)?;
    let mut opacity = load(source(images, asset)?)?;
    if opacity.dimensions() != color.dimensions() {
        opacity = image::imageops::resize(
            &opacity,
            color.width(),
            color.height(),
            image::imageops::FilterType::Triangle,
        );
    }
    for (c, o) in color.pixels_mut().zip(opacity.pixels()) {
        c.0[3] = ((u16::from(c.0[3]) * u16::from(opacity_of(*o)) + 127) / 255) as u8;
    }
    Ok(Some(TextureFile::Converted {
        tag: "color_alpha",
        png: png(&color),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_and_inversions_undo_themselves() {
        let mut image = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([x as u8 * 40, y as u8 * 90 + 7, 200, 255])
        });
        let original = image.clone();
        flip_green(&mut image);
        assert_eq!(image.get_pixel(0, 1).0, [0, 255 - 97, 200, 255]);
        flip_green(&mut image);
        invert(&mut image);
        assert_eq!(image.get_pixel(2, 0).0, [255 - 80, 248, 55, 255]);
        invert(&mut image);
        assert_eq!(image, original);
    }

    #[test]
    fn opacity_counts_grey_and_alpha() {
        assert_eq!(opacity_of(image::Rgba([255, 255, 255, 255])), 255);
        assert_eq!(opacity_of(image::Rgba([255, 0, 0, 0])), 0);
        assert_eq!(opacity_of(image::Rgba([128, 128, 128, 255])), 128);
    }
}
