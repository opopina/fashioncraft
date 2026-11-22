//! What each agent tool does in the window (`fashioncraft::agent::tools` describes them). They
//! run on the interface thread through the same commands as the menus, so every change is
//! undoable and shows on screen as it happens.
use super::agent_hub::Pending;
use super::*;
use fashioncraft::agent::{Content, ToolResult};
use fashioncraft::authoring::Command;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Menu commands that open a native dialog, which would wait for a person: a file dialog, or
/// File › New's "save changes?". Agents use save_project, export_garment and draft_garment.
const DIALOG_PREFIXES: [&str; 10] = [
    "file.new",
    "file.open",
    "file.import",
    "file.add_",
    "file.save",
    "file.python_script",
    "file.recover",
    "export.",
    "animation.export",
    "file.templates",
];

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
                "command_list" => Ok(ToolResult::json(
                    &self.command_list(args.get("query").and_then(Value::as_str).unwrap_or("")),
                )),
                "command_run" => self.agent_command(ctx, args.get("key").and_then(Value::as_str)),
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
                "save_project" => self.agent_save(ctx, args),
                "export_garment" => self.agent_export(ctx, args),
                other => Err(format!("unknown tool '{other}'")),
            };
        Ok(result.unwrap_or_else(ToolResult::error))
    }

    /// Finishes a call started in an earlier frame, or None while it still waits.
    pub(super) fn continue_tool(
        &mut self,
        ctx: &egui::Context,
        pending: &Pending,
    ) -> Option<ToolResult> {
        match pending {
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
        }
    }

    fn screenshot_result(
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

    fn scene_summary(&self) -> Value {
        let template = &self.project.template;
        let pieces: Vec<Value> = template
            .panels
            .iter()
            .map(|p| {
                let t = p.placement.translation;
                json!({
                    "id": p.id,
                    "name": p.name,
                    "fabric": p.fabric,
                    "placed_at_cm": [centimetres(t.x), centimetres(t.y), centimetres(t.z)],
                    "rotation_degrees": [p.placement.rotation_degrees.x.round(), p.placement.rotation_degrees.y.round(), p.placement.rotation_degrees.z.round()],
                })
            })
            .collect();
        let seams: Vec<Value> = template
            .seams
            .iter()
            .map(|s| {
                json!({
                    "id": s.id,
                    "a": format!("{}.{}", s.a.panel, s.a.edge.as_deref().unwrap_or("?")),
                    "b": format!("{}.{}", s.b.panel, s.b.edge.as_deref().unwrap_or("?")),
                })
            })
            .collect();
        let fabrics: Vec<Value> = template
            .fabrics
            .iter()
            .map(|f| {
                let colour = template.fabric_appearance.get(&f.id).map(|a| a.color_srgb);
                json!({"id": f.id, "name": f.name, "rgb": colour})
            })
            .collect();
        let x = &self.scene.state.x;
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
            })
        };
        let body = &self.scene.collider.surface.vertices;
        let height = body.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max)
            - body.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
        json!({
            "pieces": pieces,
            "seams": seams,
            "fabrics": fabrics,
            "avatar": {
                "file": self.project.avatar_path.file_name().map(|n| n.to_string_lossy()),
                "height_cm": if height.is_finite() { centimetres(height) } else { 0. },
            },
            "garment": garment,
            "simulation_running": self.playing,
            "unsaved_changes": self.document.dirty,
            "fabric_presets": fashioncraft::fabrics::PRESETS.iter().map(|p| json!({"id": p.id, "name": p.name})).collect::<Vec<_>>(),
        })
    }

    fn command_list(&self, query: &str) -> Value {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let context = self.command_context();
        let commands: Vec<Value> = commands::REGISTRY
            .iter()
            .filter(|d| {
                let text = format!("{} {}", d.key, d.label).to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .map(|d| {
                json!({
                    "key": d.key,
                    "label": d.label.trim_end_matches('…'),
                    "available": commands::availability(d.id, context).is_ok(),
                    "opens_a_dialog": DIALOG_PREFIXES.iter().any(|p| d.key.starts_with(p)),
                })
            })
            .collect();
        json!({"commands": commands})
    }

    fn agent_command(
        &mut self,
        ctx: &egui::Context,
        key: Option<&str>,
    ) -> Result<ToolResult, String> {
        let key = key.ok_or("command_run needs a key")?;
        let descriptor = commands::REGISTRY
            .iter()
            .find(|d| d.key == key)
            .ok_or_else(|| format!("no command '{key}': see command_list"))?;
        if DIALOG_PREFIXES.iter().any(|p| key.starts_with(p)) {
            return Err(format!(
                "'{key}' opens a dialog; use save_project, export_garment or draft_garment instead"
            ));
        }
        self.dispatch_command(descriptor.id, ctx)?;
        Ok(ToolResult::text(format!(
            "Ran {key} ({}). {}",
            descriptor.label.trim_end_matches('…'),
            self.status
        )))
    }

    /// The pieces an agent named, or every piece; unknown ids are an error.
    fn named_pieces(&self, args: &Value) -> Result<Vec<String>, String> {
        let panels = &self.project.template.panels;
        match args.get("pieces").and_then(Value::as_array) {
            None => Ok(panels.iter().map(|p| p.id.clone()).collect()),
            Some(ids) => ids
                .iter()
                .map(|id| {
                    let id = id.as_str().unwrap_or("");
                    panels
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.id.clone())
                        .ok_or_else(|| format!("no piece '{id}': see scene_summary"))
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
