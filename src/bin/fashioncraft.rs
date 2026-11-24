//! FashionCraft, as an installed app starts it: the studio, opened on an empty project (the
//! bundled mannequin alone) under the Home screen, or on a project passed to it (a
//! double-clicked file). A scripted demo (`FASHIONCRAFT_DEMO`) starts on the T-shirt. Its work (session files,
//! preferences, recovery) goes to the user's data folder. `cloth-fit` stays the tool for headless
//! runs and checks.
//!
//! The bundled assets (the mannequin and the files that sit beside it) are looked for next to
//! the program: in `Contents/Resources/assets` of a macOS app bundle, in `assets/` beside the
//! program on Windows and in portable folders, and in `share/fashioncraft/assets` under a Unix
//! prefix (`bin/` and `share/`). `FASHIONCRAFT_ASSETS` overrides them.
//!
//! `--version` and `--help` print and quit; any other option is refused, never taken for a
//! project to open.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use fashioncraft::{fitting::FittingProject, garment::presets::GarmentKind, vbd::VbdSettings};
use std::ffi::OsString;
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

/// What the command line asks FashionCraft to do.
#[derive(Debug, PartialEq)]
enum Request {
    /// Open the studio, on this project when one is given (a double-clicked file).
    Open(Option<PathBuf>),
    /// Print the version and quit.
    Version,
    /// Print how to start FashionCraft and quit.
    Help,
    /// An option FashionCraft does not have.
    Unknown(OsString),
}

/// Reads the command line, the program's name first. Only the first argument counts. Old macOS
/// versions pass a `-psn_…` process serial number first; it is neither a project nor an option.
fn request(args: impl IntoIterator<Item = OsString>) -> Request {
    let Some(first) = args.into_iter().nth(1) else {
        return Request::Open(None);
    };
    let text = first.to_string_lossy().into_owned();
    match text.as_str() {
        _ if text.starts_with("-psn_") => Request::Open(None),
        "-V" | "--version" => Request::Version,
        "-h" | "--help" => Request::Help,
        _ if text.starts_with('-') => Request::Unknown(first),
        _ => Request::Open(Some(PathBuf::from(first))),
    }
}

/// How to start FashionCraft, for `--help`.
fn usage() -> String {
    format!(
        "FashionCraft {}\n\n\
         Usage: fashioncraft [PROJECT]\n\n\
         Opens the studio on PROJECT, or on the Home screen when none is given.\n\n\
         Options:\n  \
         -h, --help     Print this help\n  \
         -V, --version  Print the version",
        env!("CARGO_PKG_VERSION")
    )
}

/// Why a project given to open cannot be, in the interface's language.
fn unreadable(path: &Path, output: &Path) -> String {
    let spanish = std::fs::read_to_string(output.join("preferences.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|prefs| prefs["language"] == "spanish");
    unreadable_message(path, spanish, cfg!(target_os = "macos"))
}

/// The message of [`unreadable`]. Only macOS keeps an app out of a cloud folder until it is
/// given access in System Settings, so only there does the message give that advice.
fn unreadable_message(path: &Path, spanish: bool, macos: bool) -> String {
    let path = path.display();
    match (spanish, macos) {
        (true, true) => format!(
            "FashionCraft no puede leer {path}.\n\nSi está en una carpeta de la nube (Nextcloud, iCloud, Dropbox…), \
             dale acceso en Ajustes del Sistema › Privacidad y seguridad, o ábrelo desde Archivo › Abrir."
        ),
        (true, false) => format!(
            "FashionCraft no puede leer {path}.\n\nComprueba que el archivo existe, o ábrelo desde Archivo › Abrir."
        ),
        (false, true) => format!(
            "FashionCraft cannot read {path}.\n\nIf it is in a cloud folder (Nextcloud, iCloud, Dropbox…), \
             give FashionCraft access in System Settings › Privacy & Security, or open it with File › Open."
        ),
        (false, false) => format!(
            "FashionCraft cannot read {path}.\n\nCheck that the file exists, or open it with File › Open."
        ),
    }
}

fn start(source: Option<PathBuf>) -> Result<(), String> {
    let assets = assets_dir().ok_or(
        "FashionCraft could not find its mannequin beside the program. Reinstall FashionCraft.",
    )?;
    let output = data_dir();
    std::fs::create_dir_all(&output).map_err(|e| format!("{}: {e}", output.display()))?;
    // A project given on the command line, as a double-clicked file is. One that cannot be read
    // is an error, not a reason to open the T-shirt in its place.
    let source = match source {
        Some(path) if path.is_file() => Some(path),
        Some(path) => return Err(unreadable(&path, &output)),
        None => None,
    };
    // A demo's script starts from the T-shirt; anyone else from an empty project, with the
    // Home screen over it to pick what to begin with.
    let demo = std::env::var_os("FASHIONCRAFT_DEMO").is_some();
    let mut project = match &source {
        Some(path) => FittingProject::load(path)?,
        None if demo => FittingProject::basic(GarmentKind::Tshirt, 2, assets.join(MANNEQUIN)),
        None => FittingProject::empty(assets.join(MANNEQUIN)),
    };
    let home = source.is_none() && !demo;
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
            home,
        },
        false,
        false,
        false,
        false,
    )
}

fn main() {
    let source = match request(std::env::args_os()) {
        Request::Open(source) => source,
        Request::Version => {
            println!("FashionCraft {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Request::Help => {
            println!("{}", usage());
            return;
        }
        Request::Unknown(option) => {
            eprintln!(
                "FashionCraft: unknown option {}\n\n{}",
                option.to_string_lossy(),
                usage()
            );
            std::process::exit(2);
        }
    };
    if let Err(error) = start(source) {
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

    fn args(list: &[&str]) -> Vec<OsString> {
        std::iter::once("fashioncraft")
            .chain(list.iter().copied())
            .map(OsString::from)
            .collect()
    }

    /// Options print and quit; only what is not an option is a project to open.
    #[test]
    fn reads_the_command_line() {
        assert_eq!(request(args(&[])), Request::Open(None));
        assert_eq!(request(args(&["--version"])), Request::Version);
        assert_eq!(request(args(&["-V"])), Request::Version);
        assert_eq!(request(args(&["--help"])), Request::Help);
        assert_eq!(request(args(&["-h"])), Request::Help);
        assert_eq!(request(args(&["-psn_0_12345"])), Request::Open(None));
        assert_eq!(
            request(args(&["--verbose"])),
            Request::Unknown("--verbose".into())
        );
        assert_eq!(
            request(args(&["vest.fashioncraft", "--version"])),
            Request::Open(Some("vest.fashioncraft".into()))
        );
    }

    /// The System Settings advice is given on macOS only, in either language.
    #[test]
    fn suggests_system_settings_only_on_macos() {
        let path = Path::new("vest.fashioncraft");
        for spanish in [false, true] {
            let mac = unreadable_message(path, spanish, true);
            let other = unreadable_message(path, spanish, false);
            assert!(mac.contains("vest.fashioncraft") && other.contains("vest.fashioncraft"));
            assert!(mac.contains("Privacy & Security") || mac.contains("Privacidad y seguridad"));
            assert!(!other.contains("Privacy") && !other.contains("Privacidad"));
        }
    }

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
