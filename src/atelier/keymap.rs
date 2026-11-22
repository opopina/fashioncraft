//! Keyboard maps for tools and piece actions. "Compatible" (the default) puts each command
//! on the key garment designers already use in desktop cloth tools; "ClothCraft classic"
//! keeps this editor's earlier single keys. Keys shared by both (views, Space, Cmd+Z,
//! Shift+A…) are handled where they always were. Every binding runs a registry command.
use super::commands::CommandId;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum KeymapName {
    #[default]
    Compatible,
    Classic,
}

impl KeymapName {
    pub(super) const ALL: [Self; 2] = [Self::Compatible, Self::Classic];
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Compatible => "Compatible",
            Self::Classic => "FashionCraft classic",
        }
    }
}

/// Modifier combinations a binding may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mods {
    None,
    Shift,
    Alt,
    Cmd,
    ShiftCmd,
}

impl Mods {
    fn of(m: egui::Modifiers) -> Option<Self> {
        Some(match (m.shift, m.alt, m.command) {
            (false, false, false) => Self::None,
            (true, false, false) => Self::Shift,
            (false, true, false) => Self::Alt,
            (false, false, true) => Self::Cmd,
            (true, false, true) => Self::ShiftCmd,
            _ => return None,
        })
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Shift => "Shift+",
            Self::Alt => "Alt+",
            Self::Cmd => "Cmd+",
            Self::ShiftCmd => "Shift+Cmd+",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Binding {
    pub key: egui::Key,
    pub mods: Mods,
    pub command: CommandId,
}

const fn bind(key: egui::Key, mods: Mods, command: CommandId) -> Binding {
    Binding { key, mods, command }
}

use CommandId as C;
use egui::Key as K;

const COMPATIBLE: &[Binding] = &[
    bind(K::A, Mods::None, C::ToolEdit),
    bind(K::Z, Mods::None, C::ToolEdit),
    bind(K::H, Mods::None, C::ToolDraw),
    bind(K::S, Mods::None, C::ToolRectangle),
    bind(K::E, Mods::None, C::ToolEllipse),
    bind(K::G, Mods::None, C::ToolInternalLine),
    bind(K::B, Mods::None, C::ToolSew),
    bind(K::N, Mods::None, C::ToolSew),
    bind(K::T, Mods::None, C::ToolGraphic),
    bind(K::Y, Mods::None, C::ToggleLibrary),
    bind(K::D, Mods::None, C::ToolInspect),
    bind(K::W, Mods::Shift, C::ToggleGarment),
    bind(K::F, Mods::Shift, C::ToggleArrangementPoints),
    bind(K::Num6, Mods::Alt, C::StrainMap),
    bind(K::K, Mods::Cmd, C::FreezePieces),
    bind(K::J, Mods::Cmd, C::DeactivatePieces),
    bind(K::L, Mods::Cmd, C::StrengthenPieces),
    bind(K::E, Mods::Cmd, C::SolidifyPieces),
    bind(K::F, Mods::Cmd, C::ResetSelectedPieces3d),
    bind(K::Y, Mods::Cmd, C::Redo),
    bind(K::N, Mods::Cmd, C::NewProject),
    bind(K::A, Mods::ShiftCmd, C::ImportAvatar),
];

const CLASSIC: &[Binding] = &[
    bind(K::D, Mods::None, C::ToolInspect),
    bind(K::M, Mods::None, C::ToolMeasure),
    bind(K::R, Mods::None, C::ToolRotate),
    bind(K::G, Mods::None, C::ToolMove),
    bind(K::K, Mods::None, C::ToolGraphic),
];

pub(super) fn bindings(map: KeymapName) -> &'static [Binding] {
    match map {
        KeymapName::Compatible => COMPATIBLE,
        KeymapName::Classic => CLASSIC,
    }
}

/// The key a command has in `map`, written like "Cmd+K" or "H".
pub(super) fn label(map: KeymapName, command: CommandId) -> Option<String> {
    bindings(map)
        .iter()
        .find(|b| b.command == command)
        .map(|b| format!("{}{}", b.mods.prefix(), b.key.name()))
}

/// `keys` ("Cmd+K") as this platform writes it: with the Mac's symbols on macOS (⌘K), and with
/// Ctrl outside it, as `egui::Modifiers::command` already presses it there.
pub(super) fn shown(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        // As Mac menus write them: ⇧⌘Z, ⌥7.
        keys.replace("Shift+", "⇧")
            .replace("Alt+", "⌥")
            .replace("Cmd+", "⌘")
    } else {
        keys.replace("Cmd+", "Ctrl+")
    }
}

