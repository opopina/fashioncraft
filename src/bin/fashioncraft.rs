//! FashionCraft, as an installed app starts it: the studio, opened on the bundled mannequin
//! with a T-shirt, or on a project passed to it (a double-clicked file). Its work (session files,
//! preferences, recovery) goes to the user's data folder. `cloth-fit` stays the tool for headless
//! runs and checks.
//!
//! The bundled assets (the mannequin and the files that sit beside it) are looked for next to
//! the program: in `Contents/Resources/assets` of a macOS app bundle, in `assets/` beside the
//! program on Windows and in portable folders, and in `share/fashioncraft/assets` under a Unix
//! prefix (`bin/` and `share/`). `FASHIONCRAFT_ASSETS` overrides them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use fashioncraft::{fitting::FittingProject, garment::presets::GarmentKind, vbd::VbdSettings};
use std::path::{Path, PathBuf};

#[path = "../atelier.rs"]
mod fitting_editor;
use fashioncraft::render;

/// The mannequin, relative to the assets folder.
const MANNEQUIN: &str = "makehuman/mannequin.obj";

/// The folder holding the bundled assets.
fn assets_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FASHIONCRAFT_ASSETS") {
        return Some(dir.into());
    }
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    // Last, a build run from the source tree.
    assets_beside(&exe).or_else(|| {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        dev.join(MANNEQUIN).is_file().then_some(dev)
    })
}

/// The assets installed with the program at `exe`: in a macOS app bundle, beside it (Windows,
/// portable folders) or under its Unix prefix.
fn assets_beside(exe: &Path) -> Option<PathBuf> {
    let beside = exe.parent()?;
    [
        beside.join("../Resources/assets"),
        beside.join("assets"),
        beside.join("../share/fashioncraft/assets"),
    ]
    .into_iter()
    .find(|dir| dir.join(MANNEQUIN).is_file())
}

/// Where FashionCraft keeps its work: the platform's folder for application data.
fn data_dir() -> PathBuf {
    let base: Option<PathBuf> = if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/FashionCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("FashionCraft"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(|dir| PathBuf::from(dir).join("fashioncraft"))
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local/share/fashioncraft"))
            })
    };
    base.unwrap_or_else(|| PathBuf::from("FashionCraft"))
        .join("session")
}

fn start() -> Result<(), String> {
    let assets = assets_dir().ok_or(
        "FashionCraft could not find its mannequin beside the program. Reinstall FashionCraft.",
    )?;
    let output = data_dir();
    std::fs::create_dir_all(&output).map_err(|e| format!("{}: {e}", output.display()))?;
    // A project given on the command line, as a double-clicked file is.
    let source = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    let mut project = match &source {
        Some(path) => FittingProject::load(path)?,
        None => FittingProject::basic(GarmentKind::Tshirt, 2, assets.join(MANNEQUIN)),
    };
    project.simulation.get_or_insert_with(|| VbdSettings {
        dt: 1. / 60.,
        ..Default::default()
    });
    let scene = project.compile()?;
    fitting_editor::run(
        project,
        scene,
        fitting_editor::DocumentFiles {
            output,
            source,
            modified: false,
        },
        false,
        false,
        false,
        false,
    )
}

fn main() {
    if let Err(error) = start() {
        eprintln!("FashionCraft: {error}");
        rfd::MessageDialog::new()
            .set_title("FashionCraft")
            .set_description(&error)
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each installed layout finds its own mannequin and no other.
    #[test]
    fn finds_the_assets_of_each_layout() {
        let root = tempfile::tempdir().unwrap();
        let lay = |exe: &str, assets: &str| {
            let exe = root.path().join(exe);
            std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
            std::fs::write(&exe, b"").unwrap();
            let mannequin = root.path().join(assets).join(MANNEQUIN);
            std::fs::create_dir_all(mannequin.parent().unwrap()).unwrap();
            std::fs::write(&mannequin, b"v 0 0 0").unwrap();
            exe
        };
        for (exe, assets) in [
            (
                "mac/FashionCraft.app/Contents/MacOS/FashionCraft",
                "mac/FashionCraft.app/Contents/Resources/assets",
            ),
            (
                "windows/FashionCraft/fashioncraft.exe",
                "windows/FashionCraft/assets",
            ),
            (
                "prefix/bin/fashioncraft",
                "prefix/share/fashioncraft/assets",
            ),
        ] {
            let exe = lay(exe, assets);
            let found = assets_beside(&exe).unwrap();
            assert_eq!(
                found.join(MANNEQUIN).canonicalize().unwrap(),
                root.path()
                    .join(assets)
                    .join(MANNEQUIN)
                    .canonicalize()
                    .unwrap()
            );
        }
        let lone = root.path().join("elsewhere/fashioncraft");
        std::fs::create_dir_all(lone.parent().unwrap()).unwrap();
        assert!(assets_beside(&lone).is_none());
    }
}
