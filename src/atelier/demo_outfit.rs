//! The demo for the promo video: a monochrome outfit made from scratch in the new interface, the
//! way a person would make it. Wide trousers drafted from the avatar's measurements, a cropped
//! boxy overshirt drawn piece by piece, sewn and draped, buttons, the fabric, and the Assistant
//! asked (in Spanish) to colour the whole outfit and simulate it again.
//!
//!     FASHIONCRAFT_DEMO=outfit FASHIONCRAFT_DEMO_RECORD=1 fashioncraft
//!
//! The Assistant step talks to a stand-in server on this computer (the demo stops before it
//! when the preferences point anywhere else), so recording costs nothing and sends nothing out.
use super::demo::{Step, Target, act, click, guard, until};
use super::demo_vest::{Origin, anchor, draw, facing, menu, mirrored, piece, placed};
use super::*;
use crate::render::PresetView;
use glam::DVec2;
use std::{cell::Cell, rc::Rc};

/// Back of the overshirt, metres, y down from the shoulder-neck points, centred on x = 0: boxy
/// (straight sides 53 cm across) and cropped at 42 cm, to sit on the trousers' waist.
const BACK: [([f64; 2], Option<[f64; 2]>); 13] = [
    ([-0.08, 0.0], None),
    ([0.0, 0.02], Some([0.05, 0.0])),
    ([0.08, 0.0], None),
    ([0.185, 0.045], None),
    ([0.19, 0.13], None),
    ([0.215, 0.19], Some([0.015, 0.02])),
    ([0.265, 0.22], None),
    ([0.265, 0.42], None),
    ([-0.265, 0.42], None),
    ([-0.265, 0.22], None),
    ([-0.215, 0.19], Some([0.015, -0.02])),
    ([-0.19, 0.13], None),
    ([-0.185, 0.045], None),
];
/// The left front (as worn): a 2 cm button stand at the centre front, a round neck, the side
/// seam matching the back's, a straight hem.
const FRONT: [([f64; 2], Option<[f64; 2]>); 9] = [
    ([0.08, 0.0], None),
    ([0.185, 0.045], None),
    ([0.19, 0.13], None),
    ([0.215, 0.19], Some([0.015, 0.02])),
    ([0.27, 0.22], None),
    ([0.265, 0.42], None),
    ([-0.02, 0.42], None),
    ([-0.02, 0.11], None),
    ([0.035, 0.06], None),
];
/// Button heights on the centre front.
const BUTTONS: [f64; 4] = [0.15, 0.23, 0.31, 0.38];
/// The trousers draft makes four pieces; the overshirt's come after them.
const BACK_PIECE: usize = 4;
const FRONT_PIECE: usize = 5;
const MIRROR_PIECE: usize = 6;

/// What the Assistant is asked, typed into its field.
const ASK: &str = "Deja todo el conjunto en color crudo y vuelve a simular la caída.";

/// Whether the cloth hangs on the 1.80 m avatar: the highest cloth on the torso (above 1.2 m)
/// and its centre between the knees and the chest. Cloth that fell would lie near the floor.
fn on_the_body(a: &Atelier) -> bool {
    let x = &a.scene.state.x;
    if x.is_empty() {
        return false;
    }
    let centre = x.iter().map(|p| p.y).sum::<f64>() / x.len() as f64;
    let highest = x.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    highest > 1.0 && (0.4..1.4).contains(&centre)
}

/// Buttons (or buttonholes) on the garment.
fn trims(a: &Atelier, buttonholes: bool) -> usize {
    super::demo_vest::count_trims(a, buttonholes)
}

/// Type `value` into drafting field `k` of the trousers window.
fn field(steps: &mut Vec<Step>, k: usize, value: &str) {
    steps.extend([
        click(&format!("draft.trousers.{k}")),
        Step::Key(egui::Key::A, egui::Modifiers::COMMAND),
        Step::Type(value.into()),
        Step::Key(egui::Key::Enter, egui::Modifiers::NONE),
        Step::Wait(0.3),
    ]);
}

