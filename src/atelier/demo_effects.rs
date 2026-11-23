//! A check of three cloth behaviours on the T-shirt FashionCraft opens with, through the
//! interface: frayed cut edges, a seam that breaks when the cloth is pulled too hard (and is
//! mended by undo), and a selected zone subdivided while the rest keeps its triangles.
//!
//!     FASHIONCRAFT_DEMO=effects fashioncraft
use super::demo::{Step, Target, act, caption, click, until};
use super::*;
use crate::render::PresetView;
use fashioncraft::garment::seam::{BoundaryRef, Side};
use glam::DVec2;
use std::{cell::Cell, rc::Rc};

/// Seconds each result stays on screen for a look (and a screenshot).
const LOOK: f64 = 5.;
/// The seam the check pulls apart.
const SEAM: &str = "side-right";

fn edge(panel: &str, name: &str) -> BoundaryRef {
    BoundaryRef {
        reversed: false,
        panel: panel.into(),
        side: Side::Top,
        edge: Some(name.into()),
        range: [0., 1.],
    }
}

/// Where material point `uv` of `panel` is now, metres.
fn cloth_at(a: &Atelier, panel: &str, uv: [f64; 2]) -> Option<glam::DVec3> {
    let terms = a
        .scene
        .garment
        .material_point(panel, DVec2::from_array(uv))?;
    Some(terms.iter().map(|(v, w)| a.scene.state.x[*v] * *w).sum())
}

/// That point of the cloth on screen, moved by `offset` points.
fn cloth(panel: &'static str, uv: [f64; 2], offset: [f32; 2]) -> Target {
    Target::Find(Box::new(move |a: &Atelier| {
        let rect = a.authoring.selection.garment_rect?;
        let p = a.camera.project(cloth_at(a, panel, uv)?.as_vec3(), rect)?;
        Some(p + egui::vec2(offset[0], offset[1]))
    }))
}

/// Turn the 3D camera to face the front and frame `radius` metres round a point of the cloth.
fn look(what: &'static str, panel: &'static str, uv: [f64; 2], radius: f32) -> Step {
    look_from(what, PresetView::Front, panel, uv, radius)
}

/// The same, looking along `view`.
fn look_from(
    what: &'static str,
    view: PresetView,
    panel: &'static str,
    uv: [f64; 2],
    radius: f32,
) -> Step {
    act(what, move |a, _| {
        let centre = cloth_at(a, panel, uv).ok_or("that point is not on the cloth")?;
        a.camera.apply_preset(view);
        a.camera.request_frame(centre.as_vec3(), radius);
        Ok(())
    })
}

/// Where to drag the front's point `uv` to pull it away from the back's same point (across
/// the side seam), `distance` points on screen.
fn away_from_back(uv: [f64; 2], distance: f32) -> Target {
    Target::Find(Box::new(move |a: &Atelier| {
        let rect = a.authoring.selection.garment_rect?;
        let front = a
            .camera
            .project(cloth_at(a, "front", uv)?.as_vec3(), rect)?;
        let back = a.camera.project(cloth_at(a, "back", uv)?.as_vec3(), rect)?;
        let direction = (front - back).normalized();
        Some(front + direction * distance)
    }))
}

/// The most the seam's sewn stitches (those closed up; one still closing pulls the pieces
/// together, it carries no load) pull now, N per metre of seam.
fn pull_now(a: &Atelier) -> Option<f64> {
    use fashioncraft::garment::seam_break::{SEWN_GAP_M, stitch_pulls};
    let seam = a.scene.garment.seams.iter().find(|s| s.id == SEAM)?;
    stitch_pulls(seam, &a.scene.state.x)
        .iter()
        .filter(|(_, _, gap)| *gap <= SEWN_GAP_M)
        .map(|(_, pull, _)| *pull)
        .reduce(f64::max)
}

fn frays(a: &Atelier) -> usize {
    a.project
        .template
        .trims
        .iter()
        .filter(|t| matches!(t.kind, fashioncraft::garment::trims::TrimKind::Fray { .. }))
        .count()
}

