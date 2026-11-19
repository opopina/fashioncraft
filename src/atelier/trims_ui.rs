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

    /// A 15 mm button, or an 18 mm buttonhole, at the middle of a piece.
    pub(super) fn centre_trim(&self, panel: &str, buttonhole: bool) -> Option<Command> {
        let c = self.piece_centre(panel)?;
        let kind = if buttonhole {
            TrimKind::Buttonhole {
                start_m: c - DVec2::new(0.009, 0.),
                end_m: c + DVec2::new(0.009, 0.),
                width_m: 0.004,
            }
        } else {
            TrimKind::Button {
                center_m: c,
                diameter_m: 0.015,
            }
        };
        Some(Command::SetTrim {
            trim: Trim {
                id: self.new_trim_id(),
                panel_id: panel.into(),
                kind,
            },
        })
    }

    /// "Trims" in the Piece inspector.
    pub(super) fn piece_trims(&mut self, ui: &mut egui::Ui, panel: &str) {
        let ui_lang = self.prefs.language;
        let mine: Vec<Trim> = self
            .project
            .template
            .trims
            .iter()
            .filter(|t| t.panel_id == panel)
            .cloned()
            .collect();
        let mut command = None;
        egui::CollapsingHeader::new(super::workspace::section(format!(
            "{} ({})",
            i18n::tr(ui_lang, "Trims"),
            mine.len()
        )))
        .id_salt("piece_trims")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let centre = self.piece_centre(panel);
                if ui
                    .add_enabled(
                        centre.is_some(),
                        egui::Button::new(i18n::tr(ui_lang, "Add button")),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "A 15 mm button at the middle of the piece",
                    ))
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
                    .add_enabled(
                        centre.is_some(),
                        egui::Button::new(i18n::tr(ui_lang, "Add buttonhole")),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "An 18 mm buttonhole at the middle of the piece",
                    ))
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
                ui.small(i18n::tr(
                    ui_lang,
                    "Topstitching and zippers go along a selected segment.",
                ));
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
                    if ui.small_button(i18n::tr(ui_lang, "Remove")).clicked() {
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
        let ui_lang = self.prefs.language;
        let mut zipper = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(i18n::tr(ui_lang, "Topstitch"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "A row of topstitching 4 mm inside this segment",
                ))
                .clicked()
            {
                zipper = Some(false);
            }
            if ui
                .button(i18n::tr(ui_lang, "Zipper"))
                .on_hover_text(i18n::tr(ui_lang, "An 8 mm zipper 6 mm inside this segment"))
                .clicked()
            {
                zipper = Some(true);
            }
        });
        if let Some(zipper) = zipper {
            self.add_segment_trim(panel, edge, zipper);
        }
    }

    /// A row of topstitching 4 mm, or an 8 mm zipper 6 mm, inside a contour segment.
    pub(super) fn add_segment_trim(&mut self, panel: &str, edge: &str, zipper: bool) -> bool {
        let Some(polygon) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.clone())
        else {
            self.status = "The piece has no outline".into();
            return false;
        };
        let inside = |d: f64| {
            lines2d::offset_lines(&polygon, edge, 1, d, false, false)
                .ok()
                .and_then(|mut lines| lines.pop())
        };
        let kind = if zipper {
            inside(0.006).map(|path_m| TrimKind::Zipper {
                path_m,
                width_m: 0.008,
            })
        } else {
            inside(0.004).map(|path_m| TrimKind::Topstitch {
                path_m,
                rows: 1,
                spacing_m: 0.002,
            })
        };
        let Some(kind) = kind else {
            self.status = "This segment is too short or too curved for a trim inside it".into();
            return false;
        };
        let trim = Trim {
            id: self.new_trim_id(),
            panel_id: panel.into(),
            kind,
        };
        self.edit(Command::SetTrim { trim })
    }

    /// Pattern 2D › Edit › Add point / split line: a new point at the middle of the selected
    /// segment, which becomes two.
    pub(super) fn split_selected_segment(&mut self) -> Result<(), String> {
        let edge = self
            .authoring
            .edge
            .clone()
            .filter(|e| e.edge.is_some())
            .ok_or("Select a segment or an internal line first")?;
        let length = self
            .sewing_curve(std::slice::from_ref(&edge), false)
            .map(|c| c.length)
            .filter(|l| *l > 0.)
            .ok_or("The selected segment has no length")?;
        if !self.edit(Command::SplitBoundary {
            panel: edge.panel.clone(),
            edge: edge.edge.clone().unwrap_or_default(),
            at_m: length * 0.5,
        }) {
            return Err(self.status.clone());
        }
        self.status = format!(
            "Point added at the middle of the segment ({:.1} mm from each end)",
            length * 500.
        );
        Ok(())
    }

    /// Pattern 2D › Edit › Smooth curve: every selected outline point becomes a smooth one,
    /// with collinear handles, as "Round this corner" does for one point.
    pub(super) fn smooth_selected_points(&mut self) -> Result<(), String> {
        let groups: Vec<(String, Vec<usize>)> = self
            .authoring
            .board
            .point_selections()
            .into_iter()
            .filter(|(_, points)| !points.is_empty())
            .collect();
        if groups.is_empty() {
            return Err("Select outline points on the board first (Points mode)".into());
        }
        let mut rounded = 0;
        for (panel, points) in groups {
            for anchor in points {
                let curved = self
                    .project
                    .template
                    .panels
                    .iter()
                    .find(|p| p.id == panel)
                    .and_then(|p| p.polygon.as_ref())
                    .is_some_and(|poly| {
                        let n = poly.points.len();
                        anchor < n
                            && poly.curves.contains_key(&poly.edges[anchor])
                            && poly.curves.contains_key(&poly.edges[(anchor + n - 1) % n])
                    });
                if !curved
                    && self.edit(Command::CurvePoint {
                        panel: panel.clone(),
                        anchor,
                        curved: true,
                    })
                {
                    rounded += 1;
                }
            }
        }
        self.status = format!(
            "{} rounded into a smooth curve",
            super::garment_display::counted(rounded, "point", "points")
        );
        Ok(())
    }

    /// The seam picked in the seam list, or else the one on the selected segment.
    fn chosen_seam(&self) -> Option<marvelous_cloth::garment::seam::SeamSpec> {
        let id = self
            .authoring
            .selection
            .seam
            .clone()
            .or_else(|| self.edge_seams().into_iter().next())?;
        self.project
            .template
            .seams
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// The whole segments a seam joins, as (piece, segment); parts of segments are left out.
    fn seam_segments(
        seam: &marvelous_cloth::garment::seam::SeamSpec,
    ) -> (Vec<(String, String)>, usize) {
        let mut whole = vec![];
        let mut partial = 0;
        for r in seam.references() {
            match &r.edge {
                Some(edge) if r.range[0] <= 1e-9 && r.range[1] >= 1. - 1e-9 => {
                    whole.push((r.panel.clone(), edge.clone()));
                }
                _ => partial += 1,
            }
        }
        (whole, partial)
    }

    /// Materials › Seamline topstitch and Seam taping: topstitching, or a tape, along both
    /// sides of the selected seam.
    pub(super) fn finish_selected_seam(&mut self, tape: bool) -> Result<(), String> {
        let seam = self
            .chosen_seam()
            .ok_or("Select a seam (or a sewn segment) first")?;
        let (segments, partial) = Self::seam_segments(&seam);
        if segments.is_empty() {
            return Err(
                "This seam joins parts of segments only; split them at the seam's ends first"
                    .into(),
            );
        }
        let mut done = 0;
        for (panel, edge) in &segments {
            let ok = if tape {
                self.edit(Command::SetLineSpring {
                    panel: panel.clone(),
                    line: edge.clone(),
                    spring: Some(marvelous_cloth::garment::template::LineSpring::tape(
                        panel, edge,
                    )),
                })
            } else {
                self.add_segment_trim(panel, edge, false)
            };
            done += usize::from(ok);
        }
        self.status = format!(
            "{} along {} of seam {}{}",
            if tape { "Tape" } else { "Topstitching" },
            super::garment_display::counted(done, "segment", "segments"),
            seam.id,
            if partial > 0 {
                " · parts of segments were left out"
            } else {
                ""
            }
        );
        Ok(())
    }

    /// Materials › Tape: a tape along the selected segment or internal line, so it does not
    /// stretch.
    pub(super) fn tape_selected_line(&mut self) -> Result<(), String> {
        let edge = self
            .authoring
            .edge
            .clone()
            .ok_or("Select a segment or an internal line first")?;
        let line = edge
            .edge
            .clone()
            .ok_or("Select a segment or an internal line first")?;
        let taped = self.project.template.line_springs.iter().any(|s| {
            s.panel == edge.panel
                && s.line == line
                && s.kind == marvelous_cloth::garment::template::LineKind::Tape
        });
        if taped {
            return Err("This line is taped already".into());
        }
        if !self.edit(Command::SetLineSpring {
            panel: edge.panel.clone(),
            line: line.clone(),
            spring: Some(marvelous_cloth::garment::template::LineSpring::tape(
                &edge.panel,
                &line,
            )),
        }) {
            return Err(self.status.clone());
        }
        self.status = format!("Tape along {line}: it no longer stretches");
        Ok(())
    }

    /// Materials › Free topstitch: topstitching along the selected open internal line.
    pub(super) fn topstitch_selected_line(&mut self) -> Result<(), String> {
        let edge = self
            .authoring
            .edge
            .clone()
            .ok_or("Select an internal line first")?;
        let name = edge.edge.clone().ok_or("Select an internal line first")?;
        let polygon = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == edge.panel)
            .and_then(|p| p.polygon.clone())
            .ok_or("The piece has no outline")?;
        if !polygon.lines.iter().any(|l| l.id == name) {
            return Err(
                "Select an internal line to stitch along (for a segment, use Segment topstitch)"
                    .into(),
            );
        }
        let path_m = lines2d::path(&polygon, &name)?.points;
        let trim = Trim {
            id: self.new_trim_id(),
            panel_id: edge.panel.clone(),
            kind: TrimKind::Topstitch {
                path_m,
                rows: 1,
                spacing_m: 0.002,
            },
        };
        if !self.edit(Command::SetTrim { trim }) {
            return Err(self.status.clone());
        }
        self.status = format!("Topstitching along {name}");
        Ok(())
    }

    /// The outline segment selected on the board, as (piece, segment name).
    pub(super) fn selected_segment(&self) -> Option<(String, String)> {
        let edge = self.authoring.edge.as_ref()?;
        let name = edge.edge.clone()?;
        let polygon = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == edge.panel)?
            .polygon
            .as_ref()?;
        polygon
            .edges
            .contains(&name)
            .then(|| (edge.panel.clone(), name))
    }
}
