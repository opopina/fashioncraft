//! Workspace presentation; no physical state or file-format promises live here.
use super::*;
use std::sync::OnceLock;

// The ArtCraft look, as ArtCraft's own apps draw it (storytold/artcraft `artcraft-native` and
// storytold/craft-launcher, Apache-2.0): near-black surfaces, hairline rules, square panels,
// 3 px corners on anything pressable. The colour tokens below are theirs, by their names.
// What is selected, and the wordmark, carry FashionCraft's own colour: amber, which in the
// ArtCraft family only AniCraft's yellow comes near.

const fn hex(rgb: u32) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

const fn alpha(rgb: u32, a: u8) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgba_unmultiplied_const(r, g, b, a)
}

/// FashionCraft's amber: selected things, active tools, the wordmark.
pub(super) const ACCENT: Color32 = hex(0xf2a13a);
/// Text and icons drawn on the amber.
const ON_ACCENT: Color32 = hex(0x1b1b1f);
/// The amber as text on dark and on light surfaces.
const APP_INK_DARK: Color32 = ACCENT;
const APP_INK_LIGHT: Color32 = hex(0x9c600c);
/// Corners of buttons, fields, menus and windows; panels stay square.
const RADIUS: egui::CornerRadius = egui::CornerRadius::same(3);

/// One appearance's colours.
struct Tokens {
    /// The page behind everything.
    bg: Color32,
    /// Windows, menus and the top bar.
    panel: Color32,
    /// Buttons and other controls at rest.
    controls: Color32,
    /// Wells and inputs.
    sunken: Color32,
    ink: Color32,
    ink_strong: Color32,
    muted: Color32,
    /// Every hairline.
    line: Color32,
    /// Outlined buttons and hovered borders.
    line_strong: Color32,
    /// Hovered and pressed controls.
    wash: Color32,
    accent_ink: Color32,
    warn: Color32,
    bad: Color32,
    shadow: Color32,
}

const DARK: Tokens = Tokens {
    bg: hex(0x121316),
    panel: hex(0x101014),
    controls: hex(0x1e1f22),
    sunken: hex(0x060607),
    ink: hex(0xf2f1ee),
    ink_strong: Color32::WHITE,
    muted: alpha(0xf2f1ee, 153),
    line: alpha(0xffffff, 38),
    line_strong: alpha(0xf2f1ee, 102),
    wash: alpha(0xffffff, 26),
    accent_ink: APP_INK_DARK,
    warn: hex(0xffc14d),
    bad: hex(0xf87171),
    shadow: Color32::from_black_alpha(140),
};

const LIGHT: Tokens = Tokens {
    bg: hex(0xf2f1ee),
    panel: hex(0xf7f6f3),
    controls: hex(0xe9e8e4),
    sunken: hex(0xffffff),
    ink: hex(0x101014),
    ink_strong: Color32::BLACK,
    muted: alpha(0x101014, 158),
    line: alpha(0x101014, 41),
    line_strong: alpha(0x101014, 102),
    wash: alpha(0x101014, 20),
    accent_ink: APP_INK_LIGHT,
    warn: hex(0x8a5800),
    bad: hex(0xb3261e),
    shadow: Color32::from_black_alpha(36),
};

/// FashionCraft's colour, readable on the current appearance.
pub(super) fn app_ink(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        APP_INK_DARK
    } else {
        APP_INK_LIGHT
    }
}

/// A section title in an inspector or window: small capitals, as ArtCraft labels its groups.
pub(super) fn section(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into().to_uppercase())
        .small()
        .strong()
}

/// Pristine egui base styles, captured before the first install. Deriving each
/// theme from its own base keeps switching Light -> Dark -> Light idempotent
/// instead of letting the light palette inherit the dark overrides.
fn bases(ctx: &egui::Context) -> &'static (egui::Style, egui::Style) {
    static BASES: OnceLock<(egui::Style, egui::Style)> = OnceLock::new();
    BASES.get_or_init(|| {
        (
            (*ctx.style_of(egui::Theme::Light)).clone(),
            (*ctx.style_of(egui::Theme::Dark)).clone(),
        )
    })
}

/// Theme-independent geometry and text sizes: dense enough for an editor.
fn geometry(style: &mut egui::Style) {
    style.spacing.item_spacing = Vec2::new(8., 6.);
    style.spacing.button_padding = Vec2::new(10., 6.);
    style.spacing.interact_size.y = 28.;
    style.spacing.menu_margin = egui::Margin::same(4);
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 8.,
        floating: true,
        ..egui::style::ScrollStyle::thin()
    };
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.));
}

