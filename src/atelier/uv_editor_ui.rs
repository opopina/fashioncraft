//! UV editor (Materials › UV › UV editor, Window › UV editor): every piece's island in the
//! texture square that exports and the baked atlas use. Click an island to select it (Shift
//! adds), drag to move, drag a corner handle to scale about the selection's centre, drag the
//! round handle above it to turn (Shift snaps to 15°); drag on empty space to select with a
//! box, scroll to zoom, middle or right drag to pan, Escape cancels a drag. Pack fits every
//! island into the square without overlap, a margin apart. Each change is one undo step; the
//! fabric look in the 3D view never changes, it follows the material, not the layout.
use super::*;
use fashioncraft::{
    authoring::Command,
    garment::uv_layout::{self, IslandShape, PackOptions, PlacedIsland, UvPlacement},
};
use glam::DVec2;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Move,
    Scale,
    Turn,
    Box,
    Pan,
}

struct UvDrag {
    handle: Handle,
    /// Pointer where the drag started, texture units.
    start: DVec2,
    /// Placements of the selected islands when it started.
    before: BTreeMap<String, UvPlacement>,
    /// Centre of the selection's bounds: what turns and scaling go about.
    pivot: DVec2,
}

/// The islands and their automatic or edited placements, rebuilt when the garment or the
/// layout changes.
struct Cache {
    key: u64,
    shapes: Vec<IslandShape>,
    resolved: Vec<UvPlacement>,
    overlapping: BTreeSet<usize>,
    outside: BTreeSet<usize>,
}

pub(super) struct UvEditor {
    pub open: bool,
    selected: BTreeSet<String>,
    /// The piece selected in the rest of the editor when last looked at.
    app_selection: Option<String>,
    drag: Option<UvDrag>,
    /// The layout being dragged, drawn before it is committed.
    preview: Option<BTreeMap<String, UvPlacement>>,
    /// Texture point at the canvas centre, and zoom over the fitted view.
    view_center: DVec2,
    zoom: f32,
    pub pack: PackOptions,
    cache: Option<Cache>,
}

impl Default for UvEditor {
    fn default() -> Self {
        Self {
            open: false,
            selected: BTreeSet::new(),
            app_selection: None,
            drag: None,
            preview: None,
            view_center: DVec2::splat(0.5),
            zoom: 1.,
            pack: PackOptions::default(),
            cache: None,
        }
    }
}

/// Pixels between the square and the canvas edge in the fitted view.
const PAD: f32 = 28.;
/// Pixels from the selection's top to the turn handle.
const TURN_HANDLE: f32 = 26.;
const HANDLE_RADIUS: f32 = 6.;

fn cache_key(
    template: &fashioncraft::garment::GarmentTemplate,
    garment: &fashioncraft::garment::CompiledGarment,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&template.uv_layout)
        .unwrap_or_default()
        .hash(&mut h);
    for p in &template.panels {
        p.id.hash(&mut h);
        p.layout_rotation_degrees.to_bits().hash(&mut h);
    }
    for p in &garment.panels {
        p.id.hash(&mut h);
        p.vertices.hash(&mut h);
        p.triangles.hash(&mut h);
    }
    for q in &garment.mesh.uv {
        q.x.to_bits().hash(&mut h);
        q.y.to_bits().hash(&mut h);
    }
    h.finish()
}

/// Canvas mapping: texture units to screen points and back.
#[derive(Clone, Copy)]
struct View {
    rect: egui::Rect,
    center: DVec2,
    pixels: f64,
}

impl View {
    fn screen(&self, p: DVec2) -> egui::Pos2 {
        let d = (p - self.center) * self.pixels;
        self.rect.center() + egui::vec2(d.x as f32, d.y as f32)
    }
    fn texture(&self, p: egui::Pos2) -> DVec2 {
        let d = p - self.rect.center();
        self.center + DVec2::new(d.x as f64, d.y as f64) / self.pixels
    }
}

