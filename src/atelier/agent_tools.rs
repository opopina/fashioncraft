//! What each agent tool does in the window (`fashioncraft::agent::tools` describes them). They
//! run on the interface thread through the same commands as the menus, so every change is
//! undoable and shows on screen as it happens.
use super::agent_hub::Pending;
use super::*;
use fashioncraft::agent::{Content, ToolResult};
use fashioncraft::agent_ops::{self, Live};
use fashioncraft::authoring::Command;
use fashioncraft::garment::seam::{BoundaryRef, Side};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Marks the agent's screenshot among the window's screenshots.
struct AgentShot;

fn centimetres(v: f64) -> f64 {
    (v * 1000.).round() / 10.
}

impl Atelier {
    /// Starts tool `name`. Most finish at once; a simulation or a screenshot finishes over
    /// later frames (the `Err` is what it waits for).
    pub(super) fn begin_tool(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        args: &Value,
    ) -> Result<ToolResult, Pending> {
        let result =
            match name {
                "scene_summary" => Ok(ToolResult::json(&self.scene_summary())),
                "piece_detail" => agent_ops::piece_detail(
                    &self.project.template,
                    args.get("piece").and_then(Value::as_str).unwrap_or(""),
                )
                .map(|v| ToolResult::json(&v)),
                "select" => self.agent_select(args).map(|v| ToolResult::json(&v)),
                "command_list" => Ok(ToolResult::json(
                    &self.command_list(
                        args.get("query").and_then(Value::as_str).unwrap_or(""),
                        args.get("include_interactive")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ),
                )),
                "command_run" => self.agent_command(ctx, args),
                "create_piece"
                | "move_points"
                | "set_segment_length"
                | "add_point"
                | "add_dart"
                | "add_pleats"
                | "piece_edit"
                | "sew"
                | "seam_edit"
                | "set_fabric_parameters" => self.agent_plan(name, args),
                "place_piece" => self.agent_place_piece(args),
                "set_avatar" => self.agent_avatar(args),
                "set_pose" => self.agent_pose(args),
                "grading" => self.agent_grading(args),
                "render" => match self.agent_render(args) {
                    Ok(pending) => return Err(pending),
                    Err(e) => Err(e),
                },
                "draft_garment" => {
                    let overrides: std::collections::BTreeMap<String, f64> = args
                        .get("measurements_cm")
                        .and_then(Value::as_object)
                        .map(|m| {
                            m.iter()
                                .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                                .collect()
                        })
                        .unwrap_or_default();
                    let from_avatar = args
                        .get("from_avatar")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
                    self.agent_draft(kind, from_avatar, &overrides)
                        .map(ToolResult::text)
                }
                "set_fabric" => self.agent_fabric(args),
                "set_color" => self.agent_color(args),
                "simulate" => {
                    let seconds = args
                        .get("seconds")
                        .and_then(Value::as_f64)
                        .unwrap_or(5.)
                        .clamp(0.5, 60.);
                    if !self.playing {
                        self.invoke_command(commands::CommandId::ToggleSimulation, ctx);
                    }
                    if !self.playing {
                        Err(format!("The simulation did not start: {}", self.status))
                    } else {
                        return Err(Pending::Simulate {
                            until: Instant::now() + Duration::from_secs_f64(seconds),
                            seconds,
                        });
                    }
                }
                "screenshot" => {
                    let whole_window = args.get("area").and_then(Value::as_str) == Some("window");
                    if ctx.input(|i| i.viewport().visible() == Some(false)) {
                        Err(
                            "The FashionCraft window is minimised or covered, so there is nothing \
                         to capture: bring it to the front"
                                .to_owned(),
                        )
                    } else if !whole_window && self.authoring.selection.garment_rect.is_none() {
                        Err("The 3D view is not on screen: show it (Window › 3D only or Split view)"
                        .to_owned())
                    } else {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                            egui::UserData::new(AgentShot),
                        ));
                        return Err(Pending::Screenshot {
                            asked: Instant::now(),
                            whole_window,
                        });
                    }
                }
                "size_chart" => Ok(ToolResult::json(&self.size_chart())),
                "save_project" => self.agent_save(ctx, args),
                "export_garment" => self.agent_export(ctx, args),
                other => return self.begin_base_tool(ctx, other, args),
            };
        Ok(result.unwrap_or_else(ToolResult::error))
    }

    /// Finishes a call started in an earlier frame, or None while it still waits.
    pub(super) fn continue_tool(
        &mut self,
        ctx: &egui::Context,
        pending: &mut Pending,
    ) -> Option<ToolResult> {
        match pending {
            Pending::Base(base) => self.continue_base_tool(ctx, base),
            Pending::Simulate { until, seconds } => {
                if Instant::now() < *until && self.playing {
                    return None;
                }
                if self.playing {
                    self.invoke_command(commands::CommandId::ToggleSimulation, ctx);
                }
                let summary = self.scene_summary();
                Some(ToolResult::json(&json!({
                    "simulated_seconds_of_wall_clock": seconds,
                    "garment": summary["garment"],
                    "seams": summary["seams"],
                    "warnings": summary["warnings"],
                    "status": self.status,
                })))
            }
            Pending::Screenshot {
                asked,
                whole_window,
            } => {
                let shot = ctx.input(|i| {
                    i.events.iter().find_map(|event| match event {
                        egui::Event::Screenshot {
                            user_data, image, ..
                        } if user_data
                            .data
                            .as_ref()
                            .is_some_and(|d| d.downcast_ref::<AgentShot>().is_some()) =>
                        {
                            Some(image.clone())
                        }
                        _ => None,
                    })
                });
                let Some(image) = shot else {
                    return (asked.elapsed() > Duration::from_secs(10)).then(|| {
                        ToolResult::error("No screenshot came back: is the window minimised?")
                    });
                };
                Some(
                    self.screenshot_result(ctx, &image, *whole_window)
                        .unwrap_or_else(ToolResult::error),
                )
            }
            Pending::Render {
                started,
                until,
                path,
                saved,
                was_open,
            } => {
                let Some(asked) = *saved else {
                    if Instant::now() < *until {
                        return None;
                    }
                    if !self.render_running() || self.render_samples() == 0 {
                        // The render may take a while to receive the scene and start.
                        return (started.elapsed() > *until - *started + Duration::from_secs(30))
                            .then(|| {
                                ToolResult::error(
                                    "The photoreal render did not start: the GPU may not support it",
                                )
                            });
                    }
                    self.save_render(path.clone());
                    *saved = Some(Instant::now());
                    return None;
                };
                let written = std::fs::metadata(&*path)
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| {
                        t + Duration::from_secs(1) >= std::time::SystemTime::now() - asked.elapsed()
                    });
                if !written {
                    return (asked.elapsed() > Duration::from_secs(30))
                        .then(|| ToolResult::error("The render's image was not written"));
                }
                let samples = self.render_samples();
                if !*was_open {
                    self.authoring.render.open = false;
                }
                Some(ToolResult::text(format!(
                    "Rendered {samples} samples per pixel and saved {}",
                    path.display()
                )))
            }
        }
    }

    pub(super) fn screenshot_result(
        &self,
        ctx: &egui::Context,
        image: &egui::ColorImage,
        whole_window: bool,
    ) -> Result<ToolResult, String> {
        let pixels_per_point = ctx.pixels_per_point();
        let view = if whole_window {
            image.clone()
        } else {
            let rect = self
                .authoring
                .selection
                .garment_rect
                .and_then(|r| super::board_media::capture_rect(r, image.size, pixels_per_point))
                .ok_or("The 3D view is not on screen")?;
            image.region(&rect, Some(pixels_per_point))
        };
        let pixels: Vec<u8> = view.pixels.iter().flat_map(|p| p.to_array()).collect();
        let mut picture =
            image::RgbaImage::from_raw(view.size[0] as u32, view.size[1] as u32, pixels)
                .ok_or("The screenshot has the wrong size")?;
        // A model sees at most about this much; a smaller picture uploads faster.
        let longest = picture.width().max(picture.height());
        if longest > 1280 {
            let scale = 1280. / longest as f32;
            picture = image::imageops::resize(
                &picture,
                (picture.width() as f32 * scale) as u32,
                (picture.height() as f32 * scale) as u32,
                image::imageops::FilterType::Triangle,
            );
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(picture)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(ToolResult {
            content: vec![
                Content::Image {
                    data: fashioncraft::agent::base64(&png),
                    mime_type: "image/png".into(),
                },
                Content::Text {
                    text: if whole_window {
                        "The whole window now."
                    } else {
                        "The 3D view now."
                    }
                    .into(),
                },
            ],
            is_error: false,
        })
    }

    /// The drape as it stands: seam gaps mean something once the cloth has left its layout.
    pub(super) fn live(&self) -> Live<'_> {
        let x = &self.scene.state.x;
        let laid = &self.scene.garment.initial_state.x;
        let simulated =
            x.len() == laid.len() && x.iter().zip(laid).any(|(a, b)| a.distance(*b) > 0.002);
        Live {
            garment: &self.scene.garment,
            x,
            simulated,
        }
    }

    fn selection_json(&self) -> Value {
        let a = &self.authoring;
        json!({
            "pieces": self.selected_panels(),
            "segment": a.edge.as_ref().map(|e| format!("{}.{}", e.panel, e.edge.as_deref().unwrap_or("?"))),
            "point": a.anchor.as_ref().map(|(p, k)| json!({"piece": p, "point": k})),
            "points": a.board.point_selections(),
            "seam": a.selection.seam,
        })
    }

    pub(super) fn scene_summary(&mut self) -> Value {
        let avatar = if self.project.avatar_removed {
            json!({"present": false})
        } else {
            let body = self.avatar_body_cm().map(|v| {
                super::avatar_shape_ui::BODY_FIELDS
                    .iter()
                    .zip(v)
                    .map(|(k, v)| ((*k).to_owned(), json!(v)))
                    .collect::<serde_json::Map<_, _>>()
            });
            let surface = &self.scene.collider.surface.vertices;
            let height = surface
                .iter()
                .map(|v| v.y)
                .fold(f64::NEG_INFINITY, f64::max)
                - surface.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
            json!({
                "present": true,
                "file": self.project.avatar_path.file_name().map(|n| n.to_string_lossy()),
                "height_cm": if height.is_finite() { centimetres(height) } else { 0. },
                "measurements_cm": body.as_ref().ok(),
                "measurements_note": body.err(),
                "posed": self.project.avatar_pose.as_ref().is_some_and(|p| !p.pose.is_empty()),
                "arrangement_points": self.arrangement_anchor_ids(),
                "library_presets": fashioncraft::agent::avatar_preset_ids(),
                "pose_presets": fashioncraft::avatar_rig::PRESETS,
            })
        };
        let template = &self.project.template;
        let live = self.live();
        let x = live.x;
        let garment = if x.is_empty() {
            Value::Null
        } else {
            let (mut low, mut high, mut sum) = (f64::INFINITY, f64::NEG_INFINITY, 0.);
            for p in x {
                low = low.min(p.y);
                high = high.max(p.y);
                sum += p.y;
            }
            json!({
                "lowest_cm": centimetres(low),
                "highest_cm": centimetres(high),
                "centre_height_cm": centimetres(sum / x.len() as f64),
                "draped": live.simulated,
            })
        };
        let mut warnings = agent_ops::warnings(template, Some(&live));
        if let Some(e) = &self.simulation_error {
            warnings.push(format!("Simulation: {e}"));
        }
        json!({
            "garment_name": template.name,
            "pieces": agent_ops::describe_pieces(template),
            "seams": agent_ops::describe_seams(template, Some(&live)),
            "fabrics": agent_ops::describe_fabrics(template),
            "avatar": avatar,
            "sizes": agent_ops::describe_grading(template),
            "selection": self.selection_json(),
            "warnings": warnings,
            "garment": garment,
            "simulation_running": self.playing,
            "unsaved_changes": self.document.dirty,
            "can_undo": self.authoring.history.can_undo(),
            "fabric_presets": fashioncraft::fabrics::PRESETS.iter().map(|p| json!({"id": p.id, "name": p.name})).collect::<Vec<_>>(),
            "units": "cm; 2D board coordinates: x right, y down",
        })
    }

    fn command_list(&self, query: &str, include_interactive: bool) -> Value {
        use commands::AgentUse;
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let context = self.command_context();
        let mut hidden = 0;
        let commands: Vec<Value> = commands::REGISTRY
            .iter()
            .filter(|d| {
                let text = format!("{} {}", d.key, d.label).to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .filter(|d| {
                let runs = commands::agent_use(d.id) == AgentUse::Runs;
                hidden += usize::from(!runs && !include_interactive);
                runs || include_interactive
            })
            .map(|d| {
                let usage = commands::agent_use(d.id);
                let target = commands::agent_target(d.id);
                let mut entry = json!({
                    "key": d.key,
                    "label": d.label.trim_end_matches('…'),
                });
                if target != commands::AgentTarget::Nothing {
                    entry["acts_on"] = json!(target.label());
                }
                match (usage, commands::availability(d.id, context)) {
                    (AgentUse::Runs, Ok(())) => entry["available_now"] = json!(true),
                    (AgentUse::Runs, Err(why)) => {
                        entry["available_now"] = json!(false);
                        entry["why_not"] = json!(why.message());
                    }
                    (other, _) => {
                        entry["agent_can_run"] = json!(false);
                        entry["because"] = json!(usage_text(other));
                        entry["instead"] = json!(commands::agent_instead(d.id));
                        if let Some(why) = commands::agent_reason(d.id) {
                            entry["why_not"] = json!(why);
                        }
                    }
                }
                entry
            })
            .collect();
        json!({
            "commands": commands,
            "left_out": hidden,
            "note": if hidden > 0 {
                "Commands that need the mouse, a window or a file dialog are left out; include_interactive lists them"
            } else {
                ""
            },
        })
    }

    /// Select what an agent named, as clicks would. Returns the selection.
    fn agent_select(&mut self, args: &Value) -> Result<Value, String> {
        let template = &self.project.template;
        if args.get("clear").and_then(Value::as_bool) == Some(true) {
            let a = &mut self.authoring;
            a.selected = None;
            a.selection.panels.clear();
            a.selection.seam = None;
            a.edge = None;
            a.anchor = None;
            a.board.selection.clear();
            a.board.selection_panel = None;
            a.board.point_groups.clear();
        }
        if let Some(ids) = args.get("pieces").and_then(Value::as_array) {
            let ids: Vec<String> = ids
                .iter()
                .map(|v| {
                    agent_ops::find_piece(template, v.as_str().unwrap_or("")).map(|p| p.id.clone())
                })
                .collect::<Result<_, _>>()?;
            self.authoring.selection.panels = ids.iter().cloned().collect();
            self.authoring.selected = ids.first().cloned();
        }
        if let Some(key) = args.get("piece").and_then(Value::as_str) {
            let panel = agent_ops::find_piece(template, key)?;
            let id = panel.id.clone();
            if let Some(segment) = args.get("segment").and_then(Value::as_str) {
                let polygon = panel.polygon.as_ref();
                let known = polygon.is_some_and(|q| {
                    q.edges.iter().any(|e| e == segment) || q.lines.iter().any(|l| l.id == segment)
                });
                if !known {
                    return Err(format!(
                        "Piece {id} has no segment or internal line '{segment}': see piece_detail"
                    ));
                }
                self.authoring.edge = Some(BoundaryRef {
                    reversed: false,
                    panel: id.clone(),
                    side: Side::Top,
                    edge: Some(segment.to_owned()),
                    range: [0., 1.],
                });
            }
            if let Some(k) = args.get("point").and_then(Value::as_u64) {
                let k = k as usize;
                let n = panel.polygon.as_ref().map_or(0, |q| q.points.len());
                if k >= n {
                    return Err(format!(
                        "Piece {id} has points 0 to {}",
                        n.saturating_sub(1)
                    ));
                }
                self.authoring.anchor = Some((id.clone(), k));
                self.authoring.board.selection_panel = Some(id.clone());
                self.authoring.board.selection = vec![k];
            }
            self.authoring.selected = Some(id.clone());
            self.authoring.selection.panels = [id].into_iter().collect();
        }
        if let Some(seam) = args.get("seam").and_then(Value::as_str) {
            let spec = template
                .seams
                .iter()
                .find(|s| s.id == seam)
                .ok_or_else(|| format!("No seam '{seam}': see scene_summary"))?;
            self.authoring.edge = Some(spec.a.clone());
            self.authoring.selection.seam = Some(seam.to_owned());
        }
        Ok(self.selection_json())
    }

    /// Whether what a command acts on is selected.
    fn agent_has(&self, target: commands::AgentTarget) -> bool {
        use commands::AgentTarget as T;
        let outline = |e: &BoundaryRef, line: bool| {
            self.project
                .template
                .panels
                .iter()
                .find(|p| p.id == e.panel)
                .and_then(|p| p.polygon.as_ref())
                .zip(e.edge.as_ref())
                .is_some_and(|(q, name)| {
                    if line {
                        q.lines.iter().any(|l| &l.id == name)
                    } else {
                        q.edges.contains(name)
                    }
                })
        };
        match target {
            T::Nothing => true,
            T::Pieces => !self.selected_panels().is_empty(),
            T::Segment => self
                .authoring
                .edge
                .as_ref()
                .is_some_and(|e| outline(e, false)),
            T::Line => self
                .authoring
                .edge
                .as_ref()
                .is_some_and(|e| outline(e, true)),
            T::Points => {
                self.authoring.anchor.is_some()
                    || self
                        .authoring
                        .board
                        .point_selections()
                        .values()
                        .any(|v| !v.is_empty())
            }
            T::Seam => self.authoring.selection.seam.is_some(),
            T::Mesh => false,
        }
    }

    fn agent_command(&mut self, ctx: &egui::Context, args: &Value) -> Result<ToolResult, String> {
        let key = args
            .get("key")
            .and_then(Value::as_str)
            .ok_or("command_run needs a key")?;
        let descriptor = commands::REGISTRY
            .iter()
            .find(|d| d.key == key)
            .ok_or_else(|| format!("No command '{key}': see command_list"))?;
        let usage = commands::agent_use(descriptor.id);
        if usage != commands::AgentUse::Runs {
            let label = descriptor.label.trim_end_matches('…');
            return Err(match commands::agent_instead(descriptor.id) {
                Some(tool) => format!(
                    "'{key}' ({label}) {}, so an agent cannot use it. Use {tool} instead.",
                    usage_text(usage)
                ),
                None => format!(
                    "'{key}' ({label}) {}, so an agent cannot use it ({}). Tell the person to \
                     choose it from the menu themselves.",
                    usage_text(usage),
                    commands::agent_reason(descriptor.id).unwrap_or("no agent tool does it yet")
                ),
            });
        }
        if let Some(target) = args.get("target") {
            self.agent_select(target)?;
        }
        let target = commands::agent_target(descriptor.id);
        if !self.agent_has(target) {
            return Err(format!(
                "'{key}' acts on {}: select it first (the select tool, or command_run's target)",
                target.label()
            ));
        }
        let before = self.authoring.history.done().count();
        self.dispatch_command(descriptor.id, ctx)?;
        let undoable = self.authoring.history.done().count() > before;
        Ok(ToolResult::text(format!(
            "Ran {key} ({}). {}{}",
            descriptor.label.trim_end_matches('…'),
            self.status,
            if undoable { " (undoable)" } else { "" }
        )))
    }

    /// Apply an editing tool's change through the history.
    fn agent_plan(&mut self, tool: &str, args: &Value) -> Result<ToolResult, String> {
        let plan = agent_ops::plan(&self.project.template, tool, args)?;
        let name = plan.history_name.unwrap_or_else(|| plan.command.name());
        if !self.edit_named(plan.command, name) {
            return Err(self.status.clone());
        }
        Ok(ToolResult::text(plan.done))
    }

    fn agent_place_piece(&mut self, args: &Value) -> Result<ToolResult, String> {
        let piece = agent_ops::find_piece(
            &self.project.template,
            args.get("piece").and_then(Value::as_str).unwrap_or(""),
        )?
        .id
        .clone();
        let anchor = args
            .get("anchor")
            .and_then(Value::as_str)
            .ok_or("Name an arrangement point (scene_summary lists them)")?;
        let clearance = args
            .get("clearance_cm")
            .and_then(Value::as_f64)
            .map(|c| c / 100.);
        let wrap = args.get("wrap").and_then(Value::as_bool);
        self.agent_place(&piece, anchor, clearance, wrap)
            .map(ToolResult::text)
    }

    fn agent_avatar(&mut self, args: &Value) -> Result<ToolResult, String> {
        if self.pending {
            return Err("Wait for the current simulation step to finish, then try again".into());
        }
        if let Some(id) = args.get("preset").and_then(Value::as_str) {
            let preset = fashioncraft::avatar_look::PRESETS
                .iter()
                .find(|p| fashioncraft::agent::avatar_preset_id(p.name) == id)
                .ok_or_else(|| {
                    format!(
                        "No avatar preset '{id}'. They are: {}",
                        fashioncraft::agent::avatar_preset_ids().join(", ")
                    )
                })?;
            let obj = super::avatar_library_ui::write_made_avatar(
                &self.output,
                &(preset.body)(),
                &preset.look,
            )?;
            self.install_avatar_file(&obj)?;
            return Ok(ToolResult::text(format!(
                "The avatar is now '{}' from the library. The drape was reset: simulate again \
                 (pieces drafted for the old body may need draft_garment again).",
                preset.name
            )));
        }
        let overrides: BTreeMap<String, f64> = args
            .get("measurements_cm")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                    .collect()
            })
            .unwrap_or_default();
        if overrides.is_empty() {
            return Err("Give a library preset or measurements_cm".into());
        }
        self.agent_reshape_avatar(&overrides).map(ToolResult::text)
    }

    fn agent_pose(&mut self, args: &Value) -> Result<ToolResult, String> {
        let name = args.get("pose").and_then(Value::as_str).unwrap_or("");
        let preset = fashioncraft::avatar_rig::preset(name).ok_or_else(|| {
            format!(
                "No pose '{name}'. They are: {}",
                fashioncraft::avatar_rig::PRESETS.join(", ")
            )
        })?;
        if self.project.avatar_removed {
            return Err("There is no avatar to pose".into());
        }
        let mut pose = self.current_pose()?;
        pose.pose = preset;
        if !self.apply_pose(pose) {
            return Err(self.status.clone());
        }
        Ok(ToolResult::text(format!(
            "Posed '{name}'. The drape restarts from the arrangement: simulate again."
        )))
    }

    fn agent_grading(&mut self, args: &Value) -> Result<ToolResult, String> {
        use super::grading_ui::{Change, edit};
        use fashioncraft::garment::grading::Target;
        let text = |key: &str| -> Result<String, String> {
            args.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("'{key}' is missing"))
        };
        let group = self
            .project
            .template
            .grading
            .as_ref()
            .and_then(|g| g.groups.first())
            .cloned();
        let size_id = |name: &str| -> Result<String, String> {
            let g = group
                .as_ref()
                .ok_or("The garment has no sizes yet: create them first")?;
            g.sizes
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(name) || s.id == name)
                .map(|s| s.id.clone())
                .ok_or_else(|| {
                    let names: Vec<&str> = g.sizes.iter().map(|s| s.name.as_str()).collect();
                    format!("No size '{name}'. The sizes are: {}", names.join(", "))
                })
        };
        let change = match args.get("action").and_then(Value::as_str).unwrap_or("") {
            "create" => {
                if group.is_some() {
                    return Err("The garment already has sizes: add_size adds more".into());
                }
                let sizes: Vec<String> = args
                    .get("sizes")
                    .and_then(Value::as_array)
                    .map(|v| {
                        v.iter()
                            .filter_map(|s| Some(s.as_str()?.to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                let base = text("base")?;
                let at = sizes
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(&base))
                    .ok_or("The base must be one of the sizes")?;
                Change::Create(sizes, at)
            }
            "add_size" => {
                let g = group.as_ref().ok_or("Create sizes first")?.id.clone();
                let name = text("size")?;
                let after = match args.get("after").and_then(Value::as_str) {
                    Some(a) => Some(size_id(a)?),
                    None => None,
                };
                edit("Add size", move |grading, _| {
                    grading.add_size(&g, &name, after.as_deref()).map(|_| ())
                })
            }
            "set_step" => {
                let panel = agent_ops::find_piece(&self.project.template, &text("piece")?)?
                    .id
                    .clone();
                let segment = text("segment")?;
                let step = glam::DVec2::new(
                    args.get("dx_mm").and_then(Value::as_f64).unwrap_or(0.),
                    args.get("dy_mm").and_then(Value::as_f64).unwrap_or(0.),
                ) / 1000.;
                let target = Target::Point {
                    panel,
                    edge: segment,
                };
                edit("Grade rule", move |grading, _| {
                    grading.set_step(&target, step)
                })
            }
            "show" => {
                let g = group.as_ref().ok_or("Create sizes first")?.id.clone();
                Change::Show(g, size_id(&text("size")?)?)
            }
            other => {
                return Err(format!(
                    "action '{other}': create, add_size, set_step or show"
                ));
            }
        };
        let before = serde_json::to_string(&self.project.template.grading).unwrap_or_default();
        self.apply_grading_change(change);
        if serde_json::to_string(&self.project.template.grading).unwrap_or_default() == before {
            return Err(format!("Nothing changed: {}", self.status));
        }
        Ok(ToolResult::json(&json!({
            "done": self.status,
            "sizes": agent_ops::describe_grading(&self.project.template),
        })))
    }

    fn agent_render(&mut self, args: &Value) -> Result<Pending, String> {
        let name = fashioncraft::agent::plain_name(
            args.get("name").and_then(Value::as_str).unwrap_or(""),
        )?;
        let seconds = args
            .get("seconds")
            .and_then(Value::as_f64)
            .unwrap_or(20.)
            .clamp(3., 120.);
        if self.authoring.selection.garment_rect.is_none() {
            return Err(
                "The 3D view is not on screen: show it (Window › 3D only or Split view)".into(),
            );
        }
        let dir = fashioncraft::agent::documents_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let was_open = self.authoring.render.open;
        if !was_open {
            self.open_photoreal_render();
        }
        let started = Instant::now();
        Ok(Pending::Render {
            started,
            until: started + Duration::from_secs_f64(seconds),
            path: dir.join(format!("{}.png", name.trim_end_matches(".png"))),
            saved: None,
            was_open,
        })
    }

    /// The pieces an agent named, or every piece; unknown ids are an error.
    fn named_pieces(&self, args: &Value) -> Result<Vec<String>, String> {
        let panels = &self.project.template.panels;
        match args.get("pieces").and_then(Value::as_array) {
            None => Ok(panels.iter().map(|p| p.id.clone()).collect()),
            Some(ids) => ids
                .iter()
                .map(|id| {
                    agent_ops::find_piece(&self.project.template, id.as_str().unwrap_or(""))
                        .map(|p| p.id.clone())
                })
                .collect(),
        }
    }

    fn agent_fabric(&mut self, args: &Value) -> Result<ToolResult, String> {
        let id = args.get("preset").and_then(Value::as_str).unwrap_or("");
        let preset = fashioncraft::fabrics::PRESETS
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("no fabric preset '{id}'"))?;
        let pieces = self.named_pieces(args)?;
        let every = args.get("pieces").is_none();
        let name = format!("{} (approx.)", preset.name);
        for (k, panel) in pieces.iter().enumerate() {
            if every && k > 0 {
                break;
            }
            if !self.edit(Command::Fabric {
                panel: panel.clone(),
                name: name.clone(),
                properties: preset.properties(),
                all_panels: every,
            }) {
                return Err(self.status.clone());
            }
        }
        Ok(ToolResult::text(format!(
            "{} on {} piece(s). Indicative values: calibrate on the real fabric.",
            preset.name,
            pieces.len()
        )))
    }

    fn agent_color(&mut self, args: &Value) -> Result<ToolResult, String> {
        let rgb: Vec<u8> = args
            .get("rgb")
            .and_then(Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(|c| c.as_u64().map(|c| c.min(255) as u8))
                    .collect()
            })
            .unwrap_or_default();
        let [r, g, b] = rgb[..] else {
            return Err("rgb needs three numbers from 0 to 255".into());
        };
        let pieces = self.named_pieces(args)?;
        let mut fabrics: Vec<String> = self
            .project
            .template
            .panels
            .iter()
            .filter(|p| pieces.contains(&p.id))
            .map(|p| p.fabric.clone())
            .collect();
        fabrics.sort();
        fabrics.dedup();
        for fabric in &fabrics {
            let mut appearance = self
                .project
                .template
                .fabric_appearance
                .get(fabric)
                .cloned()
                .unwrap_or_default();
            appearance.color_srgb = [r, g, b];
            if !self.edit(Command::FabricAppearance {
                fabric: fabric.clone(),
                appearance: Some(appearance),
            }) {
                return Err(self.status.clone());
            }
        }
        Ok(ToolResult::text(format!(
            "Coloured {} fabric(s) rgb({r}, {g}, {b}); every piece of the same fabric changes too.",
            fabrics.len()
        )))
    }

    fn agent_save(&mut self, ctx: &egui::Context, args: &Value) -> Result<ToolResult, String> {
        let name = fashioncraft::agent::plain_name(
            args.get("name").and_then(Value::as_str).unwrap_or(""),
        )?;
        let dir = fashioncraft::agent::documents_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let name = name.strip_suffix(".json").unwrap_or(&name).to_owned();
        let saved = self.save_document_to(ctx, dir.join(format!("{name}.json")))?;
        Ok(ToolResult::text(format!("Saved {saved}")))
    }

    fn agent_export(&mut self, ctx: &egui::Context, args: &Value) -> Result<ToolResult, String> {
        let name = fashioncraft::agent::plain_name(
            args.get("name").and_then(Value::as_str).unwrap_or(""),
        )?;
        let dir = fashioncraft::agent::documents_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = match args.get("format").and_then(Value::as_str) {
            Some("obj") => {
                let path = dir.join(format!("{name}.obj"));
                self.export_obj_to(ctx, &path)?;
                path
            }
            Some("glb") => {
                let path = dir.join(format!("{name}.glb"));
                self.export_gltf_to(&path)?;
                path
            }
            other => return Err(format!("unknown format {other:?}: obj or glb")),
        };
        Ok(ToolResult::text(format!("Exported {}", path.display())))
    }
}

fn usage_text(usage: commands::AgentUse) -> &'static str {
    use commands::AgentUse as U;
    match usage {
        U::Runs => "runs",
        U::Mouse => "waits for clicks or drags with the mouse",
        U::Window => "opens a window for a person to fill in",
        U::Dialog => "opens a file dialog",
        U::Web => "opens a web page",
    }
}
