//! DXF-AAMA / DXF-ASTM: the export window and the import review. The format and the
//! panel conversion live in `fashioncraft::garment::dxf_aama`; this is only the dialogs.
//! The import review changes nothing until the document adapter applies its prepared edit.
use super::i18n::{self, Language};
use eframe::egui;
use fashioncraft::{
    authoring::Edit,
    garment::{
        GarmentTemplate,
        dxf_aama::{
            self, Dialect, PatternFile, Quantity, ReadOptions, Units, WriteOptions,
            template::{self as convert, Converted, PieceOptions},
        },
        grading::{self, rul::RuleTable},
        meshing::MeshingSettings,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Default)]
pub(super) struct DxfAamaUi {
    pub import: ImportUi,
    pub export: ExportUi,
}

fn named(_ui: &egui::Ui, _response: &egui::Response, _name: &str) {
    #[cfg(test)]
    super::super::authoring_smoke::widget_harness::named(_ui, _response, _name);
}

fn button(ui: &mut egui::Ui, name: &str, label: &str, enabled: bool) -> bool {
    let r = ui
        .push_id(name, |ui| ui.add_enabled(enabled, egui::Button::new(label)))
        .inner;
    named(ui, &r, name);
    r.clicked()
}

fn millimetres(value: &str) -> Result<f64, String> {
    let n = value
        .trim()
        .parse::<f64>()
        .map_err(|_| "Enter a numeric value")?
        * 0.001;
    if !n.is_finite() || n <= 0. {
        return Err("Value must be positive and finite".into());
    }
    Ok(n)
}

/// The export options, kept between exports.
#[derive(Default)]
pub(super) struct ExportUi {
    pub open: bool,
    dialect: Dialect,
    units: Units,
    size: String,
    selected_only: bool,
    /// Pieces to cut as drawn and mirrored, by panel id.
    quantities: BTreeMap<String, Quantity>,
}