fn visuals(visuals: &mut egui::Visuals, t: &Tokens) {
    let shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: t.shadow,
    };
    visuals.panel_fill = t.bg;
    visuals.window_fill = t.panel;
    visuals.faint_bg_color = t.controls;
    visuals.extreme_bg_color = t.sunken;
    visuals.code_bg_color = t.sunken;
    visuals.text_edit_bg_color = Some(t.sunken);
    visuals.window_stroke = Stroke::new(1., t.line);
    visuals.window_corner_radius = RADIUS;
    visuals.menu_corner_radius = RADIUS;
    visuals.window_shadow = shadow;
    visuals.popup_shadow = shadow;
    // No overriding text colour: selected controls draw their text dark on the amber.
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(t.muted);
    visuals.hyperlink_color = t.accent_ink;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1., ON_ACCENT);
    visuals.warn_fg_color = t.warn;
    visuals.error_fg_color = t.bad;
    visuals.striped = false;
    // Section headers sit on a full-width band, as property groups do.
    visuals.collapsing_header_frame = true;
    for (w, fill, stroke, fg) in [
        (&mut visuals.widgets.noninteractive, t.bg, t.line, t.ink),
        (&mut visuals.widgets.inactive, t.controls, t.line, t.ink),
        (
            &mut visuals.widgets.hovered,
            t.wash,
            t.line_strong,
            t.ink_strong,
        ),
        (
            &mut visuals.widgets.active,
            t.wash,
            t.line_strong,
            t.ink_strong,
        ),
        (
            &mut visuals.widgets.open,
            t.wash,
            t.line_strong,
            t.ink_strong,
        ),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1., stroke);
        w.corner_radius = RADIUS;
        w.fg_stroke = Stroke::new(1., fg);
        w.expansion = 0.;
    }
}

/// Archivo at its display weight and width: the FashionCraft wordmark and large titles.
pub(super) fn display() -> egui::FontFamily {
    egui::FontFamily::Name("display".into())
}

/// The display family once [`install_fonts`] has put it in, else the body family.
pub(super) fn display_or_body(ui: &egui::Ui) -> egui::FontFamily {
    let installed = ui.fonts(|f| f.definitions().families.contains_key(&display()));
    if installed {
        display()
    } else {
        egui::FontFamily::Proportional
    }
}

/// A face to register: its name, its file, its variation coordinates and its family.
type Face<'a> = (
    &'a str,
    &'static [u8],
    &'a [([u8; 4], f32)],
    egui::FontFamily,
);

/// ArtCraft's typefaces: Inter for text, Archivo for display type and Geist Mono for
/// monospaced text, each at the weight (and width) ArtCraft's apps use. egui's own faces stay
/// behind them, for the symbols and emoji these lack.
fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontFamily, FontTweak, epaint::text::VariationCoords};
    use std::sync::Arc;
    const INTER: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/fonts/Inter-Variable.ttf"
    ));
    const ARCHIVO: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/fonts/Archivo-Variable.ttf"
    ));
    const GEIST_MONO: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/fonts/GeistMono-Variable.ttf"
    ));
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let faces: [Face<'_>; 3] = [
        (
            "Inter",
            INTER,
            &[(*b"wght", 400.), (*b"opsz", 14.)],
            FontFamily::Proportional,
        ),
        (
            "Archivo-Display",
            ARCHIVO,
            &[(*b"wght", 620.), (*b"wdth", 118.)],
            display(),
        ),
        (
            "GeistMono",
            GEIST_MONO,
            &[(*b"wght", 400.)],
            FontFamily::Monospace,
        ),
    ];
    for (name, bytes, coords, family) in faces {
        // These faces carry no hinting: drawn at fractional positions, small text would blur.
        let tweak = FontTweak {
            coords: VariationCoords::new(coords.iter().copied()),
            subpixel_binning: Some(false),
            ..Default::default()
        };
        fonts.font_data.insert(
            name.to_owned(),
            Arc::new(FontData::from_static(bytes).tweak(tweak)),
        );
        let mut stack = vec![name.to_owned()];
        stack.extend(fallback.iter().cloned());
        fonts.families.insert(family, stack);
    }
    ctx.set_fonts(fonts);
}

