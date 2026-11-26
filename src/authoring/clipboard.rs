//! Versioned pattern clipboard. Only editable data, never draped vertices.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const FORMAT: &str = "clothcraft-pattern-clipboard";
const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternClipboard {
    format: String,
    version: u32,
    // Boxed: a template is large and travels inside `Command::Paste`.
    template: Box<GarmentTemplate>,
}

impl PatternClipboard {
    pub fn capture(source: &GarmentTemplate, ids: &[String]) -> Result<Self, String> {
        let ids: BTreeSet<_> = ids.iter().collect();
        if ids.is_empty() || ids.len() > 64 {
            return Err("Select between 1 and 64 panels to copy".into());
        }
        let mut template = source.clone();
        // An archive never travels inside copied or archived pieces.
        template.archive.clear();
        template
            .mirror_links
            .retain(|l| ids.contains(&l.a) && ids.contains(&l.b));
        template.panels = source
            .panels
            .iter()
            .enumerate()
            .filter(|(_, p)| ids.contains(&p.id))
            .map(|(i, p)| {
                let mut p = p.clone();
                p.layout_2d = Some(layout(source, i));
                p
            })
            .collect();
        if template.panels.len() != ids.len() {
            return Err("A selected panel no longer exists".into());
        }
        template
            .seams
            .retain(|s| s.references().all(|r| ids.contains(&r.panel)));
        // Holds belong to selected panels only. Retain selected holds so the
        // existing paste guard can reject unsupported target remapping.
        template
            .pin_targets
            .retain(|hold| ids.contains(&hold.panel));
        template
            .frozen_panels
            .retain(|hold| ids.contains(&hold.panel));
        template
            .graphics
            .retain(|graphic| ids.contains(&graphic.panel_id));
        template.trims.retain(|trim| ids.contains(&trim.panel_id));
        // Per-piece simulation settings of other pieces have no place on the clipboard.
        template
            .deactivated_panels
            .retain(|d| ids.contains(&d.panel));
        template
            .solidified_panels
            .retain(|s| ids.contains(&s.panel));
        template.strengthened_panels.retain(|id| ids.contains(id));
        template.panel_layers.retain(|id, _| ids.contains(id));
        template.panel_pressure.retain(|id, _| ids.contains(id));
        template.panel_shrink.retain(|id, _| ids.contains(id));
        template.flipped_panels.retain(|id| ids.contains(id));
        template.line_springs.retain(|s| ids.contains(&s.panel));
        template.line_folds.retain(|f| ids.contains(&f.panel));
        template
            .tacks
            .retain(|t| ids.contains(&t.a.panel) && ids.contains(&t.b.panel));
        let kept: BTreeSet<String> = template.seams.iter().map(|s| s.id.clone()).collect();
        template.seam_folds.retain(|id, _| kept.contains(id));
        template.seam_zippers.retain(|id, _| kept.contains(id));
        template.panel_wraps.retain(|id, _| ids.contains(id));
        template.locked_panels.retain(|id| ids.contains(id));
        template
            .fabrics
            .retain(|f| template.panels.iter().any(|p| p.fabric == f.id));
        template
            .fabric_appearance
            .retain(|id, _| template.fabrics.iter().any(|f| &f.id == id));
        let center = board_center(&template.panels);
        for p in &mut template.panels {
            p.layout_2d = p.layout_2d.map(|q| q - center);
        }
        Ok(Self {
            format: FORMAT.into(),
            version: 1,
            template: Box::new(template),
        })
    }

    pub fn panel_count(&self) -> usize {
        self.template.panels.len()
    }
    pub fn seam_count(&self) -> usize {
        self.template.seams.len()
    }

    pub fn to_text(&self) -> Result<String, String> {
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        if text.len() > MAX_BYTES {
            return Err("Pattern clipboard exceeds 2 MiB".into());
        }
        Ok(text)
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BYTES {
            return Err("Pattern clipboard exceeds 2 MiB".into());
        }
        let clip: Self = serde_json::from_str(text)
            .map_err(|_| "Clipboard does not contain FashionCraft panels".to_string())?;
        clip.validate()?;
        Ok(clip)
    }

    fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.version != 1 || !(1..=64).contains(&self.panel_count()) {
            return Err("Unsupported or empty pattern clipboard".into());
        }
        if !matches!(self.template.version, 1 | 2) {
            return Err("Unsupported pattern template version".into());
        }
        let panels: BTreeSet<_> = self.template.panels.iter().map(|p| &p.id).collect();
        let fabrics: BTreeSet<_> = self.template.fabrics.iter().map(|f| &f.id).collect();
        let seams: BTreeSet<_> = self.template.seams.iter().map(|s| &s.id).collect();
        if panels.len() != self.panel_count()
            || fabrics.len() != self.template.fabrics.len()
            || seams.len() != self.template.seams.len()
            || self.template.panels.iter().any(|p| {
                !fabrics.contains(&p.fabric) || !p.layout_2d.is_some_and(|q| q.is_finite())
            })
            || self
                .template
                .seams
                .iter()
                .any(|s| s.references().any(|r| !panels.contains(&r.panel)))
            || self
                .template
                .pin_targets
                .iter()
                .any(|p| !panels.contains(&p.panel))
            || self.template.frozen_panels.iter().any(|freeze| {
                !panels.contains(&freeze.panel)
                    || freeze.targets.iter().any(|p| p.panel != freeze.panel)
            })
            || self
                .template
                .graphics
                .iter()
                .any(|g| !panels.contains(&g.panel_id))
            || self
                .template
                .trims
                .iter()
                .any(|t| !panels.contains(&t.panel_id))
        {
            return Err("Clipboard contains missing or ambiguous references".into());
        }
        Ok(())
    }

    pub(super) fn paste(
        &self,
        target: &mut GarmentTemplate,
        center: DVec2,
    ) -> Result<Vec<String>, String> {
        self.insert(target, center, false)
    }

    /// Put archived pieces back as they were: their names, ids when free, board position
    /// around `center` and 3D arrangement, without the offset a paste adds.
    pub(super) fn restore(
        &self,
        target: &mut GarmentTemplate,
        center: DVec2,
    ) -> Result<Vec<String>, String> {
        self.insert(target, center, true)
    }

    fn insert(
        &self,
        target: &mut GarmentTemplate,
        center: DVec2,
        restore: bool,
    ) -> Result<Vec<String>, String> {
        self.validate()?;
        if !center.is_finite() {
            return Err("Invalid paste position".into());
        }
        // Place the copied assembly beside the existing flat arrangements. The
        // same world translation preserves relative placements and seam pairs.
        let max_x = target
            .panels
            .iter()
            .flat_map(|p| corners(p).map(|uv| p.placement.transform(DVec3::new(uv.x, -uv.y, 0.)).x))
            .fold(0., f64::max);
        let min_x = self
            .template
            .panels
            .iter()
            .flat_map(|p| corners(p).map(|uv| p.placement.transform(DVec3::new(uv.x, -uv.y, 0.)).x))
            .fold(f64::INFINITY, f64::min);
        let shift = if restore {
            DVec3::ZERO
        } else {
            DVec3::new(max_x + 0.15 - min_x, 0., 0.)
        };
        let mut fabrics = BTreeMap::new();
        for f in &self.template.fabrics {
            let id = if let Some(existing) = target.fabrics.iter().find(|v| {
                v.properties == f.properties
                    && v.name == f.name
                    && target.fabric_appearance.get(&v.id)
                        == self.template.fabric_appearance.get(&f.id)
            }) {
                existing.id.clone()
            } else {
                let optics = self.template.fabric_appearance.get(&f.id).cloned();
                let mut f = f.clone();
                f.id = unused_id("fabric", target.fabrics.iter().map(|f| f.id.clone()));
                let id = f.id.clone();
                target.fabrics.push(f);
                if let Some(optics) = optics {
                    target.fabric_appearance.insert(id.clone(), optics);
                }
                id
            };
            fabrics.insert(f.id.clone(), id);
        }
        let mut panels = BTreeMap::new();
        let mut created = Vec::new();
        for p in &self.template.panels {
            let mut copy = p.clone();
            let taken = target.panels.iter().any(|q| q.id == p.id);
            if !restore || taken {
                copy.id = unused_id("panel", target.panels.iter().map(|p| p.id.clone()));
            }
            if !restore {
                copy.name = format!("{} copy", p.name);
            }
            copy.fabric = fabrics[&p.fabric].clone();
            copy.layout_2d = Some(p.layout_2d.unwrap() + center);
            copy.placement.translation += shift;
            panels.insert(p.id.clone(), copy.id.clone());
            created.push(copy.id.clone());
            target.panels.push(copy);
        }
        let mut seams = BTreeMap::new();
        for s in &self.template.seams {
            let mut copy = s.clone();
            copy.id = unused_id("seam", target.seams.iter().map(|s| s.id.clone()));
            seams.insert(s.id.clone(), copy.id.clone());
            for r in copy.references_mut() {
                r.panel = panels[&r.panel].clone();
            }
            target.seams.push(copy);
        }
        for graphic in &self.template.graphics {
            let mut copy = graphic.clone();
            copy.id = unused_id("graphic", target.graphics.iter().map(|g| g.id.clone()));
            copy.panel_id = panels[&graphic.panel_id].clone();
            target.graphics.push(copy);
        }
        for trim in &self.template.trims {
            let mut copy = trim.clone();
            copy.id = unused_id("trim", target.trims.iter().map(|t| t.id.clone()));
            copy.panel_id = panels[&trim.panel_id].clone();
            target.trims.push(copy);
        }
        for link in &self.template.mirror_links {
            target
                .mirror_links
                .push(crate::garment::template::MirrorLink {
                    a: panels[&link.a].clone(),
                    b: panels[&link.b].clone(),
                    axis: link.axis,
                });
        }
        self.carry_settings(target, &panels, &seams);
        if target.panels.iter().any(|p| p.polygon.is_some()) {
            target.version = 2;
        }
        Ok(created)
    }

    /// The pieces' and seams' own settings go with them, under their new ids: a sleeve's wrap
    /// around the arm (without it a pasted set-in sleeve starts flat and its armpit corner
    /// never closes), layers, pressure, shrink, strengthening, flipped faces, locks, elastics
    /// and tapes, line folds, tacks, seam folds and zippers. Holds (pins, frozen, deactivated
    /// or solidified pieces) stay behind: they belong to the drape the pieces came from.
    fn carry_settings(
        &self,
        target: &mut GarmentTemplate,
        panels: &BTreeMap<String, String>,
        seams: &BTreeMap<String, String>,
    ) {
        let source = &self.template;
        let panel = |id: &String| panels.get(id).cloned();
        for (id, wrap) in &source.panel_wraps {
            if let Some(new) = panel(id) {
                target.panel_wraps.insert(new, *wrap);
            }
        }
        for (id, layer) in &source.panel_layers {
            if let Some(new) = panel(id) {
                target.panel_layers.insert(new, *layer);
            }
        }
        for (id, pressure) in &source.panel_pressure {
            if let Some(new) = panel(id) {
                target.panel_pressure.insert(new, *pressure);
            }
        }
        for (id, shrink) in &source.panel_shrink {
            if let Some(new) = panel(id) {
                target.panel_shrink.insert(new, *shrink);
            }
        }
        for set in [
            (&source.strengthened_panels, &mut target.strengthened_panels),
            (&source.flipped_panels, &mut target.flipped_panels),
            (&source.locked_panels, &mut target.locked_panels),
        ] {
            set.1.extend(set.0.iter().filter_map(panel));
        }
        for spring in &source.line_springs {
            if let Some(new) = panel(&spring.panel) {
                target
                    .line_springs
                    .push(crate::garment::template::LineSpring {
                        panel: new,
                        ..spring.clone()
                    });
            }
        }
        for fold in &source.line_folds {
            if let Some(new) = panel(&fold.panel) {
                target.line_folds.push(crate::garment::template::LineFold {
                    panel: new,
                    ..fold.clone()
                });
            }
        }
        for tack in &source.tacks {
            if let (Some(a), Some(b)) = (panel(&tack.a.panel), panel(&tack.b.panel)) {
                let mut copy = tack.clone();
                copy.id = unused_id("tack", target.tacks.iter().map(|t| t.id.clone()));
                copy.a.panel = a;
                copy.b.panel = b;
                target.tacks.push(copy);
            }
        }
        for (id, fold) in &source.seam_folds {
            if let Some(new) = seams.get(id) {
                target.seam_folds.insert(new.clone(), fold.clone());
            }
        }
        for (id, zipper) in &source.seam_zippers {
            if let Some(new) = seams.get(id) {
                target.seam_zippers.insert(new.clone(), zipper.clone());
            }
        }
    }
}