pub(super) fn effects() -> Vec<Step> {
    let started = Rc::new(Cell::new(0.));
    let since = started.clone();
    let mut s = vec![
        act("start from the default workspace, in English", |a, ctx| {
            a.prefs.workspace = Default::default();
            a.prefs.garment_display = Default::default();
            a.clean_view = false;
            a.prefs.language = i18n::Language::English;
            a.dispatch_command(commands::CommandId::SplitView, ctx)
        }),
        Step::Wait(1.0),
        caption("Drape the T-shirt"),
        click("toolbar.simulate"),
        until(
            "the T-shirt has draped for 2 s",
            |a| a.scene.state.time > 2.,
            180.,
        ),
        click("toolbar.simulate"),
        until("the simulation is paused", |a| !a.playing, 5.),
    ];
    // Fraying: the hem of both pieces and a cuff.
    s.push(caption(
        "Fray the hem: select it, then Materials › Fray edge",
    ));
    for (panel, name) in [("front", "hem"), ("back", "hem"), ("front", "cuff-right")] {
        let count = Rc::new(Cell::new(0));
        let before = count.clone();
        s.push(act("select a cut edge on the board", move |a, _| {
            before.set(frays(a));
            a.authoring.selection.seam = None;
            a.authoring.edge = Some(edge(panel, name));
            Ok(())
        }));
        s.push(click("menu:Materials"));
        s.push(click("cmd:trims.fray"));
        s.push(until(
            "the edge is frayed",
            move |a| frays(a) > count.get(),
            5.,
        ));
    }
    s.extend([
        look("look at the hem", "front", [0.6, 0.64], 0.07),
        caption("Frayed hem: loose threads along the cut edge"),
        Step::Wait(LOOK),
        // A seam that breaks.
        caption("A seam that breaks: Breaks at, in the seam's inspector"),
        act(
            "select the right side seam's edge: its settings show in the inspector",
            |a, _| {
                a.authoring.selected = None;
                a.authoring.edge = Some(edge("front", SEAM));
                a.authoring.selection.seam = Some(SEAM.into());
                a.prefs.workspace.inspector_open = true;
                Ok(())
            },
        ),
        Step::Wait(1.0),
        Step::Scroll(Target::widget("inspector.body"), -400.),
        Step::Scroll(Target::widget("inspector.body"), -400.),
        Step::Scroll(Target::widget("inspector.body"), -400.),
        Step::Wait(0.5),
        click("seam.breaks"),
        until(
            "the seam has a breaking strength",
            |a| a.project.template.seam_strengths.contains_key(SEAM),
            5.,
        ),
        Step::Wait(1.5),
        act(
            "make it hold three times what it carries hanging, no more",
            |a, _| {
                let now = pull_now(a).ok_or("the seam is not sewn")?;
                if a.edit(fashioncraft::authoring::Command::SetSeamStrength {
                    seam: SEAM.into(),
                    strength: Some((3. * now).max(5.)),
                }) {
                    Ok(())
                } else {
                    Err(a.status.clone())
                }
            },
        ),
        look_from(
            "look at the side seam from the side",
            PresetView::Right,
            "front",
            [0.75, 0.45],
            0.3,
        ),
        Step::Wait(1.0),
        caption("Pull the cloth hard: the seam gives way"),
        click("menu:3D Garment"),
        click("cmd:tool.grab"),
        act(
            "turn the grab's strength up (its Strength slider)",
            |a, _| {
                a.grab_strength = 20.;
                Ok(())
            },
        ),
        click("toolbar.simulate"),
        Step::Wait(1.5),
        until(
            "the seam holds while the T-shirt just hangs",
            |a| !a.project.template.seam_tears.contains_key(SEAM),
            0.1,
        ),
        Step::DragHold(
            cloth("front", [0.68, 0.45], [0., 0.]),
            away_from_back([0.68, 0.45], 200.),
            3.,
        ),
        until(
            "stitches of the seam broke",
            |a| a.project.template.seam_tears.contains_key(SEAM),
            8.,
        ),
        Step::Wait(LOOK),
        click("toolbar.simulate"),
        until("the simulation is paused", |a| !a.playing, 5.),
        caption("Undo mends it: Edit › Undo"),
        click("menu:Edit"),
        click("cmd:edit.undo"),
        until(
            "the seam is whole again",
            |a| !a.project.template.seam_tears.contains_key(SEAM),
            5.,
        ),
        Step::Wait(LOOK / 2.),
        // Subdividing a zone.
        caption("Subdivide a zone: Select mesh (box), then Subdivide"),
        act("show the triangles", |a, _| {
            a.wire = true;
            Ok(())
        }),
        look("look at the chest", "front", [0.5, 0.3], 0.3),
        Step::Wait(1.0),
        click("menu:3D Garment"),
        click("cmd:garment.select_mesh_box"),
        Step::Drag(
            cloth("front", [0.42, 0.2], [0., 0.]),
            cloth("front", [0.58, 0.36], [0., 0.]),
        ),
        until(
            "cloth is selected",
            |a| !a.authoring.mesh_select.vertices.is_empty(),
            5.,
        ),
        Step::Wait(1.0),
        click("mesh_select.subdivide"),
        until(
            "the zone is subdivided",
            |a| {
                a.project.template.panels.iter().any(|p| {
                    p.polygon
                        .as_ref()
                        .is_some_and(|q| !q.meshing_settings().refine.is_empty())
                })
            },
            10.,
        ),
        caption("Finer triangles where it was selected; the rest as it was"),
        Step::Wait(LOOK),
        caption("It still simulates"),
        act("note the time", move |a, _| {
            started.set(a.scene.state.time);
            Ok(())
        }),
        click("toolbar.simulate"),
        until(
            "the subdivided T-shirt has simulated 1 s more",
            move |a| a.scene.state.time > since.get() + 1.,
            120.,
        ),
        click("toolbar.simulate"),
        Step::Wait(LOOK),
        caption(""),
    ]);
    s
}