/// Run a command from the command palette (⇧⌘P), the way the new interface's search box does:
/// type part of its name, Enter runs the first match. Long menus scroll, so their lower items
/// can be out of reach of a click.
fn palette(steps: &mut Vec<Step>, query: &str) {
    steps.extend([
        Step::Key(
            egui::Key::P,
            egui::Modifiers {
                command: true,
                mac_cmd: cfg!(target_os = "macos"),
                shift: true,
                ..Default::default()
            },
        ),
        Step::Wait(0.6),
        Step::Type(query.into()),
        Step::Wait(0.8),
        Step::Key(egui::Key::Enter, egui::Modifiers::NONE),
        Step::Wait(0.8),
    ]);
}

pub(super) fn outfit() -> Vec<Step> {
    let origin: Origin = Rc::new(Cell::new(DVec2::ZERO));
    let back_at = DVec2::new(-0.30, -0.30);
    let front_at = DVec2::new(0.22, -0.30);
    // Seams the trousers come with: the overshirt's are counted after them.
    let base_seams = Rc::new(Cell::new(0usize));
    let entries = Rc::new(Cell::new(0usize));
    let mut s = vec![
        act("start from the default workspace, in Spanish", |a, ctx| {
            a.prefs.workspace = Default::default();
            a.prefs.garment_display = Default::default();
            a.clean_view = false;
            a.prefs.language = i18n::Language::Spanish;
            a.dispatch_command(commands::CommandId::SplitView, ctx)
        }),
        Step::Wait(1.0),
        // The Assistant step must not reach a provider on the internet.
        guard(
            "the Assistant talks only to a server on this computer",
            |a| {
                let url = a.prefs.ai.base_url.trim();
                !url.is_empty() && fashioncraft::assistant::is_local_address(url)
            },
        ),
        Step::Record(true),
        Step::Wait(1.5),
        // A new project and an avatar from the library.
        click("menu:File"),
        click("cmd:file.new"),
        Step::Wait(0.8),
        click("menu:Avatar"),
        click("cmd:avatar.library"),
        Step::Wait(2.5),
        click("library.use.5"),
        Step::Wait(1.0),
        click("avatar.confirm"),
        until(
            "the library avatar is in place",
            |a| fashioncraft::avatar_look::look_file(&a.project.avatar_path).is_some(),
            20.,
        ),
        Step::Wait(1.5),
    ];
    // Wide trousers from the avatar's measurements.
    palette(&mut s, "pantalón por medidas");
    s.extend([
        until(
            "the trousers window is open",
            |a| a.authoring.trousers_draft.open,
            3.,
        ),
        Step::Wait(1.2),
        click("draft.trousers.from_avatar"),
        Step::Wait(1.2),
    ]);
    // Ease 4 cm, length 100 cm, a 62 cm leg opening: wide, ankle length.
    field(&mut s, 2, "4");
    field(&mut s, 5, "100");
    field(&mut s, 6, "62");
    s.extend([
        Step::Wait(0.6),
        click("draft.trousers.create"),
        until(
            "the trousers are drafted",
            |a| a.project.template.panels.len() == 4,
            5.,
        ),
        Step::Wait(1.0),
    ]);
    // The drafting window closes the way it opened.
    palette(&mut s, "pantalón por medidas");
    s.extend([
        Step::Hover(Target::Find(Box::new(|a: &Atelier| {
            a.authoring.selection.board_rect.map(|r| r.center())
        }))),
        Step::Key(egui::Key::Home, egui::Modifiers::NONE),
        Step::Wait(0.8),
        // Cotton on skin grips: the friction of the Simulation tab's "Grippy" (0.6) before the
        // first drape, set directly (the tab's buttons sit below the fold at this size).
        act(
            "friction 0.6 (Simulation › Friction › Grippy)",
            |a, _| {
                a.settings.friction = 0.6;
                Ok(())
            },
        ),
        click("toolbar.simulate"),
        Step::Wait(8.0),
        click("toolbar.simulate"),
        guard("the trousers hang on the body", on_the_body),
        Step::Wait(1.0),
    ]);
    // The overshirt, drawn on the board beside the trousers' pieces.
    palette(&mut s, "solo 2d");
    s.extend([
        Step::Wait(0.8),
        act("frame the board beside the trousers for drawing", {
            let origin = origin.clone();
            move |a, _| {
                let right = (0..a.project.template.panels.len())
                    .map(|i| a.board_origin(i).x + a.project.template.panels[i].pattern.width)
                    .fold(f64::NEG_INFINITY, f64::max);
                let top = (0..a.project.template.panels.len())
                    .map(|i| a.board_origin(i).y)
                    .fold(f64::INFINITY, f64::min);
                a.authoring.board.view_rotation = 0.;
                a.authoring.board.center = DVec2::new(right + 0.55, top + 0.35);
                a.authoring.board.scale = 560.;
                origin.set(a.authoring.board.center);
                Ok(())
            }
        }),
        Step::Wait(0.6),
        click("menu:2D Pattern"),
        Step::Hover(Target::widget("sub:Create")),
        click("cmd:tool.draw"),
    ]);
    draw(&mut s, &origin, back_at, &BACK);
    s.extend([
        until(
            "the back is drawn",
            |a| a.project.template.panels.len() > BACK_PIECE,
            3.,
        ),
        Step::Wait(0.6),
    ]);
    draw(&mut s, &origin, front_at, &FRONT);
    s.extend([
        until(
            "the front is drawn",
            |a| a.project.template.panels.len() > FRONT_PIECE,
            3.,
        ),
        Step::Wait(0.6),
        click("menu:2D Pattern"),
        Step::Hover(Target::widget("sub:Edit")),
        click("cmd:tool.edit"),
        Step::Click(piece(FRONT_PIECE, &FRONT, [0.15, 0.30])),
        guard("the front is selected", |a| {
            a.authoring.selected
                == a.project
                    .template
                    .panels
                    .get(FRONT_PIECE)
                    .map(|p| p.id.clone())
        }),
        Step::Key(egui::Key::Q, egui::Modifiers::NONE),
        click("quick.copy.+X →"),
        until(
            "both fronts",
            |a| a.project.template.panels.len() > MIRROR_PIECE,
            4.,
        ),
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        Step::Wait(0.8),
    ]);
    palette(&mut s, "vista dividida");
    s.extend([
        Step::Wait(1.0),
        Step::Hover(Target::Find(Box::new(|a: &Atelier| {
            a.authoring.selection.board_rect.map(|r| r.center())
        }))),
        Step::Key(egui::Key::Home, egui::Modifiers::NONE),
        Step::Wait(0.6),
    ]);
    palette(&mut s, "mostrar puntos de colocación");
    s.extend([
        Step::Wait(0.8),
        Step::Key(egui::Key::Num8, egui::Modifiers::NONE),
        until(
            "the camera faces the back",
            |a| facing(a, PresetView::Back),
            3.,
        ),
        Step::Wait(1.0),
        Step::Click(piece(BACK_PIECE, &BACK, [0.0, 0.3])),
        Step::Click(anchor("torso-middle-back")),
        Step::Wait(1.0),
        guard("the back sits behind the body", |a| {
            placed(a, BACK_PIECE).is_some_and(|t| t.z < 0.)
        }),
        Step::Key(egui::Key::Num2, egui::Modifiers::NONE),
        until(
            "the camera faces the front",
            |a| facing(a, PresetView::Front),
            3.,
        ),
        Step::Wait(1.0),
        Step::Click(piece(FRONT_PIECE, &FRONT, [0.15, 0.30])),
        Step::Click(anchor("torso-middle-front-left")),
        Step::Wait(1.0),
        Step::Click(mirrored(MIRROR_PIECE, &FRONT, [0.15, 0.30])),
        Step::Click(anchor("torso-middle-front-right")),
        Step::Wait(1.2),
        guard("both fronts sit on the front of the body", |a| {
            [FRONT_PIECE, MIRROR_PIECE]
                .iter()
                .all(|&i| placed(a, i).is_some_and(|t| t.z > -0.05 && t.y < 1.7))
        }),
        act("count the trousers' seams", {
            let base = base_seams.clone();
            move |a, _| {
                base.set(a.project.template.seams.len());
                Ok(())
            }
        }),
    ]);
    // Sew the shoulders and the sides: one edge, then its partner.
    menu(&mut s, "Sewing", None, "tool.sew");
    for (k, (back, front, copy)) in [
        ([0.1325, 0.0225], [0.1325, 0.0225], false),
        ([-0.1325, 0.0225], [0.1325, 0.0225], true),
        ([0.265, 0.32], [0.2675, 0.32], false),
        ([-0.265, 0.32], [0.2675, 0.32], true),
    ]
    .into_iter()
    .enumerate()
    {
        s.push(Step::Click(piece(BACK_PIECE, &BACK, back)));
        s.push(Step::Click(if copy {
            mirrored(MIRROR_PIECE, &FRONT, front)
        } else {
            piece(FRONT_PIECE, &FRONT, front)
        }));
        s.push(click("sew.confirm"));
        let base = base_seams.clone();
        s.push(until(
            &format!("overshirt seam {}", k + 1),
            move |a| a.project.template.seams.len() > base.get() + k,
            4.,
        ));
    }
    s.extend([
        Step::Wait(0.6),
        click("toolbar.simulate"),
        Step::Wait(9.0),
        click("toolbar.simulate"),
        guard("the outfit hangs on the body", on_the_body),
        // One fabric for every piece: light woven cotton.
        click("fabric_tab"),
        click("fabric_presets"),
        click("fabric_preset_cotton"),
        click("fabric_all"),
        click("fabric_apply"),
        Step::Wait(0.8),
    ]);
    // Buttonholes on the left front, buttons on the right front, fastened.
    menu(&mut s, "Materials", Some("Buttons"), "trim.buttonhole");
    for y in BUTTONS {
        s.push(Step::Click(piece(FRONT_PIECE, &FRONT, [0.0, y])));
    }
    s.extend([
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        until("four buttonholes", |a| trims(a, true) >= 4, 2.),
    ]);
    menu(&mut s, "Materials", Some("Buttons"), "trim.button");
    for y in BUTTONS {
        s.push(Step::Click(mirrored(MIRROR_PIECE, &FRONT, [0.0, y])));
    }
    s.extend([
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        until("four buttons", |a| trims(a, false) >= 4, 2.),
    ]);
    menu(&mut s, "Materials", Some("Buttons"), "trim.fasten");
    s.extend([
        click("toolbar.simulate"),
        Step::Wait(4.0),
        click("toolbar.simulate"),
        guard("the outfit still hangs on the body", on_the_body),
        Step::Wait(0.8),
    ]);
    // The Assistant: asked in Spanish, it colours the outfit and simulates it again.
    palette(&mut s, "asistente");
    s.extend([
        Step::Wait(1.2),
        act("note the chat's length", {
            let entries = entries.clone();
            move |a, _| {
                entries.set(a.assistant_entries());
                Ok(())
            }
        }),
        click("assistant.input"),
        Step::Type(ASK.into()),
        Step::Wait(0.8),
        click("assistant.send"),
        until(
            "the Assistant answered",
            {
                let entries = entries.clone();
                move |a| !a.assistant_busy() && a.assistant_entries() > entries.get() + 1
            },
            60.,
        ),
        Step::Wait(3.0),
        guard(
            "the outfit hangs on the body after the Assistant",
            on_the_body,
        ),
    ]);
    palette(&mut s, "asistente");
    // A clean finish: no arrangement points, the high-quality view, the 3D view alone without
    // the inspector, annotations hidden, framed, then all the way round the outfit.
    palette(&mut s, "mostrar puntos de colocación");
    menu(&mut s, "Render", None, "view.high_quality");
    palette(&mut s, "solo 3d");
    palette(&mut s, "inspector de propiedades");
    palette(&mut s, "vista frontal");
    s.extend([
        click("header.3d.settings"),
        click("view.clean"),
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        click("header.3d.fit"),
        Step::Wait(0.8),
        Step::Orbit(360., 12.),
        Step::Wait(1.0),
        Step::Record(false),
    ]);
    s
}
