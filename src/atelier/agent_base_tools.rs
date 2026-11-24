//! What the base tools (`fashioncraft::agent_base`) do in the window: the generic edit, the cm
//! equivalents of the mouse tools, the form windows as structured tools, the exact detail
//! engines and the garment check. Like the first tools they run on the interface thread
//! through the same commands as the menus, so every pattern change is undoable.
use super::agent_hub::Pending;
use super::*;
use fashioncraft::agent::{Content, ToolResult};
use fashioncraft::agent_build::{self, Build, Lay};
use fashioncraft::authoring::Command;
use fashioncraft::{agent_base, agent_check, agent_edit, agent_ops};
use glam::{DVec2, DVec3};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Marks the screenshots these tools ask for among the window's screenshots.
struct BaseShot;

/// A base tool that finishes over later frames.
pub(super) enum BasePending {
    /// The cloth is pulled until `until`, then let go.
    Grab { until: Instant, seconds: f64 },
    /// The fabric fit runs on its own thread.
    Fit {
        thread: std::thread::JoinHandle<Result<fashioncraft::fabric_kit::FitReport, String>>,
        name: String,
        pieces: Vec<String>,
    },
    /// A picture of the 3D view once the camera has moved: frames waited, when it was asked
    /// for, and the text that goes with it.
    Shot {
        frames: u32,
        asked: Option<Instant>,
        text: String,
    },
}

fn cm(m: f64) -> f64 {
    agent_ops::cm(m)
}

fn triple(v: Option<&Value>) -> Option<DVec3> {
    let v = v?;
    Some(
        DVec3::new(
            v.get(0)?.as_f64()?,
            v.get(1)?.as_f64()?,
            v.get(2)?.as_f64()?,
        ) / 100.,
    )
}

fn pair(v: Option<&Value>) -> Option<DVec2> {
    let v = v?;
    Some(DVec2::new(v.get(0)?.as_f64()?, v.get(1)?.as_f64()?) / 100.)
}

fn number(args: &Value, key: &str) -> Option<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}

impl Atelier {
    /// Starts base tool `name`, as `begin_tool` does for the first ones.
    pub(super) fn begin_base_tool(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        args: &Value,
    ) -> Result<ToolResult, Pending> {
        if let Some(answer) = agent_base::answer_without_window(name, args) {
            return Ok(answer);
        }
        let result = match name {
            "edit" => self.agent_generic_edit(args),
            "transform_piece" => self.agent_transform(args),
            "bend_segment" => agent_build::bend_segment(&self.project.template, args)
                .and_then(|b| self.apply_build(b)),
            "add_internal_line" => agent_build::internal_line(&self.project.template, args)
                .and_then(|b| self.apply_build(b)),
            "add_buttons" => {
                agent_build::buttons(&self.project.template, args).and_then(|b| self.apply_build(b))
            }
            "add_detail" => self.ready().and_then(|()| {
                agent_build::add_detail(&self.project.template, args)
                    .and_then(|b| self.apply_build(b))
            }),
            "style_line" => self.agent_style_line(args),
            "pin" => self.agent_pin(args),
            "grab" => match self.agent_grab(args) {
                Ok(pending) => return Err(pending),
                Err(e) => Err(e),
            },
            "trace_on_body" => self.agent_trace(args),
            "simulation_settings" => self.agent_simulation_settings(args),
            "camera" => match self.agent_camera(ctx, args) {
                Ok(Some(pending)) => return Err(pending),
                Ok(None) => Ok(ToolResult::json(&self.camera_json())),
                Err(e) => Err(e),
            },
            "render_settings" => self.agent_render_settings(args),
            "avatar_maker" => self.agent_avatar_maker(args),
            "pose_joints" => self.agent_pose_joints(args),
            "measure_fabric" => match self.agent_measure_fabric(args) {
                Ok(pending) => return Err(pending),
                Err(e) => Err(e),
            },
            "points_of_measure" => self.agent_points_of_measure(args),
            "measure" => self.agent_measure(args),
            "check_garment" => {
                let report = self.garment_check();
                match args.get("view").and_then(Value::as_str) {
                    Some(view) => match self.turn_to(ctx, view) {
                        Ok(()) => {
                            return Err(Pending::Base(BasePending::Shot {
                                frames: 0,
                                asked: None,
                                text: serde_json::to_string_pretty(&report).unwrap_or_default(),
                            }));
                        }
                        Err(e) => Err(e),
                    },
                    None => Ok(ToolResult::json(&report)),
                }
            }
            other => Err(format!("unknown tool '{other}'")),
        };
        Ok(result.unwrap_or_else(ToolResult::error))
    }