impl ExportUi {
    /// The export window; whether Save was clicked.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        template: &GarmentTemplate,
        language: Language,
        selection: usize,
    ) -> bool {
        if !self.open {
            return false;
        }
        let tr = |text| i18n::tr(language, text);
        let mut open = true;
        let mut save = false;
        egui::Window::new(tr("DXF-AAMA / ASTM patterns"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                super::workspace::prop_row(ui, tr("Dialect"), |ui| {
                    egui::ComboBox::from_id_salt("dxf-aama.dialect")
                        .selected_text(self.dialect.label())
                        .show_ui(ui, |ui| {
                            for d in Dialect::ALL {
                                ui.selectable_value(&mut self.dialect, d, d.label());
                            }
                        });
                });
                super::workspace::prop_row(ui, tr("Units"), |ui| {
                    egui::ComboBox::from_id_salt("dxf-aama.units")
                        .selected_text(tr(self.units.label()))
                        .show_ui(ui, |ui| {
                            for u in Units::ALL {
                                ui.selectable_value(&mut self.units, u, tr(u.label()));
                            }
                        });
                });
                super::workspace::prop_row(ui, tr("Garment size"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.size).desired_width(80.))
                });
                ui.add_enabled(
                    selection > 0,
                    egui::Checkbox::new(&mut self.selected_only, tr("Only the selected pieces")),
                );
                egui::CollapsingHeader::new(super::workspace::section(tr("Pieces to cut")))
                    .id_salt("dxf-aama.quantities")
                    .show(ui, |ui| {
                        ui.small(tr("As drawn, and mirrored (the file's Quantity: R,L)."));
                        egui::ScrollArea::vertical().max_height(220.).show(ui, |ui| {
                            for panel in &template.panels {
                                let q = self.quantities.entry(panel.id.clone()).or_default();
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(&panel.name)
                                            .truncate(),
                                    );
                                    ui.add(egui::DragValue::new(&mut q.right).range(0..=99));
                                    ui.label("+");
                                    ui.add(egui::DragValue::new(&mut q.left).range(0..=99));
                                });
                            }
                        });
                    });
                ui.small(tr("Each piece is a block with its cut and sew lines, notches, grain line, name, size and quantity. Curves are written as points within 0.25 mm. A graded garment goes out in its base size, with its grade rules in a .rul file beside the DXF."));
                save = button(ui, "dxf-aama.save", tr("Save DXF…"), true);
            });
        self.open &= open;
        save
    }

    /// The file name offered: the style and the dialect.
    pub fn file_name(&self, template: &GarmentTemplate) -> String {
        let style: String = dxf_aama::template::file_stem(&template.name);
        let dialect = match self.dialect {
            Dialect::Aama => "aama",
            Dialect::Astm => "astm",
        };
        format!("{style}-{dialect}.dxf")
    }

    /// The file's bytes: the garment's pieces (or the selected ones) in the chosen dialect.
    #[cfg(test)]
    pub fn write(
        &self,
        template: &GarmentTemplate,
        selected: &[String],
    ) -> Result<(Vec<u8>, usize), String> {
        self.write_graded(template, selected, "")
            .map(|(bytes, count, _)| (bytes, count))
    }

    /// The DXF's bytes and, for a graded garment, its rule table (`.rul`) text, named
    /// `table` (the file's stem).
    pub fn write_graded(
        &self,
        template: &GarmentTemplate,
        selected: &[String],
        table: &str,
    ) -> Result<(Vec<u8>, usize, Option<String>), String> {
        let panels = self
            .selected_only
            .then(|| selected.iter().cloned().collect::<BTreeSet<_>>())
            .filter(|s| !s.is_empty());
        let grading::dxf::Export {
            pieces,
            table: rules,
        } = grading::dxf::export(
            template,
            &PieceOptions {
                size: self.size.trim().into(),
                quantities: self
                    .quantities
                    .iter()
                    .filter(|(_, q)| q.total() > 0)
                    .map(|(id, q)| (id.clone(), *q))
                    .collect(),
                panels,
            },
            self.units,
            self.dialect,
        )?;
        let rules = rules.map(|mut r| {
            if !table.trim().is_empty() {
                r.name = grading::rul::size_word(&table.to_uppercase());
            }
            r
        });
        let bytes = dxf_aama::write(
            &pieces,
            &WriteOptions {
                dialect: self.dialect,
                units: self.units,
                style: template.name.clone(),
                sample_size: rules
                    .as_ref()
                    .map_or_else(|| self.size.trim().into(), |r| r.sample.clone()),
                author: convert::author(),
                created: Some(convert::now_utc()),
                grade_rule_table: rules.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
            },
        )?;
        let text = rules.as_ref().map(grading::rul::write).transpose()?;
        Ok((bytes, pieces.len(), text))
    }
}

/// The import review: the file read, each piece converted, the choices.
pub(super) struct ImportUi {
    pub open: bool,
    path: Option<PathBuf>,
    text: Option<String>,
    file: Option<PatternFile>,
    reviews: Vec<Result<Converted, String>>,
    selected: BTreeSet<usize>,
    scale_mm: String,
    reviewed: Option<String>,
    mesh_mm: String,
    fabric: String,
    acknowledged: bool,
    pub error: Option<String>,
    /// The rule table found beside the file, read or failing.
    rules: Option<(PathBuf, Result<RuleTable, String>)>,
    /// Bring the grade rules in with the pieces.
    use_rules: bool,
}

