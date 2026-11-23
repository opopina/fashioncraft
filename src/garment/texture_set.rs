//! Recognising a PBR texture set from its file names.
//!
//! Material authoring tools (Substance Painter, Designer and Sampler among them, and the
//! texture libraries that follow the same habits) export one image per map, named after the
//! material with a suffix that says which map it is: `Denim_BaseColor.png`,
//! `denim_nor_gl_2k.jpg`, `Fabric062_1K-JPG_Roughness.jpg`. This module reads those names
//! only; it never opens the images. Substance's own `.sbsar` archives are not read: they need
//! Adobe's proprietary engine, so the exported maps are the supported route.
use std::path::{Path, PathBuf};

/// Which map an image is, as its name says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MapKind {
    BaseColor,
    Normal,
    Roughness,
    /// Glossiness (or smoothness): roughness inverted.
    Glossiness,
    Metalness,
    Height,
    Opacity,
    Occlusion,
}

/// How a normal map's green channel points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NormalConvention {
    /// Green up the image (OpenGL, glTF, Blender): what the renderer reads.
    OpenGl,
    /// Green down the image (DirectX, Unreal): its green channel must be flipped.
    DirectX,
}

/// One recognised file name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recognised {
    pub kind: MapKind,
    /// The convention the name states, for a normal map that says it.
    pub convention: Option<NormalConvention>,
    /// The material the file belongs to: the name before the map's suffix, lower case, with
    /// resolution and format tokens left out.
    pub material: String,
    /// How sure the name is, lower first: a full name ("roughness") beats a short one ("rgh").
    rank: u8,
}

/// Image formats the studio reads. EXR and TIFF exports are reported as unsupported.
pub const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "tga", "bmp"];

/// The names of each map, as whole words or joined words, and their rank (lower is surer).
const NAMES: &[(&str, MapKind, Option<NormalConvention>, u8)] = {
    use MapKind::*;
    use NormalConvention::*;
    &[
        ("basecolor", BaseColor, None, 0),
        ("basecolour", BaseColor, None, 0),
        ("albedo", BaseColor, None, 0),
        ("albedotransparency", BaseColor, None, 1),
        ("diffuse", BaseColor, None, 1),
        ("diff", BaseColor, None, 2),
        ("color", BaseColor, None, 2),
        ("colour", BaseColor, None, 2),
        ("col", BaseColor, None, 3),
        ("normal", Normal, None, 0),
        ("normals", Normal, None, 0),
        ("normalmap", Normal, None, 0),
        ("nrm", Normal, None, 2),
        ("nor", Normal, None, 2),
        ("norm", Normal, None, 2),
        ("normalgl", Normal, Some(OpenGl), 0),
        ("normalopengl", Normal, Some(OpenGl), 0),
        ("normalogl", Normal, Some(OpenGl), 0),
        ("norgl", Normal, Some(OpenGl), 0),
        ("nrmgl", Normal, Some(OpenGl), 0),
        ("normaldx", Normal, Some(DirectX), 1),
        ("normaldirectx", Normal, Some(DirectX), 1),
        ("nordx", Normal, Some(DirectX), 1),
        ("nrmdx", Normal, Some(DirectX), 1),
        ("roughness", Roughness, None, 0),
        ("rough", Roughness, None, 1),
        ("rgh", Roughness, None, 2),
        ("glossiness", Glossiness, None, 0),
        ("gloss", Glossiness, None, 1),
        ("smoothness", Glossiness, None, 1),
        ("metallic", Metalness, None, 0),
        ("metalness", Metalness, None, 0),
        ("metal", Metalness, None, 1),
        ("met", Metalness, None, 2),
        ("height", Height, None, 0),
        ("heightmap", Height, None, 0),
        ("displacement", Height, None, 0),
        ("displace", Height, None, 1),
        ("disp", Height, None, 1),
        ("bump", Height, None, 2),
        ("opacity", Opacity, None, 0),
        ("alpha", Opacity, None, 1),
        ("transparency", Opacity, None, 1),
        ("ambientocclusion", Occlusion, None, 0),
        ("occlusion", Occlusion, None, 0),
        ("mixedao", Occlusion, None, 0),
        ("ao", Occlusion, None, 1),
        ("occ", Occlusion, None, 2),
    ]
};

