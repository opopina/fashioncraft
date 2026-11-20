//! Edit texture (Materials › Texture › Edit texture 2D / 3D): drag on a piece, on the board or
//! on the cloth in 3D, to move its fabric texture across it; and, in the tool's window, the
//! texture's position on the selected piece, the piece's grain, and the fabric texture's repeat
//! size and turn. Moving the texture changes the look only.
use super::*;
use fashioncraft::authoring::{Command, arrange2d};
use glam::DVec2;

#[derive(Default)]
pub(super) struct TextureEdit {
    pub open: bool,
    /// A drag in progress: the piece, where it started (material metres) and the texture
    /// position then.
    drag: Option<(String, DVec2, DVec2)>,
    /// The texture position being dragged, drawn before it is committed.
    pub preview: Option<(String, DVec2)>,
}

impl Atelier {
    pub(super) fn open_texture_edit(&mut self) {
        self.authoring.texture_edit.open = true;
        self.status = i18n::tr(
            self.prefs.language,
            "Drag on a piece to move its texture · the window sets size, turn and grain",
        )
        .into();
    }

    fn texture_offset(&self, panel: &str) -> DVec2 {
        self.project
            .template
            .texture_offsets
            .get(panel)
            .copied()
            .unwrap_or_default()
    }

    /// Per-triangle texture positions for the renderer, the dragged one included; `None` when
    /// every texture sits where the fabric puts it.
    pub(super) fn texture_offsets_per_face(&self) -> Option<Vec<DVec2>> {
        let template = &self.project.template;
        let preview = self.authoring.texture_edit.preview.as_ref();
        if template.texture_offsets.is_empty() && preview.is_none() {
            return None;
        }
        let garment = &self.scene.garment;
        let mut out = vec![DVec2::ZERO; garment.mesh.triangles.len()];
        for range in &garment.panels {
            let offset = match preview {
                Some((id, offset)) if *id == range.id => *offset,
                _ => self.texture_offset(&range.id),
            };
            out[range.triangles.clone()].fill(offset);
        }
        Some(out)
    }

    /// A drag moved the texture of `panel` from material point `start` to `now`.
    fn drag_texture(&mut self, now: DVec2) {
        if let Some((panel, start, offset)) = &self.authoring.texture_edit.drag {
            self.authoring.texture_edit.preview = Some((panel.clone(), *offset + (now - *start)));
        }
    }

    fn finish_texture_drag(&mut self) {
        self.authoring.texture_edit.drag = None;
        if let Some((panel, offset)) = self.authoring.texture_edit.preview.take()
            && self.edit(Command::SetTextureOffset { panel, offset })
        {
            self.status = i18n::tr(self.prefs.language, "Texture moved").into();
        }
    }

