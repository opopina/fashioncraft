//! Workspace presentation; no physical state or file-format promises live here.
use super::*;
use std::sync::OnceLock;

// The ArtCraft look, as ArtCraft's own apps draw it (storytold/artcraft `artcraft-native` and
// storytold/craft-launcher, Apache-2.0): near-black surfaces, hairline rules, square panels,
// 3 px corners on anything pressable. The colour tokens below are theirs, by their names.
// What is selected, and the wordmark, carry FashionCraft's own colour: fuchsia, in the widest gap
// of the ArtCraft family's colours, between FilmCraft's violet and EffectCraft's pink.

const fn hex(rgb: u32) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

const fn alpha(rgb: u32, a: u8) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgba_unmultiplied_const(r, g, b, a)
}

/// FashionCraft's fuchsia: selected things, active tools, the wordmark.
pub(super) const ACCENT: Color32 = hex(0xdd2cce);
/// The fuchsia as text on dark and on light surfaces.
const APP_INK_DARK: Color32 = hex(0xe65fd9);
const APP_INK_LIGHT: Color32 = hex(0xa0128f);
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

// Graphite: the mid greys of desktop 3D and cloth tools, lighter than ArtCraft's near-black so
// a long session is easier on the eyes and the fuchsia reads as an accent, not a glare.
const DARK: Tokens = Tokens {
    bg: hex(0x26272b),
    panel: hex(0x2c2d31),
    controls: hex(0x3a3b40),
    sunken: hex(0x1f2023),
    ink: hex(0xe8e8ea),
    ink_strong: Color32::WHITE,
    muted: hex(0xa4a6ad),
    line: alpha(0xffffff, 22),
    line_strong: alpha(0xffffff, 64),
    wash: alpha(0xffffff, 18),
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

/// Whether the dark appearance is installed: [`section`] titles have no `Ui` to ask.
static DARK_THEME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// FashionCraft's colour, solid, for what must stand out: a primary button, a handle, the line
/// under the chosen tab. Selected rows and tools only take a tint of it
/// (`visuals.selection.bg_fill`).
pub(super) fn accent(visuals: &egui::Visuals) -> Color32 {
    app_ink(visuals)
}

/// A section title in an inspector or window: a short name in FashionCraft's colour, as
/// desktop cloth tools title their property groups, not capitals in a box.
pub(super) fn section(text: impl Into<String>) -> egui::RichText {
    let dark = DARK_THEME.load(std::sync::atomic::Ordering::Relaxed);
    egui::RichText::new(text.into())
        .strong()
        .color(if dark { APP_INK_DARK } else { APP_INK_LIGHT })
}

/// A group title inside a tab, where a collapsing section would be too much.
pub(super) fn group_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.);
    ui.label(section(text));
}

/// Width of the names column of a property editor.
fn label_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() * 0.42).clamp(88., 150.)
}

/// One row of a property editor: its name on the left, in a column of its own, and its
/// controls on the right, as desktop cloth tools lay out their properties.
pub(super) fn prop_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal(|ui| {
        let width = label_width(ui);
        let weak = ui.visuals().weak_text_color();
        ui.allocate_ui_with_layout(
            Vec2::new(width, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(width);
                ui.add(egui::Label::new(egui::RichText::new(label).color(weak)).truncate());
            },
        );
        add(ui)
    })
}

/// A property row whose explanation sits in a small ⓘ beside the label, shown on hover,
/// instead of a paragraph in the panel.
pub(super) fn prop_row_help<R>(
    ui: &mut egui::Ui,
    label: &str,
    help: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal(|ui| {
        let width = label_width(ui);
        let weak = ui.visuals().weak_text_color();
        ui.allocate_ui_with_layout(
            Vec2::new(width, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.x = 5.;
                ui.add(egui::Label::new(egui::RichText::new(label).color(weak)).truncate());
                info_tip(ui, help);
            },
        );
        add(ui)
    })
}

/// A small circled "i" that shows `help` on hover.
pub(super) fn info_tip(ui: &mut egui::Ui, help: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(13.), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let colour = if response.hovered() {
            accent(ui.visuals())
        } else {
            ui.visuals().weak_text_color().gamma_multiply(0.8)
        };
        let painter = ui.painter();
        painter.circle_stroke(rect.center(), 5.8, egui::Stroke::new(1., colour));
        painter.circle_filled(rect.center() - Vec2::new(0., 2.6), 0.75, colour);
        painter.line_segment(
            [
                rect.center() - Vec2::new(0., 0.9),
                rect.center() + Vec2::new(0., 3.),
            ],
            egui::Stroke::new(1.1, colour),
        );
    }
    response.on_hover_text(help)
}

/// A slider drawn thin: a 2 px rail, the travelled part in FashionCraft's colour and a small
/// round handle. egui sizes a slider's handle from the row height, so the row is lowered for
/// the slider alone.
pub(super) fn slim(ui: &mut egui::Ui, slider: egui::Slider<'_>) -> egui::Response {
    ui.scope(|ui| {
        let accent = accent(ui.visuals());
        let spacing = ui.spacing_mut();
        spacing.interact_size.y = 12.;
        spacing.slider_rail_height = 2.;
        let visuals = ui.visuals_mut();
        visuals.selection.bg_fill = accent;
        visuals.slider_trailing_fill = true;
        visuals.handle_shape = egui::style::HandleShape::Circle;
        ui.add(slider)
    })
    .inner
}