impl Default for ImportUi {
    fn default() -> Self {
        Self {
            open: false,
            path: None,
            text: None,
            file: None,
            reviews: vec![],
            selected: BTreeSet::new(),
            scale_mm: String::new(),
            reviewed: None,
            mesh_mm: "25".into(),
            fabric: String::new(),
            acknowledged: false,
            error: None,
            rules: None,
            use_rules: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Choose,
    Confirm,
    Cancel,
}

impl ImportUi {
    pub fn begin(&mut self, template: &GarmentTemplate) {
        *self = Self {
            open: true,
            fabric: template
                .fabrics
                .first()
                .map(|f| f.id.clone())
                .unwrap_or_default(),
            ..Default::default()
        };
    }

    pub fn choose(&mut self, path: &std::path::Path) {
        self.scale_mm.clear();
        self.path = Some(path.to_owned());
        self.rules = grading::rul::beside(path).map(|rul| {
            let table = std::fs::read_to_string(&rul)
                .map_err(|e| format!("{}: {e}", rul.display()))
                .and_then(|text| grading::rul::read(&text));
            (rul, table)
        });
        self.use_rules = true;
        match dxf_aama::load(path) {
            Ok(text) => {
                self.text = Some(text);
                self.refresh();
            }
            Err(e) => {
                self.text = None;
                self.file = None;
                self.error = Some(e);
            }
        }
    }

    fn refresh(&mut self) {
        self.file = None;
        self.reviews.clear();
        self.selected.clear();
        self.acknowledged = false;
        self.reviewed = None;
        let result = (|| {
            let scale = if self.scale_mm.trim().is_empty() {
                None
            } else {
                Some(millimetres(&self.scale_mm)?)
            };
            dxf_aama::read(
                self.text.as_deref().ok_or("Choose a DXF file first")?,
                ReadOptions {
                    metres_per_unit: scale,
                    ..Default::default()
                },
            )
        })();
        match result {
            Ok(file) => {
                self.reviews = file.pieces.iter().map(convert::convert).collect();
                self.selected = (0..file.pieces.len())
                    .filter(|i| self.reviews[*i].is_ok())
                    .collect();
                self.file = Some(file);
                self.reviewed = Some(self.scale_mm.clone());
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn current(&self) -> bool {
        self.reviewed.as_deref() == Some(self.scale_mm.as_str())
    }

    pub fn prepare(&self, template: &GarmentTemplate, origin: glam::DVec3) -> Result<Edit, String> {
        if !self.current() || !self.acknowledged {
            return Err("Review the pieces and what is not imported first".into());
        }
        let file = self.file.as_ref().ok_or("No reviewed file")?;
        let selected: Vec<usize> = self.selected.iter().copied().collect();
        let mut edit = convert::prepare(
            template,
            file,
            &selected,
            &self.fabric,
            MeshingSettings {
                target_edge_m: millimetres(&self.mesh_mm)?,
                ..Default::default()
            },
            origin,
        )?;
        // The grade rules beside the file grade the new pieces.
        if self.use_rules
            && let Some((_, Ok(table))) = &self.rules
        {
            let ids: Vec<String> = edit.template.panels[template.panels.len()..]
                .iter()
                .map(|p| p.id.clone())
                .collect();
            grading::dxf::attach(&mut edit.template, file, &selected, &ids, table)?;
            edit.garment = edit.template.compile()?;
        }
        Ok(edit)
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        template: &GarmentTemplate,
        language: Language,
    ) -> Option<Action> {
        if !self.open {
            return None;
        }
        let action = egui::Modal::new(egui::Id::new("dxf-aama-import"))
            .show(ctx, |ui| self.controls(ui, template, language))
            .inner;
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            Some(Action::Cancel)
        } else {
            action
        }
    }

    fn controls(
        &mut self,
        ui: &mut egui::Ui,
        template: &GarmentTemplate,
        language: Language,
    ) -> Option<Action> {
        let tr = |text| i18n::tr(language, text);
        ui.set_width((ui.ctx().content_rect().width() - 64.).clamp(180., 640.));
        ui.heading(tr("Import DXF-AAMA / ASTM patterns"));
        let mut action = None;
        let height = (ui.ctx().content_rect().height() - 150.).clamp(100., 650.);
        egui::ScrollArea::vertical()
            .id_salt("dxf-aama-details")
            .auto_shrink([false, false])
            .max_height(height)
            .min_scrolled_height(height)
            .show(ui, |ui| {
                if button(ui, "aama.choose", tr("Choose DXF…"), true) {
                    action = Some(Action::Choose);
                }
                if let Some(path) = &self.path {
                    ui.label(path.display().to_string());
                }
                ui.horizontal(|ui| {
                    ui.label(tr("mm per unit (blank = file units)"));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.scale_mm)
                            .id(egui::Id::new("aama.scale"))
                            .desired_width(70.),
                    );
                    named(ui, &r, "aama.scale");
                    if button(ui, "aama.refresh", tr("Update"), self.text.is_some()) {
                        self.refresh();
                    }
                });
                if self.file.is_some() && !self.current() {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        tr("Scale changed — update and review again"),
                    );
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                if let Some(file) = &self.file {
                    ui.label(format!(
                        "{} · {} · {} {}",
                        file.dialect.label(),
                        file.dialect_reason,
                        file.units.map_or_else(
                            || format!("{:.4} mm/unit", file.metres_per_unit * 1000.),
                            |u| u.label().to_string()
                        ),
                        file.units_source
                    ));
                    if !file.style.is_empty() || !file.sample_size.is_empty() {
                        ui.small(format!(
                            "{} · {} {}",
                            file.style,
                            tr("sample size"),
                            file.sample_size
                        ));
                    }
                    match &self.rules {
                        Some((path, Ok(table))) => {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let r = ui.checkbox(
                                &mut self.use_rules,
                                format!(
                                    "{}{name} · {} · {} {} · {} {}",
                                    tr("Grade rules: "),
                                    table.sizes.join(" "),
                                    table.rules.len(),
                                    tr("rules"),
                                    tr("sample size"),
                                    table.sample
                                ),
                            );
                            named(ui, &r, "aama.rules");
                            for note in &table.notes {
                                ui.colored_label(egui::Color32::YELLOW, note);
                            }
                        }
                        Some((path, Err(e))) => {
                            ui.colored_label(
                                egui::Color32::LIGHT_RED,
                                format!("{}: {e}", path.display()),
                            );
                        }
                        None => {
                            if file.pieces.iter().any(|p| !p.grade_points.is_empty()) {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    tr("The pieces carry grade rule numbers, but no .rul file is beside the DXF: they come in ungraded."),
                                );
                            }
                        }
                    }
                    ui.label(super::workspace::section(tr("Pieces")));
                    for (i, piece) in file.pieces.iter().enumerate() {
                        let review = &self.reviews[i];
                        let mut checked = self.selected.contains(&i);
                        let title = format!(
                            "{} · {} {} · ×{},{} · {}",
                            piece.name,
                            tr("size"),
                            piece.size,
                            piece.quantity.right,
                            piece.quantity.left,
                            piece.material
                        );
                        let r = ui
                            .push_id(("aama.piece", i), |ui| {
                                ui.add_enabled(review.is_ok(), egui::Checkbox::new(&mut checked, title))
                            })
                            .inner;
                        named(ui, &r, &format!("aama.piece.{i}"));
                        if r.changed() {
                            if checked {
                                self.selected.insert(i);
                            } else {
                                self.selected.remove(&i);
                            }
                            self.acknowledged = false;
                        }
                        ui.indent(("aama.piece.notes", i), |ui| match review {
                            Ok(c) => {
                                ui.small(format!(
                                    "{:.0} × {:.0} mm · {} {} · {} {} · {} {:.2}°{}",
                                    c.size.x * 1000.,
                                    c.size.y * 1000.,
                                    c.polygon.points.len(),
                                    tr("boundaries"),
                                    c.polygon.notches.values().map(Vec::len).sum::<usize>(),
                                    tr("notches"),
                                    tr("grain"),
                                    c.grain_degrees,
                                    if c.polygon.seam_allowances.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" · {}", tr("seam allowance"))
                                    }
                                ));
                                for note in &c.notes {
                                    ui.colored_label(egui::Color32::YELLOW, note);
                                }
                            }
                            Err(e) => {
                                ui.colored_label(egui::Color32::LIGHT_RED, e);
                            }
                        });
                    }
                    egui::CollapsingHeader::new(super::workspace::section(format!(
                        "{} ({})",
                        tr("Not imported"),
                        file.issues.len()
                    )))
                    .id_salt("aama.issues")
                    .default_open(!file.issues.is_empty())
                    .show(ui, |ui| {
                        for issue in &file.issues {
                            let text = format!(
                                "{} · {} {} · {}: {}{}",
                                if issue.piece.is_empty() { "—" } else { &issue.piece },
                                tr("layer"),
                                issue.layer,
                                issue.entity,
                                issue.reason,
                                if issue.count > 1 {
                                    format!(" (×{})", issue.count)
                                } else {
                                    String::new()
                                }
                            );
                            ui.add(egui::Label::new(&text).truncate())
                                .on_hover_text(text);
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(tr("Mesh spacing (mm, 3–500)"));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.mesh_mm)
                            .id(egui::Id::new("aama.mesh"))
                            .desired_width(70.),
                    );
                    named(ui, &r, "aama.mesh");
                });
                egui::ComboBox::from_id_salt("aama-fabric")
                    .selected_text(
                        template
                            .fabrics
                            .iter()
                            .find(|f| f.id == self.fabric)
                            .map(|f| f.name.as_str())
                            .unwrap_or("Choose fabric"),
                    )
                    .show_ui(ui, |ui| {
                        for f in &template.fabrics {
                            ui.selectable_value(&mut self.fabric, f.id.clone(), &f.name);
                        }
                    });
                ui.small(tr("The sew line becomes the piece; the cut line becomes its seam allowance. Pieces arrive unsewn, to the right of the avatar."));
                let r = ui.checkbox(
                    &mut self.acknowledged,
                    tr("Import the selected pieces; what is listed above stays out"),
                );
                named(ui, &r, "aama.acknowledge");
            });
        ui.separator();
        ui.horizontal(|ui| {
            if button(ui, "aama.cancel", tr("Cancel"), true) {
                action = Some(Action::Cancel);
            }
            let mesh_ok = millimetres(&self.mesh_mm).is_ok_and(|n| (0.003..=0.5).contains(&n));
            if button(
                ui,
                "aama.confirm",
                &format!("{} ({})", tr("Add pieces"), self.selected.len()),
                self.current() && self.acknowledged && !self.selected.is_empty() && mesh_ok,
            ) {
                action = Some(Action::Confirm);
            }
        });
        action
    }
}

