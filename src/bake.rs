//! Baking a garment's look into one image: every piece laid out in the unit square of texture
//! space (shelf by shelf, all at one scale, slightly apart) and painted texel by texel with
//! what the cloth shows there — its fabric's colour times its texture image (along the
//! fabric's grain, repeated) and, over that, the front prints in their order, by their alpha
//! and opacity. Islands are grown a few texels so filtering never reaches the background.
//! The procedural weave and the lighting are not baked: they come from the renderer.
use crate::garment::{CompiledGarment, GarmentTemplate, graphics::GraphicSide};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

/// Decoded straight-alpha RGBA8 pixels in sRGB, rows from the top.
#[derive(Clone, Copy)]
pub struct Image<'a> {
    pub size: [u32; 2],
    pub rgba: &'a [u8],
}

/// The baked image (sRGB RGBA8, rows from the top) and each garment vertex's coordinates in
/// it, (0, 0) at its top-left corner.
pub struct Atlas {
    pub size: u32,
    pub rgba: Vec<u8>,
    pub uv: Vec<DVec2>,
}

/// Texels the islands are grown by.
const BLEED: usize = 4;

/// Each box moved into one unit square, shelf by shelf, all at the same scale and slightly
/// apart: its offset and the common scale.
pub(crate) fn pack(boxes: &[(DVec2, DVec2)]) -> Vec<(DVec2, f64)> {
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

fn to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f64) -> u8 {
    let c = c.clamp(0., 1.);
    let s = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    };
    (s * 255.).round() as u8
}

/// Bilinear sample of `image` at `uv` (one image spans [0,1], rows from the top), wrapped or
/// clamped: linear RGB and alpha.
fn sample(image: Image, uv: DVec2, wrap: bool) -> (DVec3, f64) {
    let [w, h] = image.size.map(|v| v as i64);
    let x = uv.x * w as f64 - 0.5;
    let y = uv.y * h as f64 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let texel = |i: i64, j: i64| {
        let (i, j) = if wrap {
            (i.rem_euclid(w), j.rem_euclid(h))
        } else {
            (i.clamp(0, w - 1), j.clamp(0, h - 1))
        };
        let k = ((j * w + i) * 4) as usize;
        let p = &image.rgba[k..k + 4];
        (
            DVec3::new(to_linear(p[0]), to_linear(p[1]), to_linear(p[2])),
            p[3] as f64 / 255.,
        )
    };
    let (i, j) = (x0 as i64, y0 as i64);
    let mix = |a: (DVec3, f64), b: (DVec3, f64), t: f64| (a.0.lerp(b.0, t), a.1 + (b.1 - a.1) * t);
    let top = mix(texel(i, j), texel(i + 1, j), fx);
    let bottom = mix(texel(i, j + 1), texel(i + 1, j + 1), fx);
    mix(top, bottom, fy)
}