/// A pane's title, centred on its own row as desktop tools title their panels.
pub(super) fn pane_title(ui: &mut egui::Ui, text: &str) -> egui::Rect {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.), egui::Sense::hover());
    let visuals = ui.visuals();
    ui.painter()
        .rect_filled(rect, 0., visuals.faint_bg_color.gamma_multiply(0.5));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(12.),
        visuals.weak_text_color(),
    );
    rect
}

/// An icon tab: the icon, in FashionCraft's colour and underlined when chosen; its name shows
/// on hover.
pub(super) fn icon_tab<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: &mut T,
    value: T,
    icon: super::icons::Icon,
    label: &str,
    width: f32,
) -> egui::Response {
    let chosen = *current == value;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let color = if chosen {
            accent(visuals)
        } else if response.hovered() {
            visuals.strong_text_color()
        } else {
            visuals.weak_text_color()
        };
        super::icons::glyph(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), Vec2::splat(18.)),
            icon,
            color,
            1.5,
        );
        if chosen {
            let y = rect.bottom() - 1.;
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 6., y),
                    egui::pos2(rect.right() - 6., y),
                ],
                Stroke::new(2., accent(visuals)),
            );
        }
    }
    if response.clicked() {
        *current = value;
    }
    response.on_hover_text(label)
}

/// One row of a browser list: a colour swatch, the name, a quiet detail on the right, tinted
/// when chosen.
pub(super) fn list_row(
    ui: &mut egui::Ui,
    swatch: Option<Color32>,
    name: &str,
    detail: &str,
    chosen: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if chosen {
            visuals.selection.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 3., fill);
        let mut x = rect.left() + 8.;
        if let Some(swatch) = swatch {
            let square =
                egui::Rect::from_center_size(egui::pos2(x + 9., rect.center().y), Vec2::splat(18.));
            ui.painter().rect_filled(square, 3., swatch);
            ui.painter().rect_stroke(
                square,
                3.,
                Stroke::new(1., visuals.widgets.inactive.bg_stroke.color),
                egui::StrokeKind::Inside,
            );
            x += 28.;
        }
        let detail_galley = ui.painter().layout_no_wrap(
            detail.to_owned(),
            egui::FontId::proportional(11.),
            visuals.weak_text_color(),
        );
        let detail_width = detail_galley.size().x;
        ui.painter().galley(
            egui::pos2(
                rect.right() - 8. - detail_width,
                rect.center().y - detail_galley.size().y * 0.5,
            ),
            detail_galley,
            visuals.weak_text_color(),
        );
        let ink = if chosen {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        let name_galley = egui::WidgetText::from(egui::RichText::new(name).color(ink)).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.right() - 16. - detail_width - x).max(20.),
            egui::TextStyle::Body,
        );
        ui.painter().galley(
            egui::pos2(x, rect.center().y - name_galley.size().y * 0.5),
            name_galley,
            ink,
        );
    }
    response
}

/// Design or Animation: the two ways of working, as a two-part switch. Returns the side chosen
/// this frame.
pub(super) fn mode_switch(ui: &mut egui::Ui, animation: bool, labels: [&str; 2]) -> Option<bool> {
    let mut chosen = None;
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(5)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            for (k, label) in labels.into_iter().enumerate() {
                let on = animation == (k == 1);
                let visuals = ui.visuals();
                let text = egui::RichText::new(label).color(if on {
                    visuals.strong_text_color()
                } else {
                    visuals.weak_text_color()
                });
                let button = egui::Button::new(text)
                    .fill(if on {
                        visuals.widgets.inactive.bg_fill
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE)
                    .min_size(Vec2::new(0., 22.));
                if ui.add(button).clicked() && !on {
                    chosen = Some(k == 1);
                }
            }
        });
    chosen
}

/// A capsule floating at the bottom centre of a view, for what the current tool needs right
/// now (its step, its options, its confirm button). It never pushes the view down, as rows
/// under the toolbar did, and it is not drawn at all when `add` shows nothing.
pub(super) fn floating_capsule(
    ctx: &egui::Context,
    id: &str,
    pane: egui::Rect,
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(pane.center_bottom() - Vec2::new(0., 18.))
        .show(ctx, |ui| {
            ui.set_max_width((pane.width() - 40.).max(200.));
            let visuals = ui.visuals().clone();
            let mut prepared = egui::Frame::new()
                .fill(visuals.window_fill.gamma_multiply(0.95))
                .stroke(Stroke::new(
                    1.,
                    visuals.widgets.noninteractive.bg_stroke.color,
                ))
                .corner_radius(12)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .shadow(visuals.popup_shadow)
                .begin(ui);
            prepared.content_ui.vertical(add);
            let used = prepared.content_ui.min_rect();
            let key = egui::Id::new((id, "capsule-rect"));
            if used.width() > 2. && used.height() > 2. {
                let drawn = prepared.end(ui).rect;
                ctx.data_mut(|d| d.insert_temp(key, drawn));
            } else {
                ctx.data_mut(|d| d.remove::<egui::Rect>(key));
            }
        });
}