    /// Dragging on a piece on the board while the tool is open.
    pub(super) fn texture_edit_board(
        &mut self,
        rect: egui::Rect,
        response: &egui::Response,
    ) -> bool {
        if !self.authoring.texture_edit.open {
            return false;
        }
        let template = &self.project.template;
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let at = self.authoring.board.world(rect, pos);
            let piece = (0..template.panels.len()).rev().find(|&i| {
                self.board_outline(i)
                    .is_some_and(|outline| super::board_measure::contains(&outline, at))
            });
            if let Some(i) = piece {
                let id = template.panels[i].id.clone();
                let start = arrange2d::material_point(template, i, at);
                self.authoring.selected = Some(id.clone());
                let offset = self.texture_offset(&id);
                self.authoring.texture_edit.drag = Some((id, start, offset));
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some((id, _, _)) = &self.authoring.texture_edit.drag
            && let Some(i) = template.panels.iter().position(|p| &p.id == id)
        {
            let at = self.authoring.board.world(rect, pos);
            let now = arrange2d::material_point(template, i, at);
            self.drag_texture(now);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.finish_texture_drag();
        }
        true
    }

    /// Dragging on the cloth in the 3D view while the tool is open.
    pub(super) fn texture_edit_3d(&mut self, response: &egui::Response, rect: egui::Rect) -> bool {
        if !self.authoring.texture_edit.open {
            return false;
        }
        let material = |this: &Self, pos: egui::Pos2| -> Option<(String, DVec2)> {
            let v = this.picked_vertex(pos, rect)?;
            let g = &this.scene.garment;
            let range = g.panels.iter().find(|r| r.vertices.contains(&v))?;
            Some((range.id.clone(), g.mesh.uv[v]))
        };
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some((id, start)) = material(self, pos)
        {
            self.authoring.selected = Some(id.clone());
            let offset = self.texture_offset(&id);
            self.authoring.texture_edit.drag = Some((id, start, offset));
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some((id, now)) = material(self, pos)
            && self
                .authoring
                .texture_edit
                .drag
                .as_ref()
                .is_some_and(|(piece, _, _)| *piece == id)
        {
            self.drag_texture(now);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.finish_texture_drag();
        }
        // Clicks still select; drags belong to the tool.
        response.dragged() || response.drag_stopped()
    }

    pub(super) fn texture_edit_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.texture_edit.open {
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        let mut open = true;
        let mut command: Option<Command> = None;
        let panel = self.authoring.selected.clone().and_then(|id| {
            self.project
                .template
                .panels
                .iter()
                .find(|p| p.id == id)
                .cloned()
        });
        egui::Window::new(t("Edit texture"))
            .open(&mut open)
            .default_width(300.)
            .show(ctx, |ui| {
                let Some(panel) = &panel else {
                    ui.label(t("Select a piece, or drag on one, to move its texture"));
                    return;
                };
                ui.strong(&panel.name);
                let commit = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
                // Texture position on this piece.
                let mut offset = self.texture_offset(&panel.id) * 1000.;
                let mut moved = false;
                ui.horizontal(|ui| {
                    ui.label(t("Texture position"));
                    for value in [&mut offset.x, &mut offset.y] {
                        let r = ui.add(egui::DragValue::new(value).speed(0.5).suffix(" mm"));
                        moved |= commit(&r);
                    }
                    if ui.small_button(t("Reset")).clicked() {
                        offset = DVec2::ZERO;
                        moved = true;
                    }
                });
                if moved {
                    command = Some(Command::SetTextureOffset {
                        panel: panel.id.clone(),
                        offset: offset / 1000.,
                    });
                }
                // The piece's grain.
                let mut grain = panel.grain_degrees;
                ui.horizontal(|ui| {
                    ui.label(t("Grain of this piece"));
                    let r = ui
                        .add(
                            egui::DragValue::new(&mut grain)
                                .range(-180.0..=180.0)
                                .speed(0.5)
                                .suffix("°"),
                        )
                        .on_hover_text(t(
                            "Turns the warp and weft of this piece, and its texture with them",
                        ));
                    if commit(&r) && (grain - panel.grain_degrees).abs() > 1e-9 {
                        command = Some(Command::SetPanelGrain {
                            panel: panel.id.clone(),
                            degrees: grain,
                        });
                    }
                });
                // The fabric texture's repeat and turn.
                let fabric = panel.fabric.clone();
                let appearance = self
                    .project
                    .template
                    .fabric_appearance
                    .get(&fabric)
                    .cloned();
                match appearance.as_ref().and_then(|a| a.texture.clone()) {
                    Some(texture) => {
                        let mut edited = texture.clone();
                        let mut changed = false;
                        ui.label(format!("{} · {fabric}", t("Fabric texture")));
                        ui.horizontal(|ui| {
                            ui.label(t("Repeat"));
                            for value in [&mut edited.width_mm, &mut edited.height_mm] {
                                let r = ui.add(
                                    egui::DragValue::new(value)
                                        .range(1.0..=10_000.0)
                                        .speed(1.0)
                                        .suffix(" mm"),
                                );
                                changed |= commit(&r);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(t("Turn"));
                            let r = ui.add(
                                egui::DragValue::new(&mut edited.rotation_degrees)
                                    .range(-180.0..=180.0)
                                    .speed(0.5)
                                    .suffix("°"),
                            );
                            changed |= commit(&r);
                        });
                        if changed && edited != texture {
                            let mut look = appearance.clone().unwrap_or_default();
                            look.texture = Some(edited);
                            command = Some(Command::FabricAppearance {
                                fabric,
                                appearance: Some(look),
                            });
                        }
                    }
                    None => {
                        ui.small(t(
                            "This fabric has no texture image; add one in the Fabric tab",
                        ));
                    }
                }
            });
        if let Some(command) = command {
            self.edit(command);
        }
        if !open {
            self.authoring.texture_edit.open = false;
            self.authoring.texture_edit.drag = None;
            self.authoring.texture_edit.preview = None;
        }
    }
}
