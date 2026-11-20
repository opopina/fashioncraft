//! Re-drape 3D arrangement (3D Garment) and Auto fit (2D Pattern): after opening another
//! avatar, carry the drape the garment had on the previous one onto it. Re-drape keeps the
//! garment's size; Auto fit then resizes each piece so the cloth stretches on the new body as
//! it did on the old. The previous avatar and drape are remembered from the moment the new
//! avatar replaced them, for this session.
use super::*;
use fashioncraft::{authoring::Command, avatar::SurfaceMesh, redrape};
use glam::DVec3;

/// The avatar a garment was draped on before the last one opened, and that drape.
pub(super) struct RedrapeSource {
    body: SurfaceMesh,
    landmarks: Option<redrape::Landmarks>,
    x: Vec<DVec3>,
}

impl Atelier {
    /// The current avatar and drape, to remember when another avatar replaces it.
    pub(super) fn redrape_source(&self) -> Option<RedrapeSource> {
        let body = self.project.body_surface().ok()??;
        Some(RedrapeSource {
            body,
            landmarks: self.project.body_landmarks(),
            x: self.scene.state.x.clone(),
        })
    }

    /// The remembered drape carried onto the current avatar, and the drape it came from.
    fn carried_drape(&self) -> Result<(Vec<DVec3>, Vec<DVec3>), String> {
        let ui_lang = self.prefs.language;
        if self.pending {
            return Err("Wait for the current step to finish, then try again".into());
        }
        let source = self.authoring.redrape_from.as_ref().ok_or(i18n::tr(
            ui_lang,
            "Open the new avatar first (File › Open › Avatar): the garment is carried over from the previous one",
        ))?;
        if source.x.len() != self.scene.state.x.len() {
            return Err(i18n::tr(
                ui_lang,
                "The garment changed since the avatar was replaced, so its earlier drape no longer fits it",
            )
            .into());
        }
        let body = self.project.body_surface()?.ok_or(i18n::tr(
            ui_lang,
            "The avatar was deleted; open one to drape the garment on",
        ))?;
        let carried = redrape::carry(
            &source.body,
            source.landmarks.as_ref(),
            &body,
            self.project.body_landmarks().as_ref(),
            &source.x,
        )?;
        Ok((source.x.clone(), carried))
    }

    /// Put the cloth at `x`, out of the body, at rest; refused when it would start tangled.
    fn install_drape(&mut self, x: Vec<DVec3>) -> Result<(), String> {
        let garment = self.scene.garment.clone();
        let margin = self.settings.contact.radius + 0.001;
        let mut candidate = self.scene.state.clone();
        for (v, p) in x.into_iter().enumerate() {
            // Held points stay where they are held.
            if garment.mesh.pins[v].is_some() {
                continue;
            }
            candidate.x[v] = self.scene.collider.lift_outside(p, margin);
            candidate.v[v] = DVec3::ZERO;
        }
        fashioncraft::vbd::VbdSolver::new(garment, Some(&self.scene.collider.surface), &candidate)
            .map_err(|e| format!("Not re-draped: the cloth would start intersecting ({e})"))?;
        self.playing = false;
        self.scene.state = candidate;
        self.issue.invalidate();
        self.generation += 1;
        self.metrics = None;
        self.grab = None;
        Ok(())
    }

    /// 3D Garment › Re-drape 3D arrangement.
    pub(super) fn redrape(&mut self) -> Result<(), String> {
        let (_, carried) = self.carried_drape()?;
        self.install_drape(carried)?;
        self.status = i18n::tr(
            self.prefs.language,
            "Re-draped from the previous avatar, same size · simulate to settle",
        )
        .into();
        Ok(())
    }

    /// 2D Pattern › Auto fit.
    pub(super) fn auto_fit(&mut self) -> Result<(), String> {
        let ui_lang = self.prefs.language;
        let (x, carried) = self.carried_drape()?;
        let scales = redrape::fit_scales(&self.scene.garment, &x, &carried);
        self.install_drape(carried)?;
        if scales.is_empty() {
            self.status = i18n::tr(
                ui_lang,
                "Re-draped: the pieces already fit the new avatar · simulate to settle",
            )
            .into();
            return Ok(());
        }
        let count = scales.len();
        if !self.edit(Command::FitPieces { scales }) {
            return Err(self.status.clone());
        }
        self.status = format!(
            "{} {count} · {}",
            i18n::tr(ui_lang, "Pieces resized for the new avatar:"),
            i18n::tr(ui_lang, "simulate to settle")
        );
        Ok(())
    }
}
