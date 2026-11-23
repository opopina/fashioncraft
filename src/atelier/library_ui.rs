//! The library: a floating window over a folder of the user's assets, as desktop cloth tools
//! keep one. Garments (project files) open, avatars go through the avatar importer, images
//! become graphics on the selected piece and measured fabrics open in Measure fabric or cut
//! the selected piece. Nothing is copied or deleted here.
use super::*;
use std::path::{Path, PathBuf};

/// How deep the library looks inside its folder.
const MAX_DEPTH: usize = 3;
/// Most files one tab lists.
const MAX_FILES: usize = 500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Shelf {
    #[default]
    Garments,
    Avatars,
    Images,
    /// Fabrics measured with Materials › Measure fabric.
    Fabrics,
}

impl Shelf {
    const ALL: [Self; 4] = [Self::Garments, Self::Avatars, Self::Images, Self::Fabrics];
    fn label(self) -> &'static str {
        match self {
            Self::Garments => "Garments",
            Self::Avatars => "Avatars",
            Self::Images => "Images",
            Self::Fabrics => "Fabrics",
        }
    }
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Garments => &["json"],
            Self::Avatars => &[
                "obj", "gltf", "glb", "vrm", "stl", "ply", "dae", "fbx", "abc", "usda", "usd",
                "usdz",
            ],
            Self::Images => &["png"],
            Self::Fabrics => &[fashioncraft::fabric_kit::FILE_EXTENSION],
        }
    }
}

#[derive(Default)]
pub(super) struct LibraryUi {
    pub open: bool,
    shelf: Shelf,
    /// Files of the current shelf, relative to the folder, and the folder they were read from.
    listing: Option<(PathBuf, Shelf, Vec<PathBuf>)>,
}

impl LibraryUi {
    /// Lists the folder again on the next frame, after a file was added to it.
    pub(super) fn invalidate(&mut self) {
        self.listing = None;
    }
}

/// Files under `root` with one of `extensions`, depth-first up to [`MAX_DEPTH`], sorted.
pub(super) fn scan(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    fn walk(dir: &Path, root: &Path, depth: usize, ext: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if out.len() >= MAX_FILES {
                return;
            }
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if hidden {
                continue;
            }
            if path.is_dir() {
                // A project's own asset folder belongs to it, not to the library.
                let assets = path.extension().is_some_and(|e| e == "assets");
                if depth < MAX_DEPTH && !assets {
                    walk(&path, root, depth + 1, ext, out);
                }
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ext.iter().any(|x| x.eq_ignore_ascii_case(e)))
                && let Ok(relative) = path.strip_prefix(root)
            {
                out.push(relative.to_path_buf());
            }
        }
    }
    let mut out = vec![];
    walk(root, root, 1, extensions, &mut out);
    out.sort();
    out
}

impl Atelier {
    /// The library folder: the one chosen in preferences, else `library` in the session folder.
    pub(super) fn library_dir(&self) -> PathBuf {
        self.prefs
            .library_dir
            .clone()
            .unwrap_or_else(|| self.output.join("library"))
    }

