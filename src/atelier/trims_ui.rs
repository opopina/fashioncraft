//! Trims on a piece: add a button or buttonhole at its centre, topstitch or a zipper along
//! the selected segment, list them and remove them. Trims only change the look; each change
//! is one command.
use super::*;
use glam::DVec2;
use marvelous_cloth::{
    authoring::Command,
    garment::{
        lines2d,
        trims::{Trim, TrimKind},
    },
};

impl Atelier {
    /// A trim id not used yet in the document.
    fn new_trim_id(&self) -> String {
        let used = |id: &str| self.project.template.trims.iter().any(|t| t.id == id);
        (1..)
            .map(|k| format!("trim_{k}"))
            .find(|id| !used(id))
            .unwrap()
    }

    /// The middle of a piece in its material coordinates.
    pub(super) fn piece_centre(&self, panel: &str) -> Option<DVec2> {
        let g = &self.scene.garment;
        let r = g.panels.iter().find(|r| r.id == panel)?;
        let uv = &g.mesh.uv[r.vertices.clone()];
        (!uv.is_empty()).then(|| uv.iter().sum::<DVec2>() / uv.len() as f64)
    }

    /// "Trims" in the Piece inspector.
    pub(super) fn piece_trims(&mut self, ui: &mut egui::Ui, panel: &str) {
        let mine: Vec<Trim> = self
            .project
            .template
            .trims
            .iter()
            .filter(|t| t.panel_id == panel)
            .cloned()
            .collect();
        let mut command = None;
        egui::CollapsingHeader::new(format!("Trims ({})", mine.len()))
            .id_salt("piece_trims")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let centre = self.piece_centre(panel);
                    if ui
                        .add_enabled(centre.is_some(), egui::Button::new("Add button"))
                        .on_hover_text("A 15 mm button at the middle of the piece")
                        .clicked()
                        && let Some(c) = centre
                    {
                        command = Some(Command::SetTrim {
                            trim: Trim {
                                id: self.new_trim_id(),
                                panel_id: panel.into(),
                                kind: TrimKind::Button {
                                    center_m: c,
                                    diameter_m: 0.015,
                                },
                            },
                        });
                    }
                    if ui
                        .add_enabled(centre.is_some(), egui::Button::new("Add buttonhole"))
                        .on_hover_text("An 18 mm buttonhole at the middle of the piece")
                        .clicked()
                        && let Some(c) = centre
                    {
                        command = Some(Command::SetTrim {
                            trim: Trim {
                                id: self.new_trim_id(),
                                panel_id: panel.into(),
                                kind: TrimKind::Buttonhole {
                                    start_m: c - DVec2::new(0.009, 0.),
                                    end_m: c + DVec2::new(0.009, 0.),
                                    width_m: 0.004,
                                },
                            },
                        });
                    }
                });
                if mine.is_empty() {
                    ui.small("Topstitching and zippers go along a selected segment.");
                }
                for trim in &mine {
                    ui.horizontal(|ui| {
                        let kind = match &trim.kind {
                            TrimKind::Button { .. } => "Button",
                            TrimKind::Buttonhole { .. } => "Buttonhole",
                            TrimKind::Zipper { .. } => "Zipper",
                            TrimKind::Topstitch { .. } => "Topstitch",
                        };
                        ui.label(format!("{kind} · {}", trim.id));
                        if ui.small_button("Remove").clicked() {
                            command = Some(Command::RemoveTrim {
                                id: trim.id.clone(),
                            });
                        }
                    });
                }
            });
        if let Some(command) = command {
            self.edit(command);
        }
    }

    /// Topstitch or a zipper along the selected contour segment, a little inside it.
    pub(super) fn segment_trims(&mut self, ui: &mut egui::Ui, panel: &str, edge: &str) {
        let Some(polygon) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.clone())
        else {
            return;
        };
        let inside = |d: f64| {
            lines2d::offset_lines(&polygon, edge, 1, d, false, false)
                .ok()
                .and_then(|mut lines| lines.pop())
        };
        let mut kind = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Topstitch")
                .on_hover_text("A row of topstitching 4 mm inside this segment")
                .clicked()
            {
                kind = inside(0.004).map(|path_m| TrimKind::Topstitch {
                    path_m,
                    rows: 1,
                    spacing_m: 0.002,
                });
            }
            if ui
                .button("Zipper")
                .on_hover_text("An 8 mm zipper 6 mm inside this segment")
                .clicked()
            {
                kind = inside(0.006).map(|path_m| TrimKind::Zipper {
                    path_m,
                    width_m: 0.008,
                });
            }
        });
        if let Some(kind) = kind {
            let trim = Trim {
                id: self.new_trim_id(),
                panel_id: panel.into(),
                kind,
            };
            self.edit(Command::SetTrim { trim });
        }
    }
}