/// Bake `template`'s look on `garment` into a `size` × `size` image. `images` holds the
/// decoded fabric textures and print images by asset id; missing ones are left out.
pub fn bake(
    template: &GarmentTemplate,
    garment: &CompiledGarment,
    images: &HashMap<String, Image>,
    size: u32,
) -> Result<Atlas, String> {
    if !(16..=8192).contains(&size) {
        return Err("Bake into an image of 16 to 8192 pixels".into());
    }
    for image in images.values() {
        if image.size[0] == 0
            || image.size[1] == 0
            || image.rgba.len() != (image.size[0] * image.size[1] * 4) as usize
        {
            return Err("An image to bake has no pixels or the wrong number of them".into());
        }
    }
    let uv = &garment.mesh.uv;
    let boxes: Vec<(DVec2, DVec2)> = garment
        .panels
        .iter()
        .map(|p| {
            p.vertices.clone().fold(
                (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                |(lo, hi), v| (lo.min(uv[v]), hi.max(uv[v])),
            )
        })
        .collect();
    let packed = pack(&boxes);
    let mut atlas_uv = vec![DVec2::ZERO; uv.len()];
    for (panel, (offset, scale)) in garment.panels.iter().zip(&packed) {
        for v in panel.vertices.clone() {
            atlas_uv[v] = (uv[v] + *offset) * *scale;
        }
    }
    let appearances = crate::garment::appearance::bind(template, garment);
    let n = size as usize;
    let mut color = vec![DVec3::ZERO; n * n];
    let mut covered = vec![false; n * n];
    for panel in &garment.panels {
        let appearance = appearances[panel.material].as_ref();
        let base = appearance.map_or(DVec3::splat(0.82), |a| {
            DVec3::from_array(a.linear_color().map(f64::from))
        });
        let grain = garment.materials[panel.material].grain_degrees;
        let (sin, cos) = grain.to_radians().sin_cos();
        let texture = appearance
            .and_then(|a| a.texture.as_ref())
            .and_then(|t| Some((t, *images.get(&t.asset_id)?)));
        let prints: Vec<_> = template
            .graphics
            .iter()
            .filter(|g| g.panel_id == panel.id && g.side == GraphicSide::Front)
            .filter_map(|g| Some((g, *images.get(&g.asset_id)?)))
            .collect();
        let shift = template
            .texture_offsets
            .get(&panel.id)
            .copied()
            .unwrap_or_default();
        let paint = |p: DVec2| {
            let mut c = base;
            if let Some((t, image)) = texture {
                let q = p - shift;
                let warp_weft = DVec2::new(q.x * cos + q.y * sin, -q.x * sin + q.y * cos);
                c *= sample(image, t.image_uv(warp_weft), true).0;
            }
            for (g, image) in &prints {
                if let Ok(at) = g.material_to_uv(p)
                    && (0.0..=1.0).contains(&at.x)
                    && (0.0..=1.0).contains(&at.y)
                {
                    let (rgb, a) = sample(*image, at, false);
                    c = c.lerp(rgb, a * g.opacity);
                }
            }
            c
        };
        for t in &garment.mesh.triangles[panel.triangles.clone()] {
            let corners = t.vertices.map(|v| atlas_uv[v] * size as f64);
            let material = t.vertices.map(|v| uv[v]);
            let lo = corners[0]
                .min(corners[1])
                .min(corners[2])
                .floor()
                .max(DVec2::ZERO);
            let hi = corners[0]
                .max(corners[1])
                .max(corners[2])
                .ceil()
                .min(DVec2::splat(size as f64));
            let [a, b, c] = corners;
            let area = (b - a).perp_dot(c - a);
            if area.abs() < 1e-18 {
                continue;
            }
            for j in lo.y as usize..hi.y as usize {
                for i in lo.x as usize..hi.x as usize {
                    let q = DVec2::new(i as f64 + 0.5, j as f64 + 0.5);
                    let w0 = (b - q).perp_dot(c - q) / area;
                    let w1 = (c - q).perp_dot(a - q) / area;
                    let w2 = 1. - w0 - w1;
                    if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                        continue;
                    }
                    let p = material[0] * w0 + material[1] * w1 + material[2] * w2;
                    color[j * n + i] = paint(p);
                    covered[j * n + i] = true;
                }
            }
        }
    }
    // Grow the islands, so filtering at their edges never mixes in the background.
    for _ in 0..BLEED {
        let before = covered.clone();
        for j in 0..n {
            for i in 0..n {
                if before[j * n + i] {
                    continue;
                }
                let (mut sum, mut count) = (DVec3::ZERO, 0.);
                for (di, dj) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (x, y) = (i as i64 + di, j as i64 + dj);
                    if (0..n as i64).contains(&x)
                        && (0..n as i64).contains(&y)
                        && before[y as usize * n + x as usize]
                    {
                        sum += color[y as usize * n + x as usize];
                        count += 1.;
                    }
                }
                if count > 0. {
                    color[j * n + i] = sum / count;
                    covered[j * n + i] = true;
                }
            }
        }
    }
    let mut rgba = vec![0u8; n * n * 4];
    for k in 0..n * n {
        if covered[k] {
            let c = color[k];
            rgba[k * 4..k * 4 + 4].copy_from_slice(&[
                to_srgb(c.x),
                to_srgb(c.y),
                to_srgb(c.z),
                255,
            ]);
        }
    }
    Ok(Atlas {
        size,
        rgba,
        uv: atlas_uv,
    })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG file of straight-alpha RGBA8 pixels, rows from the top.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8 bits per channel, RGBA, deflate, adaptive filtering, no interlace.
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks_exact(row) {
        // Filter type 0: each row as it is.
        raw.push(0);
        raw.extend_from_slice(line);
    }
    chunk(
        &mut out,
        b"IDAT",
        &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6),
    );
    chunk(&mut out, b"IEND", &[]);
    out
}