    pub(super) fn library_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.library.open {
            return;
        }
        let root = self.library_dir();
        let mut open = true;
        let mut action: Option<(Shelf, PathBuf)> = None;
        let mut apply_fabric: Option<PathBuf> = None;
        let mut rescan = false;
        egui::Window::new(i18n::tr(ui_lang, "Library"))
            .open(&mut open)
            .default_width(300.)
            .default_height(380.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.small(root.display().to_string());
                });
                ui.horizontal(|ui| {
                    if ui
                        .small_button(i18n::tr(ui_lang, "Choose folder…"))
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().set_directory(&root).pick_folder()
                    {
                        self.prefs.library_dir = Some(dir);
                        self.prefs_dirty = true;
                        rescan = true;
                    }
                    if ui.small_button(i18n::tr(ui_lang, "Refresh")).clicked() {
                        rescan = true;
                    }
                    if !root.is_dir()
                        && ui
                            .small_button(i18n::tr(ui_lang, "Create folder"))
                            .on_hover_text(i18n::tr(
                                ui_lang,
                                "Make the library folder so you can drop files in it",
                            ))
                            .clicked()
                    {
                        self.status = match std::fs::create_dir_all(&root) {
                            Ok(()) => "Library folder created".into(),
                            Err(e) => format!("Cannot create the library folder: {e}"),
                        };
                        rescan = true;
                    }
                });
                ui.horizontal(|ui| {
                    for shelf in Shelf::ALL {
                        if ui
                            .selectable_label(
                                self.authoring.library.shelf == shelf,
                                i18n::tr(ui_lang, shelf.label()),
                            )
                            .clicked()
                        {
                            self.authoring.library.shelf = shelf;
                        }
                    }
                });
                ui.separator();
                let shelf = self.authoring.library.shelf;
                let stale = self
                    .authoring
                    .library
                    .listing
                    .as_ref()
                    .is_none_or(|(dir, s, _)| dir != &root || *s != shelf);
                if stale || rescan {
                    self.authoring.library.listing =
                        Some((root.clone(), shelf, scan(&root, shelf.extensions())));
                }
                let files = self
                    .authoring
                    .library
                    .listing
                    .as_ref()
                    .map(|(_, _, f)| f.clone())
                    .unwrap_or_default();
                if files.is_empty() {
                    ui.small(match shelf {
                        Shelf::Garments => {
                            i18n::tr(ui_lang, "No project files (.json) in this folder yet.")
                        }
                        Shelf::Avatars => i18n::tr(
                            ui_lang,
                            "No avatars (.obj, .gltf, .glb) in this folder yet.",
                        ),
                        Shelf::Images => i18n::tr(ui_lang, "No PNG images in this folder yet."),
                        Shelf::Fabrics => i18n::tr(
                            ui_lang,
                            "No measured fabrics in this folder yet: make one with Materials › Measure fabric…",
                        ),
                    });
                }
                let verb = match shelf {
                    Shelf::Garments => "Open",
                    Shelf::Avatars => "Import",
                    Shelf::Images => "Add to piece",
                    Shelf::Fabrics => "Open",
                };
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for file in &files {
                        ui.horizontal(|ui| {
                            let name = file.display().to_string();
                            let row = ui
                                .selectable_label(false, &name)
                                .on_hover_text(format!("Double-click: {}", verb.to_lowercase()));
                            if ui.small_button(i18n::tr(ui_lang, verb)).clicked()
                                || row.double_clicked()
                            {
                                action = Some((shelf, root.join(file)));
                            }
                            if shelf == Shelf::Fabrics
                                && ui
                                    .small_button(i18n::tr(ui_lang, "Apply to piece"))
                                    .on_hover_text(i18n::tr(
                                        ui_lang,
                                        "Cut the selected piece from this fabric",
                                    ))
                                    .clicked()
                            {
                                apply_fabric = Some(root.join(file));
                            }
                        });
                    }
                });
            });
        self.authoring.library.open &= open;
        match action {
            Some((Shelf::Garments, path)) => {
                if self.can_replace_document(ctx) {
                    self.open_document_path(path, false);
                }
            }
            Some((Shelf::Avatars, path)) => {
                self.open_avatar_import(ctx);
                self.choose_avatar(&path);
            }
            Some((Shelf::Images, path)) => match self.authoring.selected.clone() {
                Some(panel) => self.add_graphic_from(&panel, path),
                None => {
                    self.status = i18n::tr(ui_lang, "Select a piece to put the image on").into()
                }
            },
            Some((Shelf::Fabrics, path)) => self.open_fabric_file(&path),
            None => {}
        }
        if let Some(path) = apply_fabric {
            self.apply_fabric_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_lists_matching_files_and_skips_project_assets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("shirts/garment.json.assets")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::create_dir_all(root.join("Fabrics")).unwrap();
        for file in [
            "Fabrics/poplin.fcfabric",
            "shirts/garment.json",
            "shirts/garment.json.assets/image-1.png",
            "dress.JSON",
            "logo.png",
            "notes.txt",
            ".hidden/secret.json",
        ] {
            std::fs::write(root.join(file), b"x").unwrap();
        }
        assert_eq!(
            scan(root, Shelf::Garments.extensions()),
            vec![
                PathBuf::from("dress.JSON"),
                PathBuf::from("shirts/garment.json")
            ]
        );
        assert_eq!(
            scan(root, Shelf::Images.extensions()),
            vec![PathBuf::from("logo.png")]
        );
        // Measured fabrics have their own shelf and stay off the garments'.
        assert_eq!(
            scan(root, Shelf::Fabrics.extensions()),
            vec![PathBuf::from("Fabrics/poplin.fcfabric")]
        );
        assert!(scan(&root.join("missing"), &["json"]).is_empty());
    }
}