/// The commands whose keys were pressed this frame, consuming those key presses.
pub(super) fn pressed(map: KeymapName, ctx: &egui::Context) -> Vec<CommandId> {
    ctx.input_mut(|i| {
        let mut hits = vec![];
        for b in bindings(map) {
            let Some(mods) = Mods::of(i.modifiers) else {
                continue;
            };
            if mods == b.mods {
                let wanted = match mods {
                    Mods::None => egui::Modifiers::NONE,
                    Mods::Shift => egui::Modifiers::SHIFT,
                    Mods::Alt => egui::Modifiers::ALT,
                    Mods::Cmd => egui::Modifiers::COMMAND,
                    Mods::ShiftCmd => egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                };
                if i.consume_key(wanted, b.key) {
                    hits.push(b.command);
                }
            }
        }
        hits
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_is_bound_twice_in_a_map() {
        for map in KeymapName::ALL {
            let list = bindings(map);
            for (i, a) in list.iter().enumerate() {
                for b in &list[i + 1..] {
                    assert!(
                        !(a.key == b.key && a.mods == b.mods),
                        "{map:?}: {:?} {:?} twice",
                        a.mods,
                        a.key
                    );
                }
            }
        }
    }

    #[test]
    fn labels_read_like_menu_shortcuts() {
        assert_eq!(
            label(KeymapName::Compatible, C::FreezePieces).as_deref(),
            Some("Cmd+K")
        );
        assert_eq!(
            label(KeymapName::Compatible, C::ToolRectangle).as_deref(),
            Some("S")
        );
        assert_eq!(
            label(KeymapName::Classic, C::ToolGraphic).as_deref(),
            Some("K")
        );
        assert_eq!(label(KeymapName::Classic, C::FreezePieces), None);
        // The compatible map is the default for new preferences.
        assert_eq!(KeymapName::default(), KeymapName::Compatible);
    }

    #[test]
    fn the_command_key_reads_ctrl_outside_macos() {
        let undo = super::shown("Shift+Cmd+Z");
        if cfg!(target_os = "macos") {
            assert_eq!(undo, "⇧⌘Z");
            assert_eq!(super::shown("Alt+7"), "⌥7");
        } else {
            assert_eq!(undo, "Shift+Ctrl+Z");
        }
        assert_eq!(super::shown("H"), "H");
    }

    #[test]
    fn keys_shared_by_both_maps_are_not_rebound_here() {
        // Views, Space, F, Shift+A/C/I, Alt+0/7 and the file and edit keys keep their own
        // handlers; a map binding them again would run the command twice.
        let shared = [
            (K::F, Mods::None),
            (K::A, Mods::Shift),
            (K::C, Mods::Shift),
            (K::I, Mods::Shift),
            (K::Num7, Mods::Alt),
            (K::Num0, Mods::Alt),
            (K::Z, Mods::Cmd),
            (K::S, Mods::Cmd),
            (K::O, Mods::Cmd),
            (K::A, Mods::Cmd),
            (K::C, Mods::Cmd),
            (K::V, Mods::Cmd),
            (K::Q, Mods::None),
            (K::Num2, Mods::None),
        ];
        for map in KeymapName::ALL {
            for b in bindings(map) {
                assert!(
                    !shared.contains(&(b.key, b.mods)),
                    "{map:?} rebinds {:?} {:?}",
                    b.mods,
                    b.key
                );
            }
        }
    }
}
