//! Trims on a piece: add a button or buttonhole at its centre, topstitch or a zipper along
//! the selected segment, list them and remove them. Trims only change the look; each change
//! is one command.
use super::*;
use fashioncraft::{
    authoring::Command,
    garment::{
        lines2d,
        trims::{Trim, TrimKind},
    },
};
use glam::DVec2;

/// Position, size and turn of an object trim; the trim as changed once an edit ends.
fn object_trim_controls(ui: &mut egui::Ui, ui_lang: i18n::Language, trim: &Trim) -> Option<Trim> {
    let TrimKind::Object {
        center_m,
        turn_degrees,
        scale,
        ..
    } = &trim.kind
    else {
        return None;
    };
    let commit = |r: &egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
    let mut centre_mm = *center_m * 1000.;
    let mut turn = *turn_degrees;
    let mut percent = *scale * 100.;
    let mut done = false;
    super::workspace::prop_row(ui, i18n::tr(ui_lang, "Position"), |ui| {
        for value in [&mut centre_mm.x, &mut centre_mm.y] {
            done |= commit(&ui.add(egui::DragValue::new(value).speed(0.5).suffix(" mm")));
        }
    });
    super::workspace::prop_row(ui, i18n::tr(ui_lang, "Size"), |ui| {
        done |= commit(
            &ui.add(
                egui::DragValue::new(&mut percent)
                    .range(1.0..=10_000.0)
                    .speed(0.5)
                    .suffix(" %"),
            ),
        );
        ui.label(i18n::tr(ui_lang, "Turn"));
        done |= commit(
            &ui.add(
                egui::DragValue::new(&mut turn)
                    .range(-180.0..=180.0)
                    .speed(0.5)
                    .suffix("°"),
            ),
        );
    });
    if !done {
        return None;
    }
    let mut changed = trim.clone();
    if let TrimKind::Object {
        center_m,
        turn_degrees,
        scale,
        ..
    } = &mut changed.kind
    {
        *center_m = centre_mm / 1000.;
        *turn_degrees = turn;
        *scale = percent / 100.;
    }
    (changed != *trim && changed.validate().is_ok()).then_some(changed)
}

/// Mesh files a trim object can be opened from.
const TRIM_FILES: &[&str] = &[
    "obj", "gltf", "glb", "vrm", "stl", "ply", "dae", "fbx", "abc", "usda", "usd", "usdz",
];

impl Atelier {
    /// File › Open › Trim: a 3D object from a mesh file (a buckle, a badge, a button), which
    /// the next click on a piece, on the board or on the cloth, fixes there.
    pub(super) fn open_trim(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open trim")
            .add_filter("Mesh", TRIM_FILES)
            .pick_file()
        else {
            return;
        };
        let name = path
            .file_stem()
            .map_or_else(|| "Trim".into(), |s| s.to_string_lossy().into_owned());
        self.load_trim_object(&path, &name);
    }

    /// Ready an object trim from the mesh at `path`, waiting for its place.
    pub(super) fn load_trim_object(&mut self, path: &std::path::Path, name: &str) {
        match fashioncraft::mesh_import::load(path).and_then(|mesh| {
            fashioncraft::garment::trims::object_from_mesh(name, &mesh, DVec2::ZERO)
        }) {
            Ok(kind) => {
                self.authoring.trim_place = Some(kind);
                self.status = i18n::tr(
                    self.prefs.language,
                    "Click a piece, on the board or on the cloth, where the trim goes · Esc cancels",
                )
                .into();
            }
            Err(e) => self.status = e,
        }
    }

    /// Fix the trim waiting for its place at material point `at` of `panel`.
    pub(super) fn place_trim(&mut self, panel: String, at: DVec2) -> bool {
        let Some(mut kind) = self.authoring.trim_place.take() else {
            return false;
        };
        if let TrimKind::Object { center_m, .. } = &mut kind {
            *center_m = at;
        }
        let trim = Trim {
            id: self.new_trim_id(),
            panel_id: panel,
            kind,
        };
        let placed = self.edit(Command::SetTrim { trim });
        if placed {
            self.status = i18n::tr(self.prefs.language, "Trim placed").into();
        }
        placed
    }