/// `place` moved up clear of the floating capsule `id` when the two would overlap, so
/// corner labels of a view stay readable when the capsule is wide.
pub(super) fn clear_of_capsule(ctx: &egui::Context, id: &str, place: egui::Rect) -> egui::Rect {
    let capsule: Option<egui::Rect> = ctx.data(|d| d.get_temp(egui::Id::new((id, "capsule-rect"))));
    match capsule {
        Some(capsule) if capsule.expand(4.).intersects(place) => {
            place.translate(Vec2::new(0., capsule.top() - 6. - place.bottom()))
        }
        _ => place,
    }
}

/// One tab of a row of tabs: its name, underlined in FashionCraft's colour when chosen.
pub(super) fn tab<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: &mut T,
    value: T,
    label: &str,
) -> egui::Response {
    let chosen = *current == value;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, Color32::WHITE);
    let size = Vec2::new(galley.size().x + 20., 30.);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let color = if chosen {
            accent(visuals)
        } else if response.hovered() {
            visuals.strong_text_color()
        } else {
            visuals.weak_text_color()
        };
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, color);
        if chosen {
            let y = rect.bottom() - 1.;
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 6., y),
                    egui::pos2(rect.right() - 6., y),
                ],
                Stroke::new(2., accent(visuals)),
            );
        }
    }
    if response.clicked() {
        *current = value;
    }
    response
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
    style.spacing.item_spacing = Vec2::new(8., 5.);
    style.spacing.button_padding = Vec2::new(8., 4.);
    style.spacing.interact_size.y = 24.;
    style.spacing.slider_rail_height = 2.;
    style.spacing.slider_width = 120.;
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
    // What is chosen is tinted with the fuchsia and named in it, not filled solid: solid
    // fuchsia tabs, rows and choices shouted over the work.
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(
        ACCENT.r(),
        ACCENT.g(),
        ACCENT.b(),
        if t.bg == DARK.bg { 60 } else { 40 },
    );
    visuals.selection.stroke = Stroke::new(1., t.accent_ink);
    visuals.handle_shape = egui::style::HandleShape::Circle;
    visuals.slider_trailing_fill = true;
    visuals.warn_fg_color = t.warn;
    visuals.error_fg_color = t.bad;
    visuals.striped = false;
    // Section headers are a coloured title with its arrow, not a band.
    visuals.collapsing_header_frame = false;
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

/// What the top bar shows instead of words in a narrow window. Each is a single symbol, so it
/// must be one egui can draw with the installed fonts (checked by a test), or the button shows
/// an empty box: several emoji outside the basic plane, such as a magnifier, cannot be.
pub(super) mod symbols {
    pub const PLAY: &str = "▶";
    pub const PAUSE: &str = "⏸";
    pub const EXPORT: &str = "⬆";
}

fn install(ctx: &egui::Context, dark: bool) {
    DARK_THEME.store(dark, std::sync::atomic::Ordering::Relaxed);
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
    /// Archived pieces, each with Restore and Delete; hidden while the archive is empty.
    pub(super) fn archive_list(&mut self, ui: &mut egui::Ui) {
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
            self.files_and_formats(ui)
        });
    }

    /// The formats FashionCraft reads and writes, and where exports go.
    pub(super) fn files_and_formats(&self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        {
            ui.label(i18n::tr(ui_lang, "Import"));
            ui.small(i18n::tr(ui_lang, "FashionCraft project and template JSON"));
            ui.small(i18n::tr(
                ui_lang,
                "Static avatars: OBJ · glTF · GLB · VRM · STL · PLY · COLLADA · FBX",
            ));
            ui.small(i18n::tr(
                ui_lang,
                "Pattern outlines: generic ASCII DXF · physical SVG subsets",
            ));
            ui.label(i18n::tr(ui_lang, "Export"));
            ui.small(i18n::tr(ui_lang, "Project and template JSON · garment OBJ (welded, quads, retopology, baked look) · glTF · GLB · FBX · USD · USDZ · Alembic · MDD and PC2 caches · SVG patterns · fiber guides"));
            ui.small(i18n::tr(
                ui_lang,
                "Industrial DXF and PDF are not yet supported.",
            ));
            ui.separator();
            ui.small(i18n::tr(
                ui_lang,
                "Export folder (files replaced on every export):",
            ));
            ui.label(egui::RichText::new(self.output.display().to_string()).small());
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    #[test]
    fn the_compact_top_bar_symbols_have_glyphs() {
        let ctx = egui::Context::default();
        super::style(&ctx);
        // Fonts set on a context take effect on its next pass.
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        use super::symbols::*;
        for symbol in [PLAY, PAUSE, EXPORT] {
            let shown = ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(13.), symbol));
            assert!(shown, "{symbol} has no glyph in the workspace fonts");
        }
    }
}