fn install(ctx: &egui::Context, dark: bool) {
    install_fonts(ctx);
    let base = bases(ctx);
    let mut style = if dark { base.1.clone() } else { base.0.clone() };
    geometry(&mut style);
    visuals(&mut style.visuals, if dark { &DARK } else { &LIGHT });
    // Both egui theme slots receive the same workspace look, so the active theme
    // preference cannot pick a half-styled variant.
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Install the light workspace chrome.
pub(super) fn style(ctx: &egui::Context) {
    install(ctx, false);
}

/// Install the dark workspace chrome. `settings::apply` chooses between the two.
// The only caller is `settings::apply`, which lands with the module declaration.
#[allow(dead_code)]
pub(super) fn dark_style(ctx: &egui::Context) {
    install(ctx, true);
}

/// Menu titles in the top bar: plain text that washes on hover, like a menu bar, not boxes.
pub(super) fn menu_bar_style(ui: &mut egui::Ui) {
    let widgets = &mut ui.visuals_mut().widgets;
    widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    widgets.inactive.bg_fill = Color32::TRANSPARENT;
    widgets.inactive.bg_stroke = Stroke::NONE;
    for w in [&mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
        w.bg_stroke = Stroke::NONE;
    }
}

impl Atelier {
    pub(super) fn panel_browser(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let mut select = None;
        egui::CollapsingHeader::new(section(format!(
            "{} ({})",
            i18n::tr(ui_lang, "Pieces"),
            self.project.template.panels.len()
        )))
        .id_salt("panel_browser")
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("panel_list")
                .max_height(115.)
                .show(ui, |ui| {
                    for panel in &self.project.template.panels {
                        let name = if self.project.template.locked_panels.contains(&panel.id) {
                            format!("{} · locked in 2D", panel.name)
                        } else {
                            panel.name.clone()
                        };
                        if ui
                            .selectable_label(
                                self.authoring.selected.as_ref() == Some(&panel.id),
                                name,
                            )
                            .on_hover_text(format!("{} · select in both views", panel.id))
                            .clicked()
                        {
                            select = Some(panel.id.clone());
                        }
                    }
                });
        });
        if let Some(id) = select {
            self.set_authoring_tool(Tool::Edit, false);
            self.cancel_preview();
            self.select_panel(id, ui.input(|i| i.modifiers.shift));
            self.authoring.edge = None;
        }
        self.archive_list(ui);
        ui.separator();
    }

    /// Archived pieces, each with Restore and Delete; hidden while the archive is empty.
    fn archive_list(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        if self.project.template.archive.is_empty() {
            return;
        }
        let mut command = None;
        egui::CollapsingHeader::new(section(format!("{} ({})", i18n::tr(ui_lang, "Archived"),
            self.project.template.archive.len()
        )))
        .id_salt("archive_list")
        .show(ui, |ui| {
            for entry in &self.project.template.archive {
                ui.horizontal(|ui| {
                    ui.label(entry.names.join(", "))
                        .on_hover_text(i18n::tr(ui_lang, "Off the board and out of the simulation, kept with the project"));
                    if ui
                        .small_button(i18n::tr(ui_lang, "Restore"))
                        .on_hover_text(i18n::tr(ui_lang, "Bring them back where they were, sewn to the pieces still on the board"))
                        .clicked()
                    {
                        command = Some(fashioncraft::authoring::Command::RestoreArchived {
                            id: entry.id.clone(),
                        });
                    }
                    if ui.small_button(i18n::tr(ui_lang, "Delete")).clicked() {
                        command = Some(fashioncraft::authoring::Command::DeleteArchived {
                            id: entry.id.clone(),
                        });
                    }
                });
            }
        });
        if let Some(command) = command {
            self.edit(command);
        }
    }
    pub(super) fn export_information(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        ui.collapsing(section(i18n::tr(ui_lang, "Files & formats")), |ui| {
            ui.label(i18n::tr(ui_lang, "Import"));
            ui.small(i18n::tr(ui_lang, "FashionCraft project and template JSON"));
            ui.small(i18n::tr(ui_lang, "Static avatars: OBJ · glTF · GLB · VRM · STL · PLY · COLLADA · FBX"));
            ui.small(i18n::tr(ui_lang, "Pattern outlines: generic ASCII DXF · physical SVG subsets"));
            ui.label(i18n::tr(ui_lang, "Export"));
            ui.small(i18n::tr(ui_lang, "Project and template JSON · garment OBJ (welded, quads, retopology, baked look) · glTF · GLB · FBX · USD · USDZ · Alembic · MDD and PC2 caches · SVG patterns · fiber guides"));
            ui.small(i18n::tr(ui_lang, "Industrial DXF and PDF are not yet supported."));
            ui.separator();
            ui.small(i18n::tr(ui_lang, "Export folder (files replaced on every export):"));
            ui.label(egui::RichText::new(self.output.display().to_string()).small());
        });
    }
}