    /// Esc while a trim waits for its place.
    fn trim_place_cancelled(&mut self, ctx: &egui::Context) -> bool {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.authoring.trim_place = None;
            self.status = i18n::tr(self.prefs.language, "Trim cancelled").into();
            return true;
        }
        false
    }

    /// While a trim waits for its place, a click on the cloth puts it there.
    pub(super) fn trim_place_3d(&mut self, response: &egui::Response, rect: egui::Rect) -> bool {
        if self.authoring.trim_place.is_none() {
            return false;
        }
        if self.trim_place_cancelled(&response.ctx)
            || !response.clicked_by(egui::PointerButton::Primary)
        {
            return true;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return true;
        };
        match self.picked_cloth_point(pos, rect) {
            Some((panel, uv, _)) => {
                self.place_trim(panel, uv);
            }
            None => self.status = i18n::tr(self.prefs.language, "Click on the cloth").into(),
        }
        true
    }

    /// While a trim waits for its place, a click on a piece on the board puts it there.
    pub(super) fn trim_place_board(&mut self, rect: egui::Rect, response: &egui::Response) -> bool {
        if self.authoring.trim_place.is_none() {
            return false;
        }
        if self.trim_place_cancelled(&response.ctx)
            || !response.clicked_by(egui::PointerButton::Primary)
        {
            return true;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return true;
        };
        let at = self.authoring.board.world(rect, pos);
        let template = &self.project.template;
        let piece = (0..template.panels.len()).rev().find(|&i| {
            self.board_outline(i)
                .is_some_and(|outline| super::board_measure::contains(&outline, at))
        });
        match piece {
            Some(i) => {
                let uv = fashioncraft::authoring::arrange2d::material_point(template, i, at);
                let panel = template.panels[i].id.clone();
                self.place_trim(panel, uv);
            }
            None => self.status = i18n::tr(self.prefs.language, "Click on a piece").into(),
        }
        true
    }

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

    /// A 15 mm button, or an 18 mm buttonhole, at `c` (metres, in the piece's pattern).
    pub(super) fn trim_at(&self, panel: &str, c: DVec2, buttonhole: bool) -> Command {
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
        Command::SetTrim {
            trim: Trim {
                id: self.new_trim_id(),
                panel_id: panel.into(),
                kind,
            },
        }
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
                        TrimKind::Piping { .. } => "Piping",
                        TrimKind::Object { .. } => "Object",
                    };
                    let kind = i18n::tr(ui_lang, kind);
                    match &trim.kind {
                        TrimKind::Object { name, .. } => ui.label(format!("{kind} · {name}")),
                        _ => ui.label(format!("{kind} · {}", trim.id)),
                    };
                    if ui.small_button(i18n::tr(ui_lang, "Remove")).clicked() {
                        command = Some(Command::RemoveTrim {
                            id: trim.id.clone(),
                        });
                    }
                });
                if let Some(changed) = object_trim_controls(ui, ui_lang, trim) {
                    command = Some(Command::SetTrim { trim: changed });
                }
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
    pub(super) fn chosen_seam(&self) -> Option<fashioncraft::garment::seam::SeamSpec> {
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
        seam: &fashioncraft::garment::seam::SeamSpec,
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
                    spring: Some(fashioncraft::garment::template::LineSpring::tape(
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

    /// Materials › Piping: a round cord along the selected segment, or along the first side of
    /// the seam selected in the seam list or the 3D view.
    pub(super) fn pipe_selected(&mut self) -> Result<(), String> {
        /// Diameter of new piping, metres.
        const PIPING_DIAMETER_M: f64 = 0.004;
        /// How far inside the edge the cord's centre runs, metres.
        const PIPING_INSET_M: f64 = 0.004;
        let seam = self.authoring.selection.seam.clone().and_then(|id| {
            self.project
                .template
                .seams
                .iter()
                .find(|s| s.id == id)
                .cloned()
        });
        let targets: Vec<(String, String)> = if let Some(seam) = seam {
            seam.side(false)
                .filter(|r| r.range == [0., 1.])
                .filter_map(|r| Some((r.panel.clone(), r.edge.clone()?)))
                .collect()
        } else {
            let edge = self
                .authoring
                .edge
                .clone()
                .ok_or("Select a seam or a segment first")?;
            vec![(
                edge.panel,
                edge.edge.ok_or("Select a seam or a segment first")?,
            )]
        };
        if targets.is_empty() {
            return Err(
                "Piping goes along whole segments; split them at the seam's ends first".into(),
            );
        }
        let mut done = 0;
        for (panel, edge) in targets {
            let Some(polygon) = self
                .project
                .template
                .panels
                .iter()
                .find(|p| p.id == panel)
                .and_then(|p| p.polygon.clone())
            else {
                continue;
            };
            // The cord's centre line 4 mm inside the edge, like topstitching: closer, it can fall
            // outside the meshed cloth where the mesh cuts a curved edge short.
            let Some(path_m) =
                lines2d::offset_lines(&polygon, &edge, 1, PIPING_INSET_M, false, false)
                    .ok()
                    .and_then(|mut lines| lines.pop())
            else {
                continue;
            };
            let trim = Trim {
                id: self.new_trim_id(),
                panel_id: panel,
                kind: TrimKind::Piping {
                    path_m,
                    diameter_m: PIPING_DIAMETER_M,
                },
            };
            done += usize::from(self.edit(Command::SetTrim { trim }));
        }
        if done == 0 {
            return Err(self.status.clone());
        }
        self.status = format!(
            "Piping along {} · it changes the look, not the drape",
            super::garment_display::counted(done, "segment", "segments")
        );
        Ok(())
    }

    /// Materials › Puckering: puckering on both sides of the seam selected in the seam list or
    /// the 3D view (seamline puckering), otherwise along the selected segment or internal line
    /// (segment puckering).
    pub(super) fn pucker_selected(&mut self) -> Result<(), String> {
        use fashioncraft::garment::template::LineSpring;
        let seam = self.authoring.selection.seam.clone().and_then(|id| {
            self.project
                .template
                .seams
                .iter()
                .find(|s| s.id == id)
                .cloned()
        });
        let (targets, partial, what) = if let Some(seam) = seam {
            let (segments, partial) = Self::seam_segments(&seam);
            (segments, partial, format!("seam {}", seam.id))
        } else {
            let edge = self
                .authoring
                .edge
                .clone()
                .ok_or("Select a seam, a segment or an internal line first")?;
            let line = edge
                .edge
                .clone()
                .ok_or("Select a seam, a segment or an internal line first")?;
            (vec![(edge.panel, line.clone())], 0, line)
        };
        if targets.is_empty() {
            return Err(
                "This seam joins parts of segments only; split them at the seam's ends first"
                    .into(),
            );
        }
        let mut done = 0;
        for (panel, line) in &targets {
            done += usize::from(self.edit(Command::SetLineSpring {
                panel: panel.clone(),
                line: line.clone(),
                spring: Some(LineSpring::puckering(panel, line)),
            }));
        }
        self.status = format!(
            "Puckering along {} of {what} · simulate to see it{}",
            super::garment_display::counted(done, "segment", "segments"),
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
                && s.kind == fashioncraft::garment::template::LineKind::Tape
        });
        if taped {
            return Err("This line is taped already".into());
        }
        if !self.edit(Command::SetLineSpring {
            panel: edge.panel.clone(),
            line: line.clone(),
            spring: Some(fashioncraft::garment::template::LineSpring::tape(
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