/// Names of packed maps (several greys in one image's channels), which are not split: their
/// last word alone ("metallic") must not pass them off as a single map.
const PACKED: &[&str] = &[
    "occlusionroughnessmetallic",
    "roughnessmetallic",
    "metallicroughness",
    "metallicsmoothness",
    "aoroughnessmetallic",
    "maskmap",
    "arm",
    "orm",
    "rma",
    "mra",
];

/// Words that say nothing about the map: resolutions ("2k", "4096"), formats and "map".
fn ignorable(token: &str) -> bool {
    let digits = token.trim_end_matches(['k', 'K']);
    (!token.is_empty() && token.bytes().all(|b| b.is_ascii_digit()))
        || (!digits.is_empty()
            && digits.len() < token.len()
            && digits.bytes().all(|b| b.is_ascii_digit()))
        || matches!(
            token,
            "map"
                | "tex"
                | "texture"
                | "png"
                | "jpg"
                | "jpeg"
                | "tga"
                | "tif"
                | "tiff"
                | "exr"
                | "bmp"
                | "hires"
                | "lowres"
                | "srgb"
                | "linear"
                | "raw"
        )
}

/// Lower-case words of a file stem, split at separators and at case changes ("BaseColor" →
/// "base", "color"; "NormalGL" → "normal", "gl").
fn tokens(stem: &str) -> Vec<String> {
    let mut out = vec![];
    for part in stem.split(|c: char| !c.is_ascii_alphanumeric()) {
        let chars: Vec<char> = part.chars().collect();
        let mut word = String::new();
        for (k, &c) in chars.iter().enumerate() {
            let prev = k.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(k + 1).copied();
            let boundary = match prev {
                Some(p) if c.is_ascii_uppercase() => {
                    p.is_ascii_lowercase()
                        || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
                }
                _ => false,
            };
            if boundary && !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            word.push(c.to_ascii_lowercase());
        }
        if !word.is_empty() {
            out.push(word);
        }
    }
    out
}

/// Which map a file is, from its name alone; none when the name says no map we draw (an
/// emissive or specular map, a packed mask) or the file is not an image the studio reads.
pub fn recognise(path: &Path) -> Option<Recognised> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }
    let words = tokens(path.file_stem()?.to_str()?);
    let mut end = words.len();
    while end > 0 && ignorable(&words[end - 1]) {
        end -= 1;
    }
    // The longest run of final words that names a map: "nor"+"gl" before "gl".
    for n in (1..=3.min(end)).rev() {
        let joined: String = words[end - n..end].concat();
        if PACKED.contains(&joined.as_str()) {
            return None;
        }
        if let Some(&(_, kind, convention, rank)) = NAMES.iter().find(|(name, ..)| *name == joined)
        {
            let material = words[..end - n]
                .iter()
                .filter(|w| !ignorable(w))
                .cloned()
                .collect::<Vec<_>>()
                .join("_");
            return Some(Recognised {
                kind,
                convention,
                material,
                rank,
            });
        }
    }
    None
}

/// The maps of one material, as files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureSet {
    /// The material's name as the files give it ("denim"); empty when they only name maps.
    pub name: String,
    pub base_color: Option<PathBuf>,
    pub normal: Option<PathBuf>,
    /// How the normal map is drawn; OpenGL unless its name says DirectX.
    pub normal_convention: Option<NormalConvention>,
    pub roughness: Option<PathBuf>,
    /// The roughness file is a glossiness map, to be inverted.
    pub roughness_is_glossiness: bool,
    pub metalness: Option<PathBuf>,
    pub height: Option<PathBuf>,
    pub opacity: Option<PathBuf>,
    pub occlusion: Option<PathBuf>,
}