/// The document adapter: opening the dialogs, saving the export, applying the import.
impl super::Atelier {
    pub(in super::super) fn open_dxf_aama_import(&mut self) {
        let ui_lang = self.prefs.language;
        if self.authoring.preview.is_some() || self.has_document_draft() {
            self.status = i18n::tr(ui_lang, "Finish or cancel the current drawing, placement or sewing before importing patterns").into();
            return;
        }
        self.playing = false;
        self.document.dxf_aama.import.begin(&self.project.template);
    }
    pub(in super::super) fn open_dxf_aama_export(&mut self) {
        self.document.dxf_aama.export.open = true;
    }
    /// The DXF-AAMA/ASTM import review and export window.
    pub(super) fn dxf_aama_tick(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        let selection = self.selected_panels();
        if self
            .document
            .dxf_aama
            .export
            .show(ctx, &self.project.template, ui_lang, selection.len())
        {
            let name = self
                .document
                .dxf_aama
                .export
                .file_name(&self.project.template);
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Export DXF-AAMA / ASTM patterns")
                .add_filter("DXF", &["dxf"])
                .set_file_name(name)
                .save_file()
            {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let rul = path.with_extension("rul");
                let written = self
                    .document
                    .dxf_aama
                    .export
                    .write_graded(&self.project.template, &selection, &stem)
                    .and_then(|(bytes, count, rules)| {
                        std::fs::write(&path, bytes)
                            .map_err(|e| format!("{}: {e}", path.display()))?;
                        if let Some(text) = rules {
                            std::fs::write(&rul, text)
                                .map_err(|e| format!("{}: {e}", rul.display()))?;
                            return Ok((count, true));
                        }
                        Ok((count, false))
                    });
                self.status = match written {
                    Ok((count, graded)) => {
                        self.document.dxf_aama.export.open = false;
                        if graded {
                            format!(
                                "{count} pieces saved in {}, grade rules in {}",
                                path.display(),
                                rul.display()
                            )
                        } else {
                            format!("{count} pieces saved in {}", path.display())
                        }
                    }
                    Err(error) => error,
                };
            }
        }
        use Action;
        match self
            .document
            .dxf_aama
            .import
            .show(ctx, &self.project.template, ui_lang)
        {
            Some(Action::Choose) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Import DXF-AAMA / ASTM patterns")
                    .add_filter("DXF", &["dxf"])
                    .pick_file()
                {
                    self.document.dxf_aama.import.choose(&path);
                }
            }
            Some(Action::Cancel) => self.document.dxf_aama.import = Default::default(),
            Some(Action::Confirm) => {
                let origin = super::pattern_arrangement_origin(&self.project, &self.scene);
                match self
                    .document
                    .dxf_aama
                    .import
                    .prepare(&self.project.template, origin)
                {
                    Ok(edit) => {
                        self.authoring.history.record(self.project.template.clone());
                        self.install_edit(edit);
                        self.document.dxf_aama.import = Default::default();
                        self.status = i18n::tr(ui_lang, "DXF panels added · move and sew them in the workspace · Undo is available").into();
                    }
                    Err(error) => self.document.dxf_aama.import.error = Some(error),
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::authoring_smoke::widget_harness::Harness;
    use super::*;
    use fashioncraft::{fitting::FittingProject, garment::presets::GarmentKind};

    struct Case {
        import: ImportUi,
        template: GarmentTemplate,
        path: PathBuf,
        prepared: Option<Edit>,
    }

    fn draw(ui: &mut egui::Ui, c: &mut Case) {
        match c.import.show(ui.ctx(), &c.template, Language::English) {
            Some(Action::Choose) => c.import.choose(&c.path.clone()),
            Some(Action::Cancel) => c.import = Default::default(),
            Some(Action::Confirm) => {
                match c.import.prepare(&c.template, glam::DVec3::new(3., 1.5, 0.)) {
                    Ok(edit) => {
                        c.prepared = Some(edit);
                        c.import = Default::default();
                    }
                    Err(e) => c.import.error = Some(e),
                }
            }
            None => {}
        }
    }

    #[test]
    fn review_acknowledge_and_add_the_pieces_of_an_astm_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = FittingProject::basic(GarmentKind::Tshirt, 0, dir.path().join("a.obj"));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dxf-aama/handmade-astm.dxf");
        let mut c = Case {
            import: ImportUi::default(),
            template: project.template.clone(),
            path,
            prepared: None,
        };
        c.import.begin(&c.template);
        let mut h = Harness::new(egui::vec2(1000., 1000.));
        for _ in 0..8 {
            h.frame(&mut c, &mut draw);
        }
        h.click("aama.choose", &mut c, &mut draw).unwrap();
        for _ in 0..8 {
            h.frame(&mut c, &mut draw);
        }
        assert_eq!(c.import.selected.len(), 2, "{:?}", c.import.error);
        // Nothing is added before what stays out is acknowledged.
        assert!(h.click("aama.confirm", &mut c, &mut draw).is_err());
        h.click("aama.piece.1", &mut c, &mut draw).unwrap();
        h.click("aama.acknowledge", &mut c, &mut draw).unwrap();
        h.click("aama.confirm", &mut c, &mut draw).unwrap();
        let edit = c.prepared.expect("prepared");
        assert_eq!(
            edit.template.panels.len(),
            project.template.panels.len() + 1
        );
        assert_eq!(edit.template.panels.last().unwrap().name, "Back Yoke");
    }

    #[test]
    fn a_graded_export_writes_its_rule_table_and_the_import_reads_it_back() {
        use fashioncraft::garment::grading::{Grading, Target};
        let dir = tempfile::tempdir().unwrap();
        let project = FittingProject::basic(GarmentKind::Tshirt, 0, dir.path().join("a.obj"));
        let mut graded = project.template.clone();
        let mut g = Grading::new("Sizes", &["S", "M", "L"], 1).unwrap();
        for panel in ["front", "back"] {
            for (edge, x) in [("side-right", 0.01), ("hem", 0.01), ("side-left", -0.01)] {
                let target = Target::Point {
                    panel: panel.into(),
                    edge: edge.into(),
                };
                g.set_step(&target, glam::DVec2::new(x, 0.)).unwrap();
            }
        }
        graded.grading = Some(g);
        let export = ExportUi {
            dialect: Dialect::Astm,
            ..Default::default()
        };
        let (bytes, count, rules) = export.write_graded(&graded, &[], "shirt").unwrap();
        assert_eq!(count, 2);
        let rules = rules.expect("a graded garment writes a rule table");
        assert!(rules.contains("GRADE RULE TABLE: SHIRT"), "{rules}");
        assert!(rules.contains("SIZE LIST: S M L"), "{rules}");
        let path = dir.path().join("shirt.dxf");
        std::fs::write(&path, bytes).unwrap();
        std::fs::write(dir.path().join("SHIRT.RUL"), rules).unwrap();
        // The review finds the table beside the file and the pieces come in graded.
        let mut c = Case {
            import: ImportUi::default(),
            template: project.template.clone(),
            path,
            prepared: None,
        };
        c.import.begin(&c.template);
        let mut h = Harness::new(egui::vec2(1000., 1000.));
        for _ in 0..8 {
            h.frame(&mut c, &mut draw);
        }
        h.click("aama.choose", &mut c, &mut draw).unwrap();
        for _ in 0..8 {
            h.frame(&mut c, &mut draw);
        }
        assert!(matches!(c.import.rules, Some((_, Ok(_)))));
        h.click("aama.acknowledge", &mut c, &mut draw).unwrap();
        h.click("aama.confirm", &mut c, &mut draw).unwrap();
        let edit = c.prepared.expect("prepared");
        let grading = edit.template.grading.as_ref().expect("graded");
        let names: Vec<&str> = grading.groups[0]
            .sizes
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["S", "M", "L"]);
        assert_eq!(grading.groups[0].base_name(), "M");
        let new = &edit.template.panels[project.template.panels.len()].id;
        assert!(
            grading.panels.contains_key(new),
            "{:?}",
            grading.panels.keys()
        );
        // Unticking the rules brings the pieces in ungraded.
        let mut c = Case {
            import: ImportUi::default(),
            template: project.template.clone(),
            path: c.path,
            prepared: None,
        };
        c.import.begin(&c.template);
        for _ in 0..8 {
            h.frame(&mut c, &mut draw);
        }
        h.click("aama.choose", &mut c, &mut draw).unwrap();
        for _ in 0..8 {
            h.frame(&mut c, &mut draw);
        }
        h.click("aama.rules", &mut c, &mut draw).unwrap();
        h.click("aama.acknowledge", &mut c, &mut draw).unwrap();
        h.click("aama.confirm", &mut c, &mut draw).unwrap();
        assert!(c.prepared.expect("prepared").template.grading.is_none());
    }

    #[test]
    fn export_writes_the_chosen_dialect() {
        let dir = tempfile::tempdir().unwrap();
        let project = FittingProject::basic(GarmentKind::Tshirt, 0, dir.path().join("a.obj"));
        let mut export = ExportUi {
            dialect: Dialect::Astm,
            size: "M".into(),
            ..Default::default()
        };
        export
            .quantities
            .insert("front".into(), Quantity { right: 1, left: 1 });
        let (bytes, count) = export.write(&project.template, &[]).unwrap();
        assert_eq!(count, 2);
        let text = String::from_utf8(bytes).unwrap();
        let file = dxf_aama::read(&text, ReadOptions::default()).unwrap();
        assert_eq!(file.dialect, Dialect::Astm);
        assert_eq!(file.pieces[0].quantity, Quantity { right: 1, left: 1 });
        assert!(export.file_name(&project.template).ends_with("-astm.dxf"));
        export.selected_only = true;
        let (_, count) = export.write(&project.template, &["back".into()]).unwrap();
        assert_eq!(count, 1);
    }
}