    /// Finishes a base tool started in an earlier frame, or None while it still waits.
    pub(super) fn continue_base_tool(
        &mut self,
        ctx: &egui::Context,
        pending: &mut BasePending,
    ) -> Option<ToolResult> {
        match pending {
            BasePending::Grab { until, seconds } => {
                if Instant::now() < *until && self.playing && self.grab.is_some() {
                    ctx.request_repaint();
                    return None;
                }
                self.grab = None;
                if self.playing {
                    self.invoke_command(commands::CommandId::ToggleSimulation, ctx);
                }
                let summary = self.scene_summary();
                Some(ToolResult::json(&json!({
                    "pulled_for_seconds": seconds,
                    "let_go": true,
                    "seams": summary["seams"],
                    "warnings": summary["warnings"],
                    "status": self.status,
                })))
            }
            BasePending::Fit { thread, .. } if !thread.is_finished() => None,
            BasePending::Fit { .. } => {
                let BasePending::Fit {
                    thread,
                    name,
                    pieces,
                } = std::mem::replace(
                    pending,
                    BasePending::Shot {
                        frames: 0,
                        asked: Some(Instant::now()),
                        text: String::new(),
                    },
                )
                else {
                    return None;
                };
                Some(
                    match thread.join() {
                        Ok(Ok(report)) => self.use_measured_fabric(&name, &pieces, report),
                        Ok(Err(e)) => Err(format!("The fit failed: {e}")),
                        Err(_) => Err("The fit stopped unexpectedly".into()),
                    }
                    .unwrap_or_else(ToolResult::error),
                )
            }
            BasePending::Shot {
                frames,
                asked,
                text,
            } => {
                let Some(when) = *asked else {
                    // Let the view draw the new camera before taking it.
                    *frames += 1;
                    ctx.request_repaint();
                    if *frames >= 3 {
                        if ctx.input(|i| i.viewport().visible() == Some(false))
                            || self.authoring.selection.garment_rect.is_none()
                        {
                            return Some(ToolResult::text(format!(
                                "{text}\n(No picture: the 3D view is not on screen.)"
                            )));
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                            egui::UserData::new(BaseShot),
                        ));
                        *asked = Some(Instant::now());
                    }
                    return None;
                };
                let shot = ctx.input(|i| {
                    i.events.iter().find_map(|event| match event {
                        egui::Event::Screenshot {
                            user_data, image, ..
                        } if user_data
                            .data
                            .as_ref()
                            .is_some_and(|d| d.downcast_ref::<BaseShot>().is_some()) =>
                        {
                            Some(image.clone())
                        }
                        _ => None,
                    })
                });
                let Some(image) = shot else {
                    return (when.elapsed() > Duration::from_secs(10))
                        .then(|| ToolResult::text(format!("{text}\n(No picture came back.)")));
                };
                let mut result = self
                    .screenshot_result(ctx, &image, false)
                    .unwrap_or_else(ToolResult::error);
                if !text.is_empty() {
                    result
                        .content
                        .insert(0, Content::Text { text: text.clone() });
                }
                Some(result)
            }
        }
    }

    /// Edits that lay new cloth on the drape need the simulation between steps.
    fn ready(&self) -> Result<(), String> {
        if self.pending {
            Err("Wait for the current simulation step to finish, then try again".into())
        } else {
            Ok(())
        }
    }

    fn agent_generic_edit(&mut self, args: &Value) -> Result<ToolResult, String> {
        let op = args
            .get("op")
            .and_then(Value::as_str)
            .ok_or("edit needs op (edit_ops lists them)")?;
        let fields = args.get("fields").cloned().unwrap_or(Value::Null);
        let command = agent_edit::parse(op, &fields)?;
        if !self.edit(command) {
            return Err(format!("{op} was refused: {}", self.status));
        }
        Ok(ToolResult::text(format!(
            "{op} applied (undoable). {}",
            self.status
        )))
    }

    /// Applies a built change through the history, then lays a new piece on the drape.
    fn apply_build(&mut self, build: Build) -> Result<ToolResult, String> {
        // The drape before the change: a new piece is laid on it, not on whatever the window
        // falls back to when the piece's default spot is in the way.
        let before = build.lay.as_ref().map(|_| {
            (
                self.project.template.clone(),
                self.scene.garment.clone(),
                self.scene.state.clone(),
                build.commands.clone(),
            )
        });
        if !self.edit_sequence(build.commands, &build.names, build.history_name) {
            return Err(self.status.clone());
        }
        if let (Some(lay), Some(before)) = (&build.lay, before)
            && let Err(e) = self.lay_new_piece(lay, before)
        {
            // Left where new pieces are laid out, it would be dragged through the garment when
            // simulated: take the change back instead.
            self.undo_edit(false);
            return Err(format!(
                "The new piece could not start on the garment ({e}), so nothing was changed. \
                 Add details before simulating (right after draft_garment), or reset the drape \
                 with command_run simulation.reset, add it, then simulate"
            ));
        }
        let done = build.done;
        Ok(ToolResult::text(done))
    }

    /// A point on the body's vertical axis (or the garment's middle without a body).
    fn body_axis(&self) -> DVec3 {
        let surface = &self.scene.collider.surface;
        if surface.vertices.is_empty() || self.project.avatar_removed {
            self.scene.state.x.iter().sum::<DVec3>() / self.scene.state.x.len().max(1) as f64
        } else {
            surface.vertices.iter().sum::<DVec3>() / surface.vertices.len() as f64
        }
    }

    fn lay_new_piece(
        &mut self,
        lay: &Lay,
        (template, garment, state, commands): (
            fashioncraft::garment::GarmentTemplate,
            std::sync::Arc<fashioncraft::garment::CompiledGarment>,
            fashioncraft::model::State,
            Vec<Command>,
        ),
    ) -> Result<(), String> {
        let body = self.scene.collider.surface.clone();
        // Every other piece where it was draped (as far as the change allows).
        let start = agent_build::compose(&template, &commands)
            .map(|edit| edit.state_keeping_drape(&garment, &state))
            .ok()
            .filter(|s| s.x.len() == self.scene.state.x.len())
            .unwrap_or_else(|| self.scene.state.clone());
        let base = self.settings.contact.radius * 2. + 0.002;
        let mut last = String::new();
        // Off the cloth by the contact distance, or further where the drape curves or folds.
        for clearance in [base, 2. * base, 4. * base] {
            let placed = agent_build::lay(
                &self.scene.garment,
                &start.x,
                lay,
                clearance,
                self.body_axis(),
            );
            if placed.is_empty() {
                return Err("nothing to lay".into());
            }
            let mut candidate = start.clone();
            for (v, p) in placed {
                if v < candidate.x.len() {
                    candidate.x[v] = p;
                    candidate.v[v] = DVec3::ZERO;
                }
            }
            match fashioncraft::vbd::VbdSolver::new(
                self.scene.garment.clone(),
                Some(&body),
                &candidate,
            ) {
                Ok(_) => {
                    self.playing = false;
                    self.scene.state = candidate;
                    self.issue.invalidate();
                    self.generation += 1;
                    self.metrics = None;
                    self.grab = None;
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(self.name_faces(&last))
    }

    /// The solver's "faces A and B" with the piece (or the avatar) each belongs to.
    fn name_faces(&self, message: &str) -> String {
        let garment = &self.scene.garment;
        let owner = |face: usize| {
            garment
                .panels
                .iter()
                .find(|p| p.triangles.contains(&face))
                .map_or_else(|| "the avatar".to_owned(), |p| p.id.clone())
        };
        let faces: Vec<usize> = message
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|w| w.parse().ok())
            .collect();
        let x = &self.scene.state.x;
        let at = |face: usize| {
            garment
                .mesh
                .triangles
                .get(face)
                .map(|t| {
                    let c = t.vertices.iter().map(|v| x[*v]).sum::<DVec3>() / 3.;
                    let uv = t
                        .vertices
                        .iter()
                        .map(|v| garment.mesh.uv[*v])
                        .sum::<DVec2>()
                        / 3.;
                    format!(
                        " (at ({:.1}, {:.1}) cm on the piece, {:.0} cm up)",
                        uv.x * 100.,
                        uv.y * 100.,
                        c.y * 100.
                    )
                })
                .unwrap_or_default()
        };
        match faces[..] {
            [a, b, ..] => format!(
                "{message}: {}{} touches {}{}",
                owner(a),
                at(a),
                owner(b),
                at(b)
            ),
            _ => message.to_owned(),
        }
    }

    fn agent_transform(&mut self, args: &Value) -> Result<ToolResult, String> {
        let template = &self.project.template;
        let build = match args.get("space").and_then(Value::as_str) {
            Some("board") => agent_build::move_on_board(template, args)?,
            Some("3d") => {
                let mut ids: Vec<String> = Vec::new();
                if let Some(p) = args.get("piece").and_then(Value::as_str) {
                    ids.push(agent_ops::find_piece(template, p)?.id.clone());
                }
                for p in args
                    .get("pieces")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    ids.push(
                        agent_ops::find_piece(template, p.as_str().unwrap_or(""))?
                            .id
                            .clone(),
                    );
                }
                let x = &self.scene.state.x;
                let vertices: Vec<usize> = self
                    .scene
                    .garment
                    .panels
                    .iter()
                    .filter(|p| ids.contains(&p.id))
                    .flat_map(|p| p.vertices.clone())
                    .filter(|v| *v < x.len())
                    .collect();
                let pivot =
                    vertices.iter().map(|v| x[*v]).sum::<DVec3>() / vertices.len().max(1) as f64;
                agent_build::move_in_3d(template, args, pivot)?
            }
            _ => return Err("space: board or 3d".into()),
        };
        self.apply_build(build)
    }

    fn agent_style_line(&mut self, args: &Value) -> Result<ToolResult, String> {
        use super::style_lines::{StyleMode, edge_pick_at, seam_partners, style_command};
        let template = &self.project.template;
        let panel = agent_ops::find_piece(
            template,
            args.get("piece").and_then(Value::as_str).unwrap_or(""),
        )?
        .id
        .clone();
        let segment = args
            .get("segment")
            .and_then(Value::as_str)
            .ok_or("'segment' is missing")?;
        let mode = match args.get("mode").and_then(Value::as_str) {
            Some("edit") => StyleMode::Edit,
            Some("move") => StyleMode::Move,
            Some("scale") => StyleMode::Scale,
            _ => return Err("mode: edit, move or scale".into()),
        };
        let w = triple(args.get("move_cm")).ok_or("move_cm: [x, y, z] in world cm")?;
        let at = number(args, "at").unwrap_or(0.5).clamp(0.01, 0.99);
        let pick = edge_pick_at(
            &self.scene.garment,
            &self.scene.state.x,
            &panel,
            segment,
            at,
        )
        .ok_or_else(|| format!("{panel}.{segment} is not an outline segment on the cloth"))?;
        let mut picks = vec![pick.clone()];
        if args.get("across_seams").and_then(Value::as_bool) != Some(false) {
            picks.extend(seam_partners(
                template,
                &self.scene.garment,
                &self.scene.state.x,
                &pick,
            ));
        }
        let command =
            style_command(template, mode, &picks, w).ok_or("That move changes nothing")?;
        let partners = picks.len() - 1;
        if !self.edit(command) {
            return Err(self.status.clone());
        }
        Ok(ToolResult::text(format!(
            "Style line {panel}.{segment} changed{}; seams along it take up the length \
             difference. The drape is kept: simulate to settle",
            if partners > 0 {
                format!(" with {partners} segment(s) across its seams")
            } else {
                String::new()
            }
        )))
    }

    /// The cloth points of a piece within `radius` of its material point `at`.
    fn points_near(&self, panel: &str, at: DVec2, radius: f64) -> Vec<usize> {
        let garment = &self.scene.garment;
        garment
            .panels
            .iter()
            .find(|p| p.id == panel)
            .map(|p| {
                p.vertices
                    .clone()
                    .filter(|v| garment.mesh.uv[*v].distance(at) <= radius)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn agent_pin(&mut self, args: &Value) -> Result<ToolResult, String> {
        self.ready()?;
        let action = args.get("action").and_then(Value::as_str).unwrap_or("");
        let before = self.authoring.history.done().count();
        if action == "remove_all" {
            self.delete_all_pins();
        } else {
            let panel = agent_ops::find_piece(
                &self.project.template,
                args.get("piece").and_then(Value::as_str).unwrap_or(""),
            )?
            .id
            .clone();
            let at = pair(args.get("at_cm")).ok_or("at_cm: [x, y] on the piece, cm")?;
            let radius = number(args, "radius_cm").unwrap_or(2.) / 100.;
            let mut vertices = self.points_near(&panel, at, radius);
            if vertices.is_empty() {
                return Err(format!(
                    "No cloth of {panel} within {:.1} cm of ({:.1}, {:.1}) cm: see piece_detail",
                    radius * 100.,
                    at.x * 100.,
                    at.y * 100.
                ));
            }
            match action {
                "add" => self.apply_pins(&vertices, true),
                "remove" => self.apply_pins(&vertices, false),
                "move" => {
                    let by = triple(args.get("move_cm")).ok_or("move_cm: [x, y, z] world cm")?;
                    vertices.retain(|v| self.scene.garment.authored_holds[*v]);
                    if vertices.is_empty() {
                        return Err("No pins there to move".into());
                    }
                    self.apply_pin_move(&vertices, by);
                }
                _ => return Err("action: add, remove, remove_all or move".into()),
            }
        }
        if self.authoring.history.done().count() == before {
            return Err(self.status.clone());
        }
        Ok(ToolResult::text(format!("{} (undoable)", self.status)))
    }

    fn agent_grab(&mut self, args: &Value) -> Result<Pending, String> {
        self.ready()?;
        let panel = agent_ops::find_piece(
            &self.project.template,
            args.get("piece").and_then(Value::as_str).unwrap_or(""),
        )?
        .id
        .clone();
        let at = pair(args.get("at_cm")).ok_or("at_cm: [x, y] on the piece, cm")?;
        let pull = triple(args.get("pull_cm")).ok_or("pull_cm: [x, y, z] world cm")?;
        if pull.length() > 1. {
            return Err("A pull of more than 1 m is surely a mistake".into());
        }
        let seconds = number(args, "seconds").unwrap_or(3.).clamp(0.5, 20.);
        let garment = &self.scene.garment;
        let weights = garment
            .material_point(&panel, at)
            .filter(|w| !w.is_empty())
            .ok_or("No cloth there: see piece_detail")?;
        let mut vertices = [weights[0].0; 3];
        let mut w = [0.; 3];
        for (k, (v, weight)) in weights.iter().take(3).enumerate() {
            vertices[k] = *v;
            w[k] = *weight;
        }
        let x = &self.scene.state.x;
        let point: DVec3 = vertices.iter().zip(w).map(|(v, w)| x[*v] * w).sum();
        let range = garment
            .panels
            .iter()
            .find(|p| p.id == panel)
            .ok_or("No such piece on the cloth")?;
        let patch = fashioncraft::vbd::grab::surface_patch(
            &garment.mesh,
            x,
            range.triangles.clone(),
            vertices,
            point,
            0.03,
        );
        self.grab = Some(Grab {
            patch,
            vertices,
            weights: w,
            target: point + pull,
            stiffness: 150. * self.grab_strength,
        });
        self.playing = true;
        Ok(Pending::Base(BasePending::Grab {
            until: Instant::now() + Duration::from_secs_f64(seconds),
            seconds,
        }))
    }

    fn agent_trace(&mut self, args: &Value) -> Result<ToolResult, String> {
        self.ready()?;
        if !self.avatar_shown() || self.scene.collider.surface.vertices.is_empty() {
            return Err("Show the avatar to trace on it".into());
        }
        let surface = self.scene.collider.surface.clone();
        let mut vertices: Vec<usize> = Vec::new();
        for p in args
            .get("points_cm")
            .and_then(Value::as_array)
            .ok_or("points_cm: [[x, y, z], …] world cm")?
        {
            let p = triple(Some(p)).ok_or("Each point is [x, y, z] in cm")?;
            let (on, triangle, _) = self.scene.collider.nearest_point(p);
            let corner = surface.triangles[triangle]
                .into_iter()
                .min_by(|a, b| {
                    surface.vertices[*a]
                        .distance(on)
                        .total_cmp(&surface.vertices[*b].distance(on))
                })
                .unwrap_or(0);
            if vertices.last() != Some(&corner) {
                vertices.push(corner);
            }
        }
        if vertices.len() < 3 {
            return Err("Give at least three distinct points on the body".into());
        }
        let before = self.project.template.panels.len();
        self.trace_through(vertices);
        if self.project.template.panels.len() == before {
            return Err(self.status.clone());
        }
        let id = self
            .project
            .template
            .panels
            .last()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        Ok(ToolResult::text(format!(
            "Traced piece {id}. {} (undoable)",
            self.status
        )))
    }

    fn simulation_json(&self) -> Value {
        let s = &self.settings;
        let quality = super::environment_ui::PRESETS
            .iter()
            .find(|(_, dt, it, _)| (dt - s.dt).abs() < 1e-9 && *it == s.iterations)
            .map(|(n, ..)| n.to_lowercase());
        json!({
            "quality": quality,
            "time_step_s": s.dt,
            "iterations": s.iterations,
            "gpu": self.gpu,
            "damping": s.damping,
            "friction": s.friction,
            "collision_distance_mm": (s.contact.radius * 10000.).round() / 10.,
            "gravity_m_s2": -s.gravity.y,
            "wind_m_s": s.wind.as_ref().map(|w| (w.velocity.length() * 10.).round() / 10.),
            "floor_height_cm": s.floor.map(cm),
            "particle_distance_mm": self.mesh_mm,
            "full_speed_falls": s.max_substeps > 4,
        })
    }

    fn agent_simulation_settings(&mut self, args: &Value) -> Result<ToolResult, String> {
        let presets: Vec<(&str, f64, usize)> = super::environment_ui::PRESETS
            .iter()
            .map(|(n, dt, it, _)| (*n, *dt, *it))
            .collect();
        let body_floor = self
            .scene
            .collider
            .surface
            .vertices
            .iter()
            .map(|v| v.y)
            .fold(f64::INFINITY, f64::min);
        let mut changed =
            agent_base::change_simulation(&mut self.settings, args, &presets, body_floor)?;
        if let Some(b) = args.get("gpu").and_then(Value::as_bool) {
            self.gpu = b;
            changed.push("gpu");
        }
        let mut note = String::new();
        if let Some(mm) = number(args, "particle_distance_mm") {
            self.mesh_mm = mm.clamp(8., 60.);
            self.remesh();
            changed.push("particle distance");
            note = " The cloth was remeshed and the drape restarted: simulate.".into();
        }
        Ok(ToolResult::json(&json!({
            "changed": changed,
            "settings": self.simulation_json(),
            "note": format!("Settings apply from the next simulation step.{note}"),
        })))
    }

    fn camera_json(&self) -> Value {
        let c = &self.camera;
        json!({
            "yaw_degrees": (c.yaw.to_degrees() * 10.).round() / 10.,
            "pitch_degrees": (c.pitch.to_degrees() * 10.).round() / 10.,
            "distance_cm": cm(f64::from(c.distance)),
            "target_cm": [cm(f64::from(c.target.x)), cm(f64::from(c.target.y)), cm(f64::from(c.target.z))],
            "fov_degrees": c.fov_degrees,
            "orthographic": c.orthographic,
        })
    }

    /// Turns the 3D view to a named view, as the View menu does.
    fn turn_to(&mut self, ctx: &egui::Context, view: &str) -> Result<(), String> {
        use commands::CommandId as Id;
        let id = match view {
            "front" => Id::ViewFront,
            "back" => Id::ViewBack,
            "left" => Id::ViewAvatarLeft,
            "right" => Id::ViewAvatarRight,
            "three_quarter_left" => Id::ViewThreeQuarterLeft,
            "three_quarter_right" => Id::ViewThreeQuarterRight,
            "top" => Id::ViewTop,
            "bottom" => Id::ViewBottom,
            "isometric" => {
                self.camera
                    .apply_preset(crate::render::PresetView::Isometric);
                return Ok(());
            }
            other => return Err(format!("No view '{other}'")),
        };
        self.invoke_command(id, ctx);
        Ok(())
    }

    fn agent_camera(
        &mut self,
        ctx: &egui::Context,
        args: &Value,
    ) -> Result<Option<Pending>, String> {
        if let Some(view) = args.get("view").and_then(Value::as_str) {
            self.turn_to(ctx, view)?;
        }
        let c = &mut self.camera;
        if let Some(v) = number(args, "yaw_degrees") {
            c.yaw = (v.rem_euclid(360.) as f32).to_radians();
        }
        if let Some(v) = number(args, "pitch_degrees") {
            c.pitch = (v.clamp(-89.9, 89.9) as f32).to_radians();
        }
        if let Some(v) = number(args, "distance_cm") {
            c.distance = ((v / 100.) as f32).clamp(0.15, 30.);
        }
        if let Some(t) = triple(args.get("target_cm")) {
            c.target = glam::Vec3::new(t.x as f32, t.y as f32, t.z as f32);
        }
        if let Some(v) = number(args, "fov_degrees") {
            c.fov_degrees = (v as f32).clamp(10., 120.);
            self.prefs.camera_fov_degrees = c.fov_degrees;
        }
        if let Some(o) = args.get("orthographic").and_then(Value::as_bool) {
            c.orthographic = o;
        }
        if args.get("frame").and_then(Value::as_bool) == Some(true) {
            self.frame_selection_or_all();
        }
        ctx.request_repaint();
        if args.get("screenshot").and_then(Value::as_bool) == Some(true) {
            return Ok(Some(Pending::Base(BasePending::Shot {
                frames: 0,
                asked: None,
                text: serde_json::to_string(&self.camera_json()).unwrap_or_default(),
            })));
        }
        Ok(None)
    }

    fn agent_render_settings(&mut self, args: &Value) -> Result<ToolResult, String> {
        use super::garment_display::GarmentStyle;
        let display = &mut self.prefs.garment_display;
        if let Some(style) = args.get("style").and_then(Value::as_str) {
            display.style = match style {
                "textured_thick" => GarmentStyle::TexturedThick,
                "textured" => GarmentStyle::Textured,
                "plain" => GarmentStyle::Plain,
                "translucent" => GarmentStyle::Translucent,
                other => return Err(format!("No style '{other}'")),
            };
        }
        if let Some(on) = args.get("high_quality").and_then(Value::as_bool) {
            display.high_quality = on;
        }
        let view = json!({
            "style": format!("{:?}", display.style).to_lowercase(),
            "high_quality": display.high_quality,
        });
        let render = self.authoring.render.agent_settings(args)?;
        Ok(ToolResult::json(&json!({"render": render, "view": view})))
    }

    fn agent_avatar_maker(&mut self, args: &Value) -> Result<ToolResult, String> {
        use fashioncraft::avatar_look::{self, LookChoice};
        use fashioncraft::avatar_maker::MEASURES;
        let (current, current_look) = self.authoring.avatar_maker.current();
        let body = agent_base::macro_body(args, current)?;
        let mut look: LookChoice = current_look;
        if let Some(l) = args.get("look").and_then(Value::as_object) {
            let choose = |key: &str,
                          options: &[&'static str]|
             -> Result<Option<Option<&'static str>>, String> {
                match l.get(key).and_then(Value::as_str) {
                    None => Ok(None),
                    Some("none") => Ok(Some(None)),
                    Some(name) => options
                        .iter()
                        .find(|o| o.eq_ignore_ascii_case(name))
                        .map(|o| Some(Some(*o)))
                        .ok_or_else(|| {
                            format!("No {key} '{name}'. They are: {}", options.join(", "))
                        }),
                }
            };
            let names = |list: &[(&'static str, &'static str)]| -> Vec<&'static str> {
                list.iter().map(|(n, _)| *n).collect()
            };
            if let Some(v) = choose("skin", &names(&avatar_look::SKINS))? {
                look.skin = v;
            }
            if let Some(v) = choose("eyes", &names(&avatar_look::EYE_COLOURS))? {
                look.eyes = v;
            }
            if let Some(v) = choose("eyebrows", &names(&avatar_look::EYEBROWS))? {
                look.eyebrows = v;
            }
            if let Some(v) = choose("eyelashes", &names(&avatar_look::EYELASHES))? {
                look.eyelashes = v;
            }
            if let Some(v) = choose("hair", &names(&avatar_look::HAIR))? {
                look.hair = v;
            }
            let colours: Vec<&'static str> =
                avatar_look::HAIR_COLOURS.iter().map(|(n, ..)| *n).collect();
            if let Some(v) = choose("hair_colour", &colours)? {
                look.hair_colour = v;
            }
        }
        let settings = json!({
            "gender": body.gender, "age_years": body.age_years, "muscle": body.muscle,
            "weight": body.weight, "height": body.height, "proportions": body.proportions,
            "cupsize": body.cupsize, "firmness": body.firmness,
            "african": body.african, "asian": body.asian, "european": body.european,
            "measures": MEASURES.iter().zip(body.measures).map(|((n, _), v)| ((*n).to_owned(), json!(v))).collect::<serde_json::Map<_, _>>(),
            "look": {"skin": look.skin, "eyes": look.eyes, "eyebrows": look.eyebrows,
                     "eyelashes": look.eyelashes, "hair": look.hair, "hair_colour": look.hair_colour},
        });
        self.authoring.avatar_maker.set(body, look);
        if args.get("make").and_then(Value::as_bool) != Some(true) {
            return Ok(ToolResult::json(&json!({
                "settings": settings,
                "note": "Not made yet: call again with make true to build this body and use it",
                "skins": avatar_look::SKINS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                "hair": avatar_look::HAIR.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            })));
        }
        self.ready()?;
        let obj = super::avatar_library_ui::write_made_avatar(&self.output, &body, &look)?;
        self.install_avatar_file(&obj)?;
        Ok(ToolResult::json(&json!({
            "made": true,
            "settings": settings,
            "note": "The avatar is the new body. The drape was reset: simulate again (pieces \
                     drafted for the old body may need draft_garment again).",
        })))
    }

    fn agent_pose_joints(&mut self, args: &Value) -> Result<ToolResult, String> {
        if self.project.avatar_removed {
            return Err("There is no avatar to pose".into());
        }
        let mut pose = self.current_pose()?;
        let bones = fashioncraft::avatar_rig::built_in_bone_names(&pose.joints);
        let list = args.get("list").and_then(Value::as_bool) == Some(true);
        let joints = args.get("joints").and_then(Value::as_object);
        let reset = args.get("reset").and_then(Value::as_bool) == Some(true);
        if list || (joints.is_none() && !reset) {
            return Ok(ToolResult::json(&json!({
                "bones": bones,
                "turned": pose.pose.0,
                "units": "degrees about world x (the avatar's left), y (up), z (front), at rest",
            })));
        }
        self.ready()?;
        agent_base::change_pose(&mut pose.pose, joints, reset, &bones)?;
        let turned = pose.pose.0.clone();
        if !self.apply_pose(pose) {
            return Err(self.status.clone());
        }
        Ok(ToolResult::json(&json!({
            "turned": turned,
            "note": "Posed. The drape restarts from the arrangement: simulate again.",
        })))
    }

    fn agent_measure_fabric(&mut self, args: &Value) -> Result<Pending, String> {
        use fashioncraft::fabric_kit;
        let m = agent_base::fabric_measurements(args)?;
        let pieces = match args.get("pieces").and_then(Value::as_array) {
            Some(list) => list
                .iter()
                .map(|p| {
                    agent_ops::find_piece(&self.project.template, p.as_str().unwrap_or(""))
                        .map(|p| p.id.clone())
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Measured fabric")
            .to_owned();
        let thread = std::thread::spawn(move || {
            fabric_kit::fit(
                &m,
                fabric_kit::FitOptions::default(),
                &mut |_| {},
                &std::sync::atomic::AtomicBool::new(false),
            )
        });
        Ok(Pending::Base(BasePending::Fit {
            thread,
            name,
            pieces,
        }))
    }

    fn use_measured_fabric(
        &mut self,
        name: &str,
        pieces: &[String],
        report: fashioncraft::fabric_kit::FitReport,
    ) -> Result<ToolResult, String> {
        let every = pieces.is_empty();
        let targets: Vec<String> = if every {
            self.project
                .template
                .panels
                .iter()
                .take(1)
                .map(|p| p.id.clone())
                .collect()
        } else {
            pieces.to_vec()
        };
        for panel in &targets {
            if !self.edit(Command::Fabric {
                panel: panel.clone(),
                name: name.to_owned(),
                properties: report.properties.clone(),
                all_panels: every,
            }) {
                return Err(self.status.clone());
            }
        }
        if let Some(mu) = report.friction {
            self.settings.friction = mu;
        }
        let p = &report.properties;
        Ok(ToolResult::json(&json!({
            "fabric": name,
            "given_to": if every { json!("every piece") } else { json!(targets) },
            "weight_gsm": (p.density * 1000.).round(),
            "thickness_mm": (p.thickness * 10000.).round() / 10.,
            "warp_n_per_m": p.warp.round(), "weft_n_per_m": p.weft.round(),
            "shear_n_per_m": p.shear.round(),
            "bending_warp_n_m": p.bend_warp, "bending_weft_n_m": p.bend_weft,
            "friction": report.friction,
            "checks": report.checks.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
            "notes": report.notes.iter().map(|n| n.english()).collect::<Vec<_>>(),
            "virtual_tests": report.solves,
            "undoable": true,
        })))
    }

    fn agent_points_of_measure(&mut self, args: &Value) -> Result<ToolResult, String> {
        use super::grading_ui::edit;
        use fashioncraft::garment::grading::pom;
        let list = |template: &fashioncraft::garment::GarmentTemplate| {
            pom::table(template)
                .into_iter()
                .map(|r| {
                    json!({
                        "id": r.id, "name": r.name, "tolerance_cm": cm(r.tolerance_m),
                        "base_cm": r.base.as_ref().ok().map(|v| cm(*v)),
                        "sizes_cm": r.values.iter().map(|(s, v)| json!({"size": s, "cm": v.as_ref().ok().map(|v| cm(*v))})).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>()
        };
        let action = args.get("action").and_then(Value::as_str).unwrap_or("list");
        if action == "list" {
            return Ok(ToolResult::json(
                &json!({"points_of_measure": list(&self.project.template)}),
            ));
        }
        let template = &self.project.template;
        if template.grading.is_none() {
            return Err("The garment has no sizes yet: grading create first".into());
        }
        let tolerance = number(args, "tolerance_cm").map(|t| t / 100.);
        let change = match action {
            "add" => {
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("'name' is missing")?
                    .to_owned();
                let kind = if let Some(segments) = args.get("segments").and_then(Value::as_array) {
                    let pairs: Vec<(String, String)> = segments
                        .iter()
                        .map(|s| {
                            let p = agent_ops::find_piece(
                                template,
                                s.get("piece").and_then(Value::as_str).unwrap_or(""),
                            )?;
                            Ok((
                                p.id.clone(),
                                s.get("segment")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_owned(),
                            ))
                        })
                        .collect::<Result<_, String>>()?;
                    let refs: Vec<(&str, &str)> = pairs
                        .iter()
                        .map(|(a, b)| (a.as_str(), b.as_str()))
                        .collect();
                    pom::length(&refs)
                } else {
                    let panel = agent_ops::find_piece(
                        template,
                        args.get("piece").and_then(Value::as_str).unwrap_or(""),
                    )?;
                    let edges = &panel
                        .polygon
                        .as_ref()
                        .ok_or("That piece has no outline points")?
                        .edges;
                    let point = |key: &str| -> Result<String, String> {
                        let k = args
                            .get(key)
                            .and_then(Value::as_u64)
                            .ok_or_else(|| format!("'{key}' is missing"))?
                            as usize;
                        edges
                            .get(k)
                            .cloned()
                            .ok_or_else(|| format!("{key} {k} is not a point of {}", panel.id))
                    };
                    pom::distance(&panel.id, &point("from_point")?, &point("to_point")?)
                };
                let tol = tolerance.unwrap_or(0.005);
                edit("Add measurement", move |g, _| {
                    g.add_measurement(&name, kind, tol).map(|_| ())
                })
            }
            "remove" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("'id' is missing")?
                    .to_owned();
                edit("Remove measurement", move |g, _| g.remove_measurement(&id))
            }
            "set_tolerance" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("'id' is missing")?
                    .to_owned();
                let tol = tolerance.ok_or("'tolerance_cm' is missing")?;
                edit("Measurement tolerance", move |g, _| {
                    g.measurement_mut(&id)
                        .ok_or_else(|| format!("No point of measure '{id}'"))?
                        .tolerance_m = tol;
                    Ok(())
                })
            }
            other => {
                return Err(format!(
                    "action '{other}': add, remove, set_tolerance or list"
                ));
            }
        };
        let before = serde_json::to_string(&self.project.template.grading).unwrap_or_default();
        self.apply_grading_change(change);
        if serde_json::to_string(&self.project.template.grading).unwrap_or_default() == before {
            return Err(format!("Nothing changed: {}", self.status));
        }
        Ok(ToolResult::json(
            &json!({"done": self.status, "points_of_measure": list(&self.project.template)}),
        ))
    }

    fn agent_measure(&mut self, args: &Value) -> Result<ToolResult, String> {
        use fashioncraft::measure3d;
        match args.get("what").and_then(Value::as_str) {
            Some("points") => {
                let panel = agent_ops::find_piece(
                    &self.project.template,
                    args.get("piece").and_then(Value::as_str).unwrap_or(""),
                )?;
                let points = &panel
                    .polygon
                    .as_ref()
                    .ok_or("That piece has no outline points")?
                    .points;
                let at = |key: &str| -> Result<DVec2, String> {
                    let k = args
                        .get(key)
                        .and_then(Value::as_u64)
                        .ok_or_else(|| format!("'{key}' is missing"))?
                        as usize;
                    points
                        .get(k)
                        .copied()
                        .ok_or_else(|| format!("{key} {k} is not a point of {}", panel.id))
                };
                let (a, b) = (at("from_point")?, at("to_point")?);
                Ok(ToolResult::json(&json!({
                    "piece": panel.id, "straight_cm": cm(a.distance(b)),
                    "dx_cm": cm(b.x - a.x), "dy_cm": cm(b.y - a.y),
                })))
            }
            Some("girth") => {
                let h = number(args, "height_cm").ok_or("'height_cm' is missing")? / 100.;
                let surface = &self.scene.collider.surface;
                let floor = surface
                    .vertices
                    .iter()
                    .map(|v| v.y)
                    .fold(f64::INFINITY, f64::min);
                let floor = if floor.is_finite() {
                    floor
                } else {
                    self.scene
                        .state
                        .x
                        .iter()
                        .map(|v| v.y)
                        .fold(f64::INFINITY, f64::min)
                };
                let mut axis = self.body_axis();
                axis.y = floor + h;
                let body = (!self.project.avatar_removed && !surface.vertices.is_empty())
                    .then(|| measure3d::girth(&surface.vertices, &surface.triangles, axis))
                    .flatten()
                    .map(|c| measure3d::path_length(&c.points, c.closed));
                let triangles: Vec<[usize; 3]> = self
                    .scene
                    .garment
                    .mesh
                    .triangles
                    .iter()
                    .map(|t| t.vertices)
                    .collect();
                // Pieces whose cut lies within the body's widest reach there (and a hand more)
                // count; sleeves on arms further out do not.
                let reach = (!self.project.avatar_removed && !surface.vertices.is_empty())
                    .then(|| measure3d::girth(&surface.vertices, &surface.triangles, axis))
                    .flatten()
                    .map(|c| {
                        c.points
                            .iter()
                            .map(|p| DVec2::new(p.x - axis.x, p.z - axis.z).length())
                            .fold(0f64, f64::max)
                    })
                    .map_or(0.4, |r| r * 1.6 + 0.05);
                let cloth =
                    agent_check::garment_girth(&self.scene.state.x, &triangles, axis, reach);
                Ok(ToolResult::json(&json!({
                    "height_cm": cm(h),
                    "body_cm": body.map(cm),
                    "garment_cm": cloth.map(cm),
                    "ease_cm": body.zip(cloth).map(|(b, g)| cm(g - b)),
                    "note": "Measured on the drape as it is now; simulate first for a fit reading",
                })))
            }
            _ => Err("what: girth or points".into()),
        }
    }

    /// Pressure the cloth puts on the body, by triangle, as the pressure map shows it.
    fn pressure_by_triangle(&self) -> Option<Vec<f64>> {
        use fashioncraft::contact::Collider;
        if self.project.avatar_removed || self.scene.collider.surface.vertices.is_empty() {
            return None;
        }
        let garment = &self.scene.garment;
        let x = &self.scene.state.x;
        if x.len() != garment.mesh.uv.len() {
            return None;
        }
        let mut normals = vec![DVec3::ZERO; x.len()];
        for t in &garment.mesh.triangles {
            let [a, b, c] = t.vertices;
            let n = (x[b] - x[a]).cross(x[c] - x[a]);
            for v in t.vertices {
                normals[v] += n;
            }
        }
        for n in &mut normals {
            *n = n.normalize_or_zero();
        }
        let tolerance = self.settings.contact.radius + 0.002;
        let collider = &*self.scene.collider;
        Some(
            garment
                .mesh
                .triangles
                .iter()
                .map(|t| {
                    let m = &garment.materials[t.material];
                    let touches = t
                        .vertices
                        .iter()
                        .any(|v| collider.penetration(x[*v], m.thickness + 2. * tolerance) > 0.);
                    if touches {
                        crate::render::membrane_pressure(x, t, m, t.vertices.map(|v| normals[v]))
                    } else {
                        0.
                    }
                })
                .collect(),
        )
    }

    pub(super) fn garment_check(&mut self) -> Value {
        let pressure = self.pressure_by_triangle();
        let body = (!self.project.avatar_removed).then_some(&*self.scene.collider);
        let mut report = agent_check::check(&agent_check::Input {
            template: &self.project.template,
            live: self.live(),
            v: &self.scene.state.v,
            body,
            pressure,
        });
        if let Some(e) = &self.simulation_error {
            if let Some(p) = report["problems"].as_array_mut() {
                p.insert(0, json!(format!("Simulation: {e}")));
            }
            report["ok"] = json!(false);
        }
        report
    }
}