impl TextureSet {
    /// How many maps the set has.
    pub fn len(&self) -> usize {
        [
            &self.base_color,
            &self.normal,
            &self.roughness,
            &self.metalness,
            &self.height,
            &self.opacity,
            &self.occlusion,
        ]
        .iter()
        .filter(|m| m.is_some())
        .count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Whether the normal map must have its green channel flipped.
    pub fn normal_directx(&self) -> bool {
        self.normal_convention == Some(NormalConvention::DirectX)
    }
}

/// What a folder or a pick of files holds: the texture sets found, one per material, and the
/// files no set took (other maps, other formats, duplicates).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recognition {
    pub sets: Vec<TextureSet>,
    pub ignored: Vec<PathBuf>,
}

/// Sort files into texture sets by the material their names give. Where a set has two files
/// for one map, the surer name wins (roughness over glossiness, an OpenGL normal map over a
/// DirectX one, height over bump), then the first in name order.
pub fn group(paths: &[PathBuf]) -> Recognition {
    let mut paths: Vec<&PathBuf> = paths.iter().collect();
    paths.sort();
    let mut found: Vec<(String, Vec<(Recognised, PathBuf)>)> = vec![];
    let mut ignored = vec![];
    for path in paths {
        match recognise(path) {
            Some(r) => match found.iter_mut().find(|(m, _)| *m == r.material) {
                Some((_, files)) => files.push((r, path.clone())),
                None => found.push((r.material.clone(), vec![(r, path.clone())])),
            },
            None => ignored.push(path.clone()),
        }
    }
    let mut sets = vec![];
    for (material, mut files) in found {
        // Roughness and glossiness fill the same slot: roughness first.
        files.sort_by_key(|(r, _)| {
            let slot = match r.kind {
                MapKind::Glossiness => MapKind::Roughness,
                k => k,
            };
            (slot, r.kind == MapKind::Glossiness, r.rank)
        });
        let mut set = TextureSet {
            name: material,
            ..Default::default()
        };
        for (r, path) in files {
            let slot = match r.kind {
                MapKind::BaseColor => &mut set.base_color,
                MapKind::Normal => &mut set.normal,
                MapKind::Roughness | MapKind::Glossiness => &mut set.roughness,
                MapKind::Metalness => &mut set.metalness,
                MapKind::Height => &mut set.height,
                MapKind::Opacity => &mut set.opacity,
                MapKind::Occlusion => &mut set.occlusion,
            };
            if slot.is_some() {
                ignored.push(path);
                continue;
            }
            *slot = Some(path);
            match r.kind {
                MapKind::Normal => {
                    set.normal_convention = Some(r.convention.unwrap_or(NormalConvention::OpenGl))
                }
                MapKind::Glossiness => set.roughness_is_glossiness = true,
                _ => {}
            }
        }
        sets.push(set);
    }
    // Sets with more maps first: the material a folder is about, before stray files.
    sets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.name.cmp(&b.name)));
    ignored.sort();
    Recognition { sets, ignored }
}

/// The image files directly inside `folder`, in name order.
pub fn folder_files(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = vec![];
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Whether `path` is a Substance archive, which the studio cannot open.
pub fn is_substance_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sbsar") || e.eq_ignore_ascii_case("sbs"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_at_separators_and_case_changes() {
        assert_eq!(tokens("Denim_BaseColor"), ["denim", "base", "color"]);
        assert_eq!(
            tokens("Fabric062_1K-JPG_NormalGL"),
            ["fabric062", "1k", "jpg", "normal", "gl"]
        );
        assert_eq!(tokens("lace_AOMap"), ["lace", "ao", "map"]);
        assert_eq!(tokens("Normal_OpenGL"), ["normal", "open", "gl"]);
    }

    #[test]
    fn resolutions_and_formats_are_not_material_names() {
        for t in ["2k", "4K", "1024", "png", "map"] {
            assert!(ignorable(t), "{t}");
        }
        for t in ["k", "denim", "ao", "gl"] {
            assert!(!ignorable(t), "{t}");
        }
    }
}
