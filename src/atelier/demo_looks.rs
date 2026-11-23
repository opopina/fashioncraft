//! The "looks" demo: a fabric's look from a texture set exported by a texture tool, then a
//! photoreal render of the garment.
//!
//!     FASHIONCRAFT_DEMO=looks FASHIONCRAFT_DEMO_TEXTURES=<folder> \
//!         [FASHIONCRAFT_DEMO_RENDER=<file.png>] [FASHIONCRAFT_DEMO_HDR=<sky.hdr>] fashioncraft
//!
//! The texture folder's maps (`Lace_BaseColor.png`, `Lace_Normal.png`…) go on the first piece's
//! fabric; the render is saved to `FASHIONCRAFT_DEMO_RENDER` once it has 128 samples per pixel,
//! again (`…_dof.png`) after focusing the lens on the image's centre, and lit by the HDR image
//! (`…_hdr.png`) when one is given. The native file dialogs are the only steps taken without
//! the cursor.
use super::demo::{Step, Target, act, caption, click, guard, until};
use super::*;

/// Samples per pixel the render reaches before the demo saves it.
const SAMPLES: u32 = 128;

/// The inspector, beside the 2D board: where to scroll it.
fn inspector() -> Target {
    Target::Find(Box::new(|a: &Atelier| {
        a.authoring
            .selection
            .board_rect
            .map(|r| egui::pos2(r.right() + 130., r.center().y))
    }))
}

/// Somewhere in the 3D view, `dx` points right of its centre.
fn view3d(dx: f32) -> Target {
    Target::Find(Box::new(move |a: &Atelier| {
        a.authoring
            .selection
            .garment_rect
            .map(|r| r.center() + egui::vec2(dx, -60.))
    }))
}

/// `path` with `suffix` before its extension.
fn beside(path: &str, suffix: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(path);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    p.with_file_name(format!("{stem}_{suffix}.png"))
}

/// Wait for the render to start over and reach its samples again.
fn render_again(s: &mut Vec<Step>, what: &str) {
    s.extend([
        until(
            &format!("the render starts over ({what})"),
            |a| a.render_samples() < SAMPLES,
            5.,
        ),
        until(
            &format!("the render reached its samples ({what})"),
            |a| a.render_samples() >= SAMPLES,
            240.,
        ),
    ]);
}

fn draft_look(a: &Atelier) -> Option<&fashioncraft::garment::appearance::FabricAppearance> {
    a.authoring.fabric_draft.as_ref()?.appearance.as_ref()
}

pub(super) fn looks() -> Vec<Step> {
    let textures = std::env::var("FASHIONCRAFT_DEMO_TEXTURES").ok();
    let output = std::env::var("FASHIONCRAFT_DEMO_RENDER").ok();
    let mut s = vec![
        act("start from the default workspace, in English", |a, _| {
            a.prefs.workspace = Default::default();
            a.prefs.garment_display = Default::default();
            a.clean_view = false;
            a.prefs.language = i18n::Language::English;
            Ok(())
        }),
        guard("a texture folder is given (FASHIONCRAFT_DEMO_TEXTURES)", {
            let textures = textures.clone();
            move |_| {
                textures
                    .as_deref()
                    .is_some_and(|t| std::path::Path::new(t).is_dir())
            }
        }),
        guard("the garment has pieces", |a| {
            !a.project.template.panels.is_empty()
        }),
        Step::Wait(1.0),
        Step::Record(true),
        caption("A fabric's look from a texture set"),
        act("select the first piece", |a, _| {
            let id = a.project.template.panels[0].id.clone();
            a.select_panel(id, false);
            Ok(())
        }),
        Step::Wait(0.6),
        click("fabric_tab"),
        Step::Wait(0.6),
        guard("the fabric has no look of its own yet", |a| {
            a.authoring
                .fabric_draft
                .as_ref()
                .is_some_and(|d| d.appearance.is_none())
        }),
        click("appearance.enabled"),
        until("a look is drafted", |a| draft_look(a).is_some(), 3.),
        Step::Wait(0.5),
        caption("Texture set › Import › From a folder: Substance's exported maps"),
        act("pick the texture folder (a file dialog)", move |a, _| {
            let folder = textures.clone().unwrap_or_default();
            let files = fashioncraft::garment::texture_set::folder_files(folder.as_ref())
                .map_err(|e| e.to_string())?;
            a.texture_set_from_files(&files);
            Ok(())
        }),
        until(
            "the maps are staged",
            |a| {
                draft_look(a)
                    .and_then(|l| l.texture.as_ref())
                    .is_some_and(|t| t.asset_ids().count() > 1)
            },
            3.,
        ),
        Step::Wait(3.0),
        Step::Scroll(inspector(), -700.),
        Step::Wait(3.0),
        click("appearance.apply"),
        until(
            "the fabric wears the texture set",
            |a| {
                a.project.template.fabric_appearance.values().any(|l| {
                    l.texture
                        .as_ref()
                        .is_some_and(|t| t.asset_ids().count() > 1)
                })
            },
            3.,
        ),
        Step::Wait(1.5),
        caption("Simulate"),
        click("toolbar.simulate"),
        Step::Wait(12.0),
        click("toolbar.simulate"),
        Step::Wait(1.0),
        caption("A closer look"),
        Step::Scroll(view3d(0.), 400.),
        Step::Wait(2.5),
        caption("Render › Photoreal render: it refines while you keep working"),
    ];
    super::demo_vest::menu(&mut s, "Render", None, "render.photoreal");
    s.extend([
        until(
            "the render reached its samples",
            |a| a.render_samples() >= SAMPLES,
            240.,
        ),
        act("save the image (a file dialog)", {
            let output = output.clone();
            move |a, _| {
                if let Some(path) = &output {
                    a.save_render(path.into());
                }
                Ok(())
            }
        }),
        Step::Wait(2.0),
        caption("Depth of field: pick the point in focus on the image"),
        click("render.pick_focus"),
        click("render.image"),
    ]);
    render_again(&mut s, "in focus");
    s.push(act("save the focused image (a file dialog)", {
        let output = output.clone();
        move |a, _| {
            if let Some(path) = &output {
                a.save_render(beside(path, "dof"));
            }
            Ok(())
        }
    }));
    if let Ok(hdr) = std::env::var("FASHIONCRAFT_DEMO_HDR") {
        s.extend([
            Step::Wait(2.0),
            caption("Light from an HDR environment"),
            act("pick the HDR image (a file dialog)", move |a, _| {
                a.set_render_environment(hdr.as_ref())
            }),
        ]);
        render_again(&mut s, "HDR");
        s.push(act(
            "save the HDR-lit image (a file dialog)",
            move |a, _| {
                if let Some(path) = &output {
                    a.save_render(beside(path, "hdr"));
                }
                Ok(())
            },
        ));
    }
    s.extend([Step::Wait(2.0), caption(""), Step::Record(false)]);
    s
}