impl Atelier {
    pub(super) fn open_uv_editor(&mut self) {
        self.authoring.uv_editor.open = true;
        self.status = i18n::tr(
            self.prefs.language,
            "UV editor: drag islands to move, corners to scale, the round handle to turn",
        )
        .into();
    }

    /// Bring the islands up to date with the garment and its layout.
    fn refresh_uv_cache(&mut self) {
        let template = &self.project.template;
        let garment = &self.scene.garment;
        let key = cache_key(template, garment);
        let editor = &mut self.authoring.uv_editor;
        if editor.cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let shapes = uv_layout::shapes(garment);
        let resolved = uv_layout::resolve_shapes(template, &shapes);
        let placed: Vec<PlacedIsland> = shapes
            .iter()
            .zip(&resolved)
            .map(|(s, p)| PlacedIsland::new(s, p))
            .collect();
        let overlapping = uv_layout::overlaps(&placed)
            .into_iter()
            .flat_map(|(a, b)| [a, b])
            .collect();
        let outside = placed
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.inside_square())
            .map(|(k, _)| k)
            .collect();
        editor.cache = Some(Cache {
            key,
            shapes,
            resolved,
            overlapping,
            outside,
        });
        // Pieces gone from the garment leave the selection.
        let ids: BTreeSet<&str> = garment.panels.iter().map(|p| p.id.as_str()).collect();
        editor.selected.retain(|id| ids.contains(id.as_str()));
    }

    /// Every piece's placement as the layout stands, by id.
    fn uv_layout_now(&self) -> BTreeMap<String, UvPlacement> {
        let Some(cache) = &self.authoring.uv_editor.cache else {
            return BTreeMap::new();
        };
        cache
            .shapes
            .iter()
            .zip(&cache.resolved)
            .map(|(s, p)| (s.id.clone(), *p))
            .collect()
    }

    /// UV › Pack UV islands: every island packed into the square with the editor's options.
    pub(super) fn pack_uv_islands(&mut self) -> Result<(), String> {
        let options = self.authoring.uv_editor.pack;
        let layout = uv_layout::pack_layout(&self.project.template, &self.scene.garment, &options)?;
        if self.edit(Command::SetUvLayout {
            layout,
            name: "Pack UV islands",
        }) {
            self.status = i18n::tr(self.prefs.language, "UV islands packed").into();
        }
        Ok(())
    }

    /// UV › Automatic UV layout: forget the edited placements.
    pub(super) fn automatic_uv_layout(&mut self) {
        if self.project.template.uv_layout.is_empty() {
            self.status =
                i18n::tr(self.prefs.language, "The UV layout is automatic already").into();
            return;
        }
        if self.edit(Command::SetUvLayout {
            layout: BTreeMap::new(),
            name: "Automatic UV layout",
        }) {
            self.status = i18n::tr(self.prefs.language, "UV layout back to automatic").into();
        }
    }

    pub(super) fn uv_editor_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.uv_editor.open {
            return;
        }
        self.refresh_uv_cache();
        // An outside change of the selected piece selects its island.
        if self.authoring.selected != self.authoring.uv_editor.app_selection {
            self.authoring.uv_editor.app_selection = self.authoring.selected.clone();
            if let Some(id) = &self.authoring.selected
                && self.authoring.uv_editor.drag.is_none()
            {
                self.authoring.uv_editor.selected = BTreeSet::from([id.clone()]);
            }
        }
        let lang = self.prefs.language;
        let t = |text| i18n::tr(lang, text);
        let mut open = true;
        let mut command: Option<Command> = None;
        let mut pack = false;
        let mut app_select: Option<Option<String>> = None;
        let layout_now = self.uv_layout_now();
        let mut editor = std::mem::take(&mut self.authoring.uv_editor);
        let template = &self.project.template;
        let garment = &self.scene.garment;
        let appearances = fashioncraft::garment::appearance::bind(template, garment);
        let names: BTreeMap<&str, &str> = template
            .panels
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        let dark = ctx.style_of(ctx.theme()).visuals.dark_mode;
        let islands = editor.cache.take();
        egui::Window::new(t("UV editor"))
            // The same window in every interface language.
            .id(egui::Id::new("fashioncraft.uv_editor"))
            .open(&mut open)
            .default_size([560., 660.])
            .resizable(true)
            .show(ctx, |ui| {
                let Some(cache) = islands.as_ref() else {
                    return;
                };
                // Toolbar: packing and the view.
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(t("Pack"))
                        .on_hover_text(t(
                            "Fit every island into the square without overlap, a margin apart",
                        ))
                        .clicked()
                    {
                        pack = true;
                    }
                    ui.label(t("Margin"));
                    let mut margin = editor.pack.margin * 100.;
                    if ui
                        .add(
                            egui::DragValue::new(&mut margin)
                                .range(0.0..=uv_layout::MAX_MARGIN * 100.)
                                .speed(0.05)
                                .suffix(" %"),
                        )
                        .changed()
                    {
                        editor.pack.margin = margin / 100.;
                    }
                    ui.checkbox(&mut editor.pack.rotate, t("Allow quarter turns"));
                    ui.checkbox(
                        &mut editor.pack.keep_relative_scale,
                        t("Keep relative sizes"),
                    );
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !template.uv_layout.is_empty(),
                            egui::Button::new(t("Automatic layout")),
                        )
                        .on_hover_text(t(
                            "Forget the edited placements and lay the islands out automatically",
                        ))
                        .clicked()
                    {
                        command = Some(Command::SetUvLayout {
                            layout: BTreeMap::new(),
                            name: "Automatic UV layout",
                        });
                    }
                    let turn_selected = |degrees: f64| -> Option<Command> {
                        let mut layout = layout_now.clone();
                        let pivot = selection_center(cache, &layout, &editor.selected)?;
                        for id in &editor.selected {
                            let p = layout.get_mut(id)?;
                            p.center = pivot + uv_layout::rotate(p.center - pivot, degrees);
                            p.rotation_degrees =
                                uv_layout::normalize_degrees(p.rotation_degrees + degrees);
                        }
                        Some(Command::SetUvLayout {
                            layout,
                            name: "Turn UV islands",
                        })
                    };
                    let any = !editor.selected.is_empty();
                    if ui.add_enabled(any, egui::Button::new("⟲ 90°")).clicked() {
                        command = turn_selected(-90.);
                    }
                    if ui.add_enabled(any, egui::Button::new("⟳ 90°")).clicked() {
                        command = turn_selected(90.);
                    }
                    if ui.button(t("Fit view")).clicked() {
                        editor.view_center = DVec2::splat(0.5);
                        editor.zoom = 1.;
                    }
                });
                // The canvas.
                let available = ui.available_size();
                let height = (available.y - 96.).max(220.);
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(available.x.max(220.), height),
                    egui::Sense::click_and_drag(),
                );
                let rect = response.rect;
                let fit = ((rect.width().min(rect.height()) - 2. * PAD).max(40.)) as f64;
                let view = View {
                    rect,
                    center: editor.view_center,
                    pixels: fit * editor.zoom as f64,
                };
                let layout: BTreeMap<String, UvPlacement> =
                    editor.preview.clone().unwrap_or_else(|| layout_now.clone());
                let placed: Vec<PlacedIsland> = cache
                    .shapes
                    .iter()
                    .map(|s| PlacedIsland::new(s, &layout[&s.id]))
                    .collect();
                // Interaction.
                let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                let pointer = response.interact_pointer_pos().or(response.hover_pos());
                let handles = selection_bounds(&placed, &editor.selected).map(|(lo, hi)| {
                    let (a, b) = (view.screen(lo), view.screen(hi));
                    let corners = [
                        egui::pos2(a.x, a.y),
                        egui::pos2(b.x, a.y),
                        egui::pos2(b.x, b.y),
                        egui::pos2(a.x, b.y),
                    ];
                    let turn = egui::pos2((a.x + b.x) / 2., a.y - TURN_HANDLE);
                    ((lo + hi) / 2., corners, turn)
                });
                let hit = |pos: egui::Pos2| -> Option<usize> {
                    let q = view.texture(pos);
                    (0..placed.len()).rev().find(|k| placed[*k].contains(q))
                };
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pos) = pointer
                {
                    let start = view.texture(pos);
                    let near = |p: egui::Pos2| p.distance(pos) <= HANDLE_RADIUS + 4.;
                    let handle = match handles {
                        Some((_, _, turn)) if near(turn) => Some(Handle::Turn),
                        Some((_, corners, _)) if corners.iter().any(|c| near(*c)) => {
                            Some(Handle::Scale)
                        }
                        _ => None,
                    };
                    let handle = handle.unwrap_or_else(|| match hit(pos) {
                        Some(k) => {
                            let id = &placed[k].shape.id;
                            if !editor.selected.contains(id) {
                                if !shift {
                                    editor.selected.clear();
                                }
                                editor.selected.insert(id.clone());
                                app_select = Some(Some(id.clone()));
                            }
                            Handle::Move
                        }
                        None => Handle::Box,
                    });
                    let pivot = selection_bounds(&placed, &editor.selected)
                        .map(|(lo, hi)| (lo + hi) / 2.)
                        .unwrap_or(start);
                    let before = editor
                        .selected
                        .iter()
                        .filter_map(|id| Some((id.clone(), *layout.get(id)?)))
                        .collect();
                    editor.drag = Some(UvDrag {
                        handle,
                        start,
                        before,
                        pivot,
                    });
                }
                if (response.drag_started_by(egui::PointerButton::Middle)
                    || response.drag_started_by(egui::PointerButton::Secondary))
                    && let Some(pos) = pointer
                {
                    editor.drag = Some(UvDrag {
                        handle: Handle::Pan,
                        start: view.texture(pos),
                        before: BTreeMap::new(),
                        pivot: DVec2::ZERO,
                    });
                }
                let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if cancelled && editor.drag.is_some() {
                    editor.drag = None;
                    editor.preview = None;
                }
                if let Some(drag) = &editor.drag
                    && let Some(pos) = pointer
                    && response.dragged()
                {
                    let now = view.texture(pos);
                    match drag.handle {
                        Handle::Pan => {
                            let delta = response.drag_delta();
                            editor.view_center -=
                                DVec2::new(delta.x as f64, delta.y as f64) / view.pixels;
                        }
                        Handle::Box => {}
                        Handle::Move | Handle::Scale | Handle::Turn => {
                            let mut next = layout_now.clone();
                            for (id, before) in &drag.before {
                                let mut p = *before;
                                match drag.handle {
                                    Handle::Move => p.center = before.center + (now - drag.start),
                                    Handle::Scale => {
                                        let from = (drag.start - drag.pivot).length();
                                        let to = (now - drag.pivot).length();
                                        let factor = if from > 1e-9 {
                                            (to / from).clamp(0.02, 50.)
                                        } else {
                                            1.
                                        };
                                        p.center =
                                            drag.pivot + (before.center - drag.pivot) * factor;
                                        p.scale = (before.scale * factor)
                                            .clamp(1e-6, uv_layout::MAX_SCALE);
                                    }
                                    _ => {
                                        let angle = |q: DVec2| {
                                            let d = q - drag.pivot;
                                            d.y.atan2(d.x).to_degrees()
                                        };
                                        let mut degrees = angle(now) - angle(drag.start);
                                        if ui.input(|i| i.modifiers.shift) {
                                            degrees = (degrees / 15.).round() * 15.;
                                        }
                                        p.center = drag.pivot
                                            + uv_layout::rotate(
                                                before.center - drag.pivot,
                                                degrees,
                                            );
                                        p.rotation_degrees = uv_layout::normalize_degrees(
                                            before.rotation_degrees + degrees,
                                        );
                                    }
                                }
                                next.insert(id.clone(), p);
                            }
                            editor.preview = Some(next);
                        }
                    }
                }
                if response.drag_stopped()
                    && let Some(drag) = editor.drag.take()
                {
                    match drag.handle {
                        Handle::Box => {
                            if let Some(pos) = pointer {
                                let now = view.texture(pos);
                                let (lo, hi) = (drag.start.min(now), drag.start.max(now));
                                if !shift {
                                    editor.selected.clear();
                                }
                                for island in &placed {
                                    let c = (island.lo + island.hi) / 2.;
                                    if c.cmpge(lo).all() && c.cmple(hi).all() {
                                        editor.selected.insert(island.shape.id.clone());
                                    }
                                }
                            }
                        }
                        Handle::Pan => {}
                        handle => {
                            if let Some(layout) = editor.preview.take()
                                && layout != layout_now
                            {
                                command = Some(Command::SetUvLayout {
                                    layout,
                                    name: match handle {
                                        Handle::Move => "Move UV islands",
                                        Handle::Scale => "Scale UV islands",
                                        _ => "Turn UV islands",
                                    },
                                });
                            }
                        }
                    }
                    editor.preview = None;
                }
                if response.clicked_by(egui::PointerButton::Primary)
                    && let Some(pos) = pointer
                {
                    match hit(pos) {
                        Some(k) => {
                            let id = placed[k].shape.id.clone();
                            if shift {
                                if !editor.selected.remove(&id) {
                                    editor.selected.insert(id.clone());
                                }
                            } else {
                                editor.selected = BTreeSet::from([id.clone()]);
                            }
                            app_select = Some(Some(id));
                        }
                        None if !shift => editor.selected.clear(),
                        None => {}
                    }
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.
                        && let Some(pos) = response.hover_pos()
                    {
                        let anchor = view.texture(pos);
                        let zoom = (editor.zoom * (scroll * 0.004).exp()).clamp(0.2, 40.);
                        // Keep the texture point under the pointer where it is.
                        let pixels = fit * zoom as f64;
                        let d = pos - rect.center();
                        editor.view_center = anchor - DVec2::new(d.x as f64, d.y as f64) / pixels;
                        editor.zoom = zoom;
                    }
                }
                // Drawing: what is being dragged, or what was just committed.
                let committed = match &command {
                    Some(Command::SetUvLayout { layout, .. }) => Some(layout),
                    _ => None,
                };
                let drawn = editor.preview.as_ref().or(committed);
                let placed: Vec<PlacedIsland> = cache
                    .shapes
                    .iter()
                    .map(|s| {
                        let p = drawn
                            .and_then(|d| d.get(&s.id))
                            .unwrap_or(&layout_now[&s.id]);
                        PlacedIsland::new(s, p)
                    })
                    .collect();
                let view = View {
                    rect,
                    center: editor.view_center,
                    pixels: fit * editor.zoom as f64,
                };
                let painter = painter.with_clip_rect(rect);
                let ink = if dark {
                    egui::Color32::from_gray(200)
                } else {
                    egui::Color32::from_gray(60)
                };
                painter.rect_filled(
                    rect,
                    0.,
                    if dark {
                        egui::Color32::from_gray(28)
                    } else {
                        egui::Color32::from_gray(236)
                    },
                );
                let square =
                    egui::Rect::from_two_pos(view.screen(DVec2::ZERO), view.screen(DVec2::ONE));
                painter.rect_filled(
                    square,
                    0.,
                    if dark {
                        egui::Color32::from_gray(42)
                    } else {
                        egui::Color32::WHITE
                    },
                );
                for k in 1..10 {
                    let f = k as f64 / 10.;
                    let grid = egui::Stroke::new(1., ink.gamma_multiply(0.12));
                    painter.line_segment(
                        [
                            view.screen(DVec2::new(f, 0.)),
                            view.screen(DVec2::new(f, 1.)),
                        ],
                        grid,
                    );
                    painter.line_segment(
                        [
                            view.screen(DVec2::new(0., f)),
                            view.screen(DVec2::new(1., f)),
                        ],
                        grid,
                    );
                }
                painter.rect_stroke(
                    square,
                    0.,
                    egui::Stroke::new(1.5, ink.gamma_multiply(0.7)),
                    egui::StrokeKind::Outside,
                );
                let accent = ui.visuals().selection.stroke.color;
                let dragging = editor.preview.is_some();
                for (k, island) in placed.iter().enumerate() {
                    let id = &island.shape.id;
                    let range = &garment.panels[k];
                    let color = appearances[range.material]
                        .as_ref()
                        .map_or([200, 204, 212], |a| a.color_srgb);
                    let fill =
                        egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 190);
                    let mut mesh = egui::Mesh::default();
                    for p in &island.uv {
                        mesh.colored_vertex(view.screen(*p), fill);
                    }
                    for tri in &island.shape.triangles {
                        mesh.add_triangle(tri[0] as u32, tri[1] as u32, tri[2] as u32);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    let selected = editor.selected.contains(id);
                    let flagged = !dragging && cache.overlapping.contains(&k);
                    let outside = !dragging && cache.outside.contains(&k);
                    let stroke = if selected {
                        egui::Stroke::new(2., accent)
                    } else if flagged {
                        egui::Stroke::new(2., egui::Color32::from_rgb(220, 50, 47))
                    } else if outside {
                        egui::Stroke::new(2., egui::Color32::from_rgb(230, 150, 30))
                    } else {
                        egui::Stroke::new(1., ink.gamma_multiply(0.8))
                    };
                    for [a, b] in &island.shape.outline {
                        painter.line_segment(
                            [view.screen(island.uv[*a]), view.screen(island.uv[*b])],
                            stroke,
                        );
                    }
                    let name = names.get(id.as_str()).copied().unwrap_or(id.as_str());
                    painter.text(
                        view.screen((island.lo + island.hi) / 2.),
                        egui::Align2::CENTER_CENTER,
                        name,
                        egui::FontId::proportional(11.),
                        ink,
                    );
                }
                // The selection's bounds and handles.
                if let Some((lo, hi)) = selection_bounds(&placed, &editor.selected) {
                    let (a, b) = (view.screen(lo), view.screen(hi));
                    let frame = egui::Rect::from_two_pos(a, b);
                    painter.rect_stroke(
                        frame,
                        0.,
                        egui::Stroke::new(1., accent.gamma_multiply(0.8)),
                        egui::StrokeKind::Outside,
                    );
                    for c in [
                        frame.left_top(),
                        frame.right_top(),
                        frame.right_bottom(),
                        frame.left_bottom(),
                    ] {
                        painter.rect_filled(
                            egui::Rect::from_center_size(
                                c,
                                egui::vec2(2. * HANDLE_RADIUS, 2. * HANDLE_RADIUS) * 0.8,
                            ),
                            1.,
                            accent,
                        );
                    }
                    let top = egui::pos2(frame.center().x, frame.top());
                    let turn = top - egui::vec2(0., TURN_HANDLE);
                    painter.line_segment([top, turn], egui::Stroke::new(1., accent));
                    painter.circle_filled(turn, HANDLE_RADIUS, accent);
                }
                // A box selection being dragged.
                if let Some(drag) = &editor.drag
                    && drag.handle == Handle::Box
                    && let Some(pos) = pointer
                {
                    let r = egui::Rect::from_two_pos(view.screen(drag.start), pos);
                    painter.rect_filled(r, 0., accent.gamma_multiply(0.08));
                    painter.rect_stroke(
                        r,
                        0.,
                        egui::Stroke::new(1., accent),
                        egui::StrokeKind::Inside,
                    );
                }
                // The selected island in numbers.
                let single = (editor.selected.len() == 1)
                    .then(|| editor.selected.first().cloned())
                    .flatten();
                ui.horizontal(|ui| {
                    match single
                        .as_ref()
                        .and_then(|id| Some((id, layout_now.get(id)?)))
                    {
                        Some((id, placement)) => {
                            let name = names.get(id.as_str()).copied().unwrap_or(id.as_str());
                            ui.strong(name);
                            let commit = |r: &egui::Response| {
                                r.drag_stopped() || (r.changed() && !r.dragged())
                            };
                            let mut p = *placement;
                            let mut changed = false;
                            ui.label("U");
                            changed |= commit(
                                &ui.add(
                                    egui::DragValue::new(&mut p.center.x)
                                        .speed(0.002)
                                        .max_decimals(4),
                                ),
                            );
                            ui.label("V");
                            changed |= commit(
                                &ui.add(
                                    egui::DragValue::new(&mut p.center.y)
                                        .speed(0.002)
                                        .max_decimals(4),
                                ),
                            );
                            ui.label(t("Turn"));
                            changed |= commit(
                                &ui.add(
                                    egui::DragValue::new(&mut p.rotation_degrees)
                                        .range(-180.0..=180.0)
                                        .speed(0.5)
                                        .suffix("°"),
                                ),
                            );
                            ui.label(t("Scale"));
                            let scale = ui
                                .add(
                                    egui::DragValue::new(&mut p.scale)
                                        .range(1e-4..=uv_layout::MAX_SCALE)
                                        .speed(0.005)
                                        .max_decimals(4),
                                )
                                .on_hover_text(t("Texture units per metre of material"));
                            changed |= commit(&scale);
                            if changed && p != *placement {
                                let mut layout = layout_now.clone();
                                layout.insert(id.clone(), p);
                                command = Some(Command::SetUvLayout {
                                    layout,
                                    name: "Edit UV island",
                                });
                            }
                        }
                        None => {
                            ui.small(t(
                                "Click an island to select it; Shift adds to the selection",
                            ));
                        }
                    }
                });
                // What the layout is like.
                let mut summary = format!("{} {}", cache.shapes.len(), t("pieces"));
                if template.uv_layout.is_empty() {
                    summary.push_str(&format!(" · {}", t("automatic layout")));
                }
                if !cache.overlapping.is_empty() {
                    summary.push_str(&format!(
                        " · {} {}",
                        cache.overlapping.len(),
                        t("overlapping")
                    ));
                }
                if !cache.outside.is_empty() {
                    summary.push_str(&format!(
                        " · {} {}",
                        cache.outside.len(),
                        t("outside the square")
                    ));
                }
                if !cache.resolved.is_empty() {
                    // Texel density of a 2048-pixel image, as most islands have it.
                    let mut scales: Vec<f64> = cache.resolved.iter().map(|p| p.scale).collect();
                    scales.sort_by(f64::total_cmp);
                    let density = scales[scales.len() / 2] * 2048. / 100.;
                    summary.push_str(&format!(" · {density:.1} {}", t("px/cm at 2048 px")));
                }
                ui.small(summary);
            });
        editor.cache = islands;
        self.authoring.uv_editor = editor;
        if let Some(selection) = app_select {
            self.authoring.selected = selection.clone();
            self.authoring.uv_editor.app_selection = selection;
        }
        if pack && let Err(error) = self.pack_uv_islands() {
            self.status = error;
        }
        if let Some(command) = command {
            self.edit(command);
        }
        if !open {
            self.authoring.uv_editor.open = false;
            self.authoring.uv_editor.drag = None;
            self.authoring.uv_editor.preview = None;
        }
    }
}

/// Centre of the selected islands' bounds in `layout`.
fn selection_center(
    cache: &Cache,
    layout: &BTreeMap<String, UvPlacement>,
    selected: &BTreeSet<String>,
) -> Option<DVec2> {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for shape in &cache.shapes {
        if selected.contains(&shape.id) {
            let (a, b) = uv_layout::placed_bounds(shape, layout.get(&shape.id)?);
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    lo.is_finite().then_some((lo + hi) / 2.)
}

/// Bounds of the selected islands, texture units.
fn selection_bounds(
    placed: &[PlacedIsland],
    selected: &BTreeSet<String>,
) -> Option<(DVec2, DVec2)> {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for island in placed {
        if selected.contains(&island.shape.id) {
            lo = lo.min(island.lo);
            hi = hi.max(island.hi);
        }
    }
    lo.is_finite().then_some((lo, hi))
}
