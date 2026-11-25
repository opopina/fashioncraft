//! Garments and poses as files of their own: File › Save as › Garment / Pose, File › Add ›
//! Garment and File › Open › Pose. A garment file is the pattern template the template browser
//! opens; a pose file holds the joint rotations only, so it fits any avatar with those bones.
use super::*;
use fashioncraft::{
    authoring::{
        Command,
        clipboard::{self, PatternClipboard},
    },
    avatar_rig::Pose,
    garment::GarmentTemplate,
};
use glam::DVec2;
use rfd::FileDialog;
use std::{io::Read, path::Path};

/// Largest garment or project file read, as in the template browser.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// The garment in a saved garment, or the garment of a saved project.
fn read_garment(path: &Path) -> Result<GarmentTemplate, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("{} is larger than 8 MiB", path.display()));
    }
    if let Ok(template) = serde_json::from_slice::<GarmentTemplate>(&bytes) {
        return Ok(template);
    }
    serde_json::from_slice::<FittingProject>(&bytes)
        .map(|project| project.template)
        .map_err(|_| format!("{} is neither a garment nor a project", path.display()))
}

impl Atelier {
    pub(super) fn save_garment(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .set_title("Save the garment")
            .add_filter("FashionCraft template JSON", &["json"])
            .set_file_name("garment.json")
            .save_file()
        else {
            return Ok(());
        };
        let text =
            serde_json::to_string_pretty(&self.project.template).map_err(|e| e.to_string())?;
        fashioncraft::document::atomic_write(&path, text.as_bytes())?;
        self.status = format!(
            "Garment saved: {} in {} · open it from the garment templates or add it to another project",
            super::garment_display::pieces(self.project.template.panels.len()),
            path.display()
        );
        Ok(())
    }

    pub(super) fn add_garment(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .set_title("Add a garment")
            .add_filter("FashionCraft garment or project", &["json"])
            .pick_file()
        else {
            return Ok(());
        };
        let template = read_garment(&path)?;
        self.add_garment_template(template)
    }

    /// The pieces, seams, trims and graphics of `incoming`, added to the right of the pieces
    /// already on the board. Pins and frozen holds stay behind: they belong to its own drape.
    pub(super) fn add_garment_template(
        &mut self,
        mut incoming: GarmentTemplate,
    ) -> Result<(), String> {
        let held = !incoming.pin_targets.is_empty() || !incoming.frozen_panels.is_empty();
        incoming.pin_targets.clear();
        incoming.frozen_panels.clear();
        let ids: Vec<String> = incoming.panels.iter().map(|p| p.id.clone()).collect();
        let (lo, hi) =
            clipboard::layout_bounds(&incoming, &ids).ok_or("That garment has no pieces")?;
        let clip = PatternClipboard::capture(&incoming, &ids)?;
        let current = &self.project.template;
        let here: Vec<String> = current.panels.iter().map(|p| p.id.clone()).collect();
        let corner = clipboard::layout_bounds(current, &here)
            .map_or(DVec2::ZERO, |(l, h)| DVec2::new(h.x + 0.1, l.y));
        let before = current.panels.len();
        self.playing = false;
        if !self.edit(Command::Paste {
            clipboard: clip,
            center: corner + (hi - lo) * 0.5,
        }) {
            return Err(self.status.clone());
        }
        self.authoring.selected = None;
        self.authoring.selection.panels.clear();
        self.authoring.edge = None;
        self.status = format!(
            "Garment added: {} with their seams, to the right on the board{} · arrange them in 3D before simulating",
            super::garment_display::pieces(self.project.template.panels.len() - before),
            if held { " (its pins stay behind)" } else { "" }
        );
        Ok(())
    }

    /// A drafted garment worn over the one on the avatar, which keeps its pieces and drape
    /// (see [`fashioncraft::agent_build::wear_over`]): each try is checked as the edit would
    /// start, the drape kept, against the body. One undo step.
    pub(super) fn wear_over(
        &mut self,
        incoming: GarmentTemplate,
        garment: &str,
    ) -> Result<String, String> {
        let body = std::sync::Arc::clone(&self.scene.collider.surface);
        let axis = if body.vertices.is_empty() {
            glam::DVec3::ZERO
        } else {
            body.vertices.iter().copied().sum::<glam::DVec3>() / body.vertices.len() as f64
        };
        let (draped, state) = (self.scene.garment.clone(), self.scene.state.clone());
        let (build, _) = fashioncraft::agent_build::wear_over(
            &self.project.template,
            &incoming,
            garment,
            axis,
            &mut |edit| {
                let kept = edit.state_keeping_drape(&draped, &state);
                fashioncraft::vbd::VbdSolver::new(
                    std::sync::Arc::new(edit.garment.clone()),
                    Some(&body),
                    &kept,
                )
                .is_ok()
            },
        )?;
        self.playing = false;
        if !self.edit_sequence(build.commands, &build.names, build.history_name) {
            return Err(self.status.clone());
        }
        self.authoring.selected = None;
        self.authoring.selection.panels.clear();
        Ok(build.done)
    }

    /// The pose on show: the one being edited in the pose window, or the avatar's own.
    fn pose_on_show(&self) -> Pose {
        self.authoring
            .pose
            .draft
            .as_ref()
            .or(self.project.avatar_pose.as_ref())
            .map(|p| p.pose.clone())
            .unwrap_or_default()
    }

    /// File › Save as › Avatar: the avatar as it stands now (shaped and posed), as an OBJ mesh
    /// the avatar importer reads back.
    pub(super) fn save_avatar(&mut self) -> Result<(), String> {
        let body = std::sync::Arc::clone(&self.scene.collider.surface);
        if body.triangles.is_empty() {
            return Err("There is no avatar to save".into());
        }
        let Some(path) = FileDialog::new()
            .set_title("Save the avatar")
            .add_filter("OBJ mesh", &["obj"])
            .set_file_name("avatar.obj")
            .save_file()
        else {
            return Ok(());
        };
        let text = body.to_obj("FashionCraft avatar, posed, metres, y up");
        fashioncraft::document::atomic_write(&path, text.as_bytes())?;
        self.status = format!(
            "Avatar saved: {} vertices, {} triangles in {}",
            body.vertices.len(),
            body.triangles.len(),
            path.display()
        );
        Ok(())
    }

    pub(super) fn save_pose_file(&mut self) -> Result<(), String> {
        let pose = self.pose_on_show();
        let Some(path) = FileDialog::new()
            .set_title("Save the pose")
            .add_filter("FashionCraft pose", &["json"])
            .set_file_name("pose.json")
            .save_file()
        else {
            return Ok(());
        };
        fashioncraft::document::atomic_write(&path, pose.to_file()?.as_bytes())?;
        self.status = format!(
            "Pose saved: {} rotated bones in {}",
            pose.0.len(),
            path.display()
        );
        Ok(())
    }

    pub(super) fn load_pose_file(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .set_title("Open a pose")
            .add_filter("FashionCraft pose", &["json"])
            .pick_file()
        else {
            return Ok(());
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.put_on_pose(Pose::from_file(&text)?)
    }

    /// Put the avatar in `pose`, keeping its own joints, added bones and painted weights.
    pub(super) fn put_on_pose(&mut self, pose: Pose) -> Result<(), String> {
        let mut avatar = self.current_pose()?;
        avatar.pose = pose;
        if !self.apply_pose(avatar) {
            return Err(self.status.clone());
        }
        self.authoring.pose.draft = None;
        Ok(())
    }
}