fn corners(p: &Panel) -> [DVec2; 4] {
    let (w, h) = (p.pattern.width, p.pattern.height);
    [
        DVec2::ZERO,
        DVec2::new(w, 0.),
        DVec2::new(w, h),
        DVec2::new(0., h),
    ]
}

/// Middle of the board box of `panels` (their laid-out corners).
fn board_center(panels: &[crate::garment::template::Panel]) -> DVec2 {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for p in panels {
        for uv in corners(p) {
            let q = p.layout_2d.unwrap_or(DVec2::ZERO) + rotate_2d(uv, p.layout_rotation_degrees);
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    (lo + hi) * 0.5
}

/// Middle of the board box the pieces `ids` occupy in `template`, with implicit layouts
/// resolved as the board draws them.
/// Lowest and highest corners, on the board, of the pieces `ids` as they are laid out;
/// `None` when there are none.
pub fn layout_bounds(template: &GarmentTemplate, ids: &[String]) -> Option<(DVec2, DVec2)> {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for (i, p) in template.panels.iter().enumerate() {
        if !ids.contains(&p.id) {
            continue;
        }
        let origin = layout(template, i);
        for uv in corners(p) {
            let q = origin + rotate_2d(uv, p.layout_rotation_degrees);
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    lo.is_finite().then_some((lo, hi))
}

pub fn layout_center(template: &GarmentTemplate, ids: &[String]) -> DVec2 {
    let panels: Vec<_> = template
        .panels
        .iter()
        .enumerate()
        .filter(|(_, p)| ids.contains(&p.id))
        .map(|(i, p)| {
            let mut p = p.clone();
            p.layout_2d = Some(layout(template, i));
            p
        })
        .collect();
    board_center(&panels)
}
