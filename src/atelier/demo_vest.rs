//! The demo scripts: a vest made from scratch, the way a person would make it.
use super::demo::{Step, Target, act, caption, click, guard, until};
use super::*;
use crate::render::PresetView;
use glam::DVec2;
use std::{cell::Cell, rc::Rc};

/// The script called `name`, if there is one.
/// The demos [`script`] knows.
pub(super) const NAMES: &[&str] = &["vest", "outfit"];

/// Whether the 3D camera looks along preset `view`.
pub(super) fn facing(a: &Atelier, view: PresetView) -> bool {
    let off = (a.camera.yaw - view.angles().0).rem_euclid(std::f32::consts::TAU);
    off < 0.05 || off > std::f32::consts::TAU - 0.05
}

/// Where piece `i` was put on the body, once it is no longer at the spot new pieces start at
/// (above and in front of the avatar): the placement's translation, metres.
pub(super) fn placed(a: &Atelier, i: usize) -> Option<glam::DVec3> {
    let panel = a.project.template.panels.get(i)?;
    (panel.placement.rotation_degrees.y.abs() > 1. || panel.placement.translation.y < 1.7)
        .then_some(panel.placement.translation)
}

/// Whether the cloth hangs on the torso of the 1.80 m avatar: its centre above 1 m and nothing
/// below 0.75 m. A vest that slid off (as one did on a Windows VM) lies near the floor, and
/// without this the demo still counted every step as done.
fn on_the_body(a: &Atelier) -> bool {
    let x = &a.scene.state.x;
    if x.is_empty() {
        return false;
    }
    let centre = x.iter().map(|p| p.y).sum::<f64>() / x.len() as f64;
    let lowest = x.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    centre > 1.0 && lowest > 0.75
}

pub(super) fn script(name: &str) -> Option<Vec<Step>> {
    match name {
        "vest" => Some(vest()),
        "outfit" => Some(super::demo_outfit::outfit()),
        _ => None,
    }
}

/// Outline of the back, metres, y down from the shoulder-neck points, centred on x = 0, for the
/// library's "Man · medium" (1.80 m; chest 97, waist 84 cm): 8 cm of ease at the chest, armholes
/// 22 cm deep, ending 48 cm down, just below the waist. Dragged points carry their curve handle.
const BACK: [([f64; 2], Option<[f64; 2]>); 13] = [
    ([-0.08, 0.0], None),
    ([0.0, 0.02], Some([0.05, 0.0])),
    ([0.08, 0.0], None),
    ([0.18, 0.045], None),
    ([0.185, 0.13], None),
    ([0.21, 0.19], Some([0.015, 0.02])),
    ([0.255, 0.22], None),
    ([0.24, 0.48], None),
    ([-0.24, 0.48], None),
    ([-0.255, 0.22], None),
    ([-0.21, 0.19], Some([0.015, -0.02])),
    ([-0.185, 0.13], None),
    ([-0.18, 0.045], None),
];
/// The left front (as worn): centre front at x = 0 with a 2 cm button stand, a V neck down to
/// the first button, side seam at the right matching the back's, a point at the centre front.
const FRONT: [([f64; 2], Option<[f64; 2]>); 10] = [
    ([0.08, 0.0], None),
    ([0.18, 0.045], None),
    ([0.185, 0.13], None),
    ([0.21, 0.19], Some([0.015, 0.02])),
    ([0.26, 0.22], None),
    ([0.25, 0.48], None),
    ([0.06, 0.52], None),
    ([-0.02, 0.55], None),
    ([-0.02, 0.27], None),
    ([0.03, 0.13], None),
];
/// Button heights on the centre front, below the V.
const BUTTONS: [f64; 4] = [0.30, 0.37, 0.44, 0.51];

/// Where the drawing starts on the board: its view centre when drawing begins.
pub(super) type Origin = Rc<Cell<DVec2>>;

/// A board point `offset` metres from the origin, found when the step runs.
pub(super) fn board(origin: &Origin, offset: DVec2) -> Target {
    let origin = origin.clone();
    Target::Find(Box::new(move |a: &Atelier| {
        let rect = a.authoring.selection.board_rect?;
        Some(a.authoring.board.screen(rect, origin.get() + offset))
    }))
}

/// Click (or drag, for curve points) every point of an outline placed at `at`, then close it.
pub(super) fn draw(
    steps: &mut Vec<Step>,
    origin: &Origin,
    at: DVec2,
    outline: &[([f64; 2], Option<[f64; 2]>)],
) {
    for (p, handle) in outline {
        let p = at + DVec2::from_array(*p);
        match handle {
            Some(h) => steps.push(Step::Drag(
                board(origin, p),
                board(origin, p + DVec2::from_array(*h)),
            )),
            None => steps.push(Step::Click(board(origin, p))),
        }
    }
    steps.push(Step::Key(egui::Key::Enter, egui::Modifiers::NONE));
}

/// A point of piece `index` (in the order they were made), from its outline as drawn: the
/// piece keeps the outline moved so its lowest corner is at (0, 0).
pub(super) fn piece(
    index: usize,
    drawn: &'static [([f64; 2], Option<[f64; 2]>)],
    at: [f64; 2],
) -> Target {
    Target::Find(Box::new(move |a: &Atelier| {
        let lo = drawn.iter().fold(DVec2::splat(f64::INFINITY), |m, (p, _)| {
            m.min(DVec2::from_array(*p))
        });
        a.board_point_screen(index, DVec2::from_array(at) - lo)
    }))
}

/// The same point on piece `index`, a mirrored copy of `drawn`: its x runs the other way.
pub(super) fn mirrored(
    index: usize,
    drawn: &'static [([f64; 2], Option<[f64; 2]>)],
    at: [f64; 2],
) -> Target {
    Target::Find(Box::new(move |a: &Atelier| {
        let (lo, hi) = drawn.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), (p, _)| (lo.min(DVec2::from_array(*p)), hi.max(DVec2::from_array(*p))),
        );
        let p = DVec2::from_array(at) - lo;
        a.board_point_screen(index, DVec2::new((hi.x - lo.x) - p.x, p.y))
    }))
}

/// Buttons (or buttonholes) on the garment.
pub(super) fn count_trims(a: &Atelier, buttonholes: bool) -> usize {
    use fashioncraft::garment::trims::TrimKind;
    a.project
        .template
        .trims
        .iter()
        .filter(|t| match t.kind {
            TrimKind::Button { .. } => !buttonholes,
            TrimKind::Buttonhole { .. } => buttonholes,
            _ => false,
        })
        .count()
}

/// Arrangement point `id` on the avatar, once the points are shown.
pub(super) fn anchor(id: &'static str) -> Target {
    Target::Find(Box::new(move |a: &Atelier| a.arrangement_point_screen(id)))
}

/// Open `menu`, go through `sub` (if any) and click the command `key`.
pub(super) fn menu(steps: &mut Vec<Step>, menu: &str, sub: Option<&str>, key: &str) {
    steps.push(click(&format!("menu:{menu}")));
    if let Some(sub) = sub {
        steps.push(Step::Hover(Target::widget(&format!("sub:{sub}"))));
    }
    steps.push(click(&format!("cmd:{key}")));
}

fn vest() -> Vec<Step> {
    let origin: Origin = Rc::new(Cell::new(DVec2::ZERO));
    let back_at = DVec2::new(-0.30, -0.33);
    let front_at = DVec2::new(0.20, -0.33);
    let mut s = vec![
        act("start from the default workspace, in English", |a, ctx| {
            // Whatever the last session left (a hidden inspector, Clean view…), the demo starts
            // from the defaults.
            a.prefs.workspace = Default::default();
            a.prefs.garment_display = Default::default();
            a.clean_view = false;
            a.prefs.language = i18n::Language::English;
            a.dispatch_command(commands::CommandId::SplitView, ctx)
        }),
        Step::Wait(1.0),
        Step::Record(true),
        caption("FashionCraft · a vest from scratch"),
        Step::Wait(2.0),
        caption("Start a new project"),
        click("menu:File"),
        click("cmd:file.new"),
        Step::Wait(0.8),
        caption("Choose an avatar from the library"),
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
        caption("Work on the 2D board"),
        click("menu:Window"),
        click("cmd:workspace.view_patterns"),
        Step::Wait(0.8),
        act("frame the board for drawing", {
            let origin = origin.clone();
            move |a, _| {
                a.authoring.board.view_rotation = 0.;
                // Left of the rectangle a new project starts with, which goes once the
                // vest has pieces of its own.
                a.authoring.board.center = DVec2::new(-0.55, 0.05);
                a.authoring.board.scale = 560.;
                origin.set(a.authoring.board.center);
                Ok(())
            }
        }),
        caption("Draw the back with the polygon tool: click the corners, drag for curves"),
        click("menu:2D Pattern"),
        Step::Hover(Target::widget("sub:Create")),
        click("cmd:tool.draw"),
    ];
    draw(&mut s, &origin, back_at, &BACK);
    s.extend([
        until(
            "the back is drawn",
            |a| a.project.template.panels.len() >= 2,
            3.,
        ),
        Step::Wait(0.6),
        caption("Then the front, with its button stand and pointed hem"),
    ]);
    draw(&mut s, &origin, front_at, &FRONT);
    s.extend([
        until(
            "the front is drawn",
            |a| a.project.template.panels.len() >= 3,
            3.,
        ),
        Step::Wait(0.6),
        caption("The rectangle a new project starts with is not needed: select it and delete it"),
        click("menu:2D Pattern"),
        Step::Hover(Target::widget("sub:Edit")),
        click("cmd:tool.edit"),
        Step::Click(Target::Find(Box::new(|a: &Atelier| {
            a.board_point_screen(0, DVec2::new(0.1, 0.3))
        }))),
        guard("the rectangle, and only it, is selected", |a| {
            let first = a.project.template.panels.first().map(|p| p.id.clone());
            a.authoring.selected == first && a.selected_panels().len() <= 1
        }),
        Step::Key(egui::Key::Delete, egui::Modifiers::NONE),
        until(
            "back and front only",
            |a| a.project.template.panels.len() == 2,
            3.,
        ),
        Step::Wait(0.6),
        caption("Mirror the front for the other side: Q opens the quick actions"),
        Step::Click(piece(1, &FRONT, [0.15, 0.40])),
        guard("the front is selected", |a| {
            a.authoring.selected == a.project.template.panels.get(1).map(|p| p.id.clone())
        }),
        Step::Key(egui::Key::Q, egui::Modifiers::NONE),
        click("quick.copy.+X →"),
        until("both fronts", |a| a.project.template.panels.len() >= 3, 4.),
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        Step::Wait(0.8),
        caption("Back to the 3D view beside the board"),
    ]);
    menu(&mut s, "Window", None, "workspace.view_split");
    s.extend([
        Step::Wait(1.0),
        // Home fits every piece in the narrower board of the split view.
        Step::Hover(Target::Find(Box::new(|a: &Atelier| {
            a.authoring.selection.board_rect.map(|r| r.center())
        }))),
        Step::Key(egui::Key::Home, egui::Modifiers::NONE),
        Step::Wait(0.6),
        caption("Show the arrangement points on the avatar"),
    ]);
    menu(
        &mut s,
        "Display",
        Some("Avatar"),
        "display.arrangement_points",
    );
    s.extend([
        Step::Wait(0.8),
        caption("Place each piece: select it, then click its point on the body"),
    ]);
    // The number keys turn the camera at once; a menu on a slow software renderer was not yet
    // open when clicked, and the camera stayed facing front.
    s.extend([
        Step::Key(egui::Key::Num8, egui::Modifiers::NONE),
        until(
            "the camera faces the back",
            |a| facing(a, PresetView::Back),
            3.,
        ),
        Step::Wait(1.2),
        Step::Click(piece(0, &BACK, [0.0, 0.3])),
        Step::Click(anchor("torso-middle-back")),
        Step::Wait(1.0),
        guard("the back sits behind the body", |a| {
            placed(a, 0).is_some_and(|t| t.z < 0.)
        }),
    ]);
    s.extend([
        Step::Key(egui::Key::Num2, egui::Modifiers::NONE),
        until(
            "the camera faces the front",
            |a| facing(a, PresetView::Front),
            3.,
        ),
        Step::Wait(1.2),
        Step::Click(piece(1, &FRONT, [0.15, 0.35])),
        Step::Click(anchor("torso-middle-front-left")),
        Step::Wait(1.0),
        Step::Click(mirrored(2, &FRONT, [0.15, 0.35])),
        Step::Click(anchor("torso-middle-front-right")),
        Step::Wait(1.2),
        guard("both fronts sit on the front of the body", |a| {
            [1, 2]
                .iter()
                .all(|&i| placed(a, i).is_some_and(|t| t.z > -0.05 && t.y < 1.7))
        }),
        caption("Sew the shoulders and the sides: click one edge, then its partner, Enter"),
    ]);
    menu(&mut s, "Sewing", None, "tool.sew");
    // The board shows every piece as seen from the front, the back too (its arrangement point's
    // "right" is the body's left): the back's +x side is the body's left, where the left
    // front goes, and its -x side meets the mirrored, right front.
    for (k, (back, front, copy)) in [
        ([0.13, 0.0225], [0.13, 0.0225], false),
        ([-0.13, 0.0225], [0.13, 0.0225], true),
        ([0.2475, 0.35], [0.255, 0.35], false),
        ([-0.2475, 0.35], [0.255, 0.35], true),
    ]
    .into_iter()
    .enumerate()
    {
        s.push(Step::Click(piece(0, &BACK, back)));
        s.push(Step::Click(if copy {
            mirrored(2, &FRONT, front)
        } else {
            piece(1, &FRONT, front)
        }));
        // The drawn edges differ by a few millimetres (the grid snaps the corners): the
        // confirmation offers to sew them with easing.
        s.push(click("sew.confirm"));
        s.push(until(
            &format!("seam {}", k + 1),
            move |a| a.project.template.seams.len() > k,
            4.,
        ));
    }
    s.extend([
        until("four seams", |a| a.project.template.seams.len() >= 4, 3.),
        caption("Simulate: the vest drapes on the body"),
        click("toolbar.simulate"),
        Step::Wait(9.0),
        click("toolbar.simulate"),
        guard("the vest stays on the body", on_the_body),
        caption("Give it a fabric: thick wool for every piece"),
        click("fabric_tab"),
        click("fabric_presets"),
        click("fabric_preset_wool"),
        click("fabric_all"),
        click("fabric_apply"),
        Step::Wait(0.8),
        caption("And a colour: charcoal grey"),
        click("appearance.enabled"),
    ]);
    for (key, value) in [
        ("appearance.color.red", "52"),
        ("appearance.color.green", "56"),
        ("appearance.color.blue", "66"),
    ] {
        s.extend([
            click(key),
            Step::Key(egui::Key::A, egui::Modifiers::COMMAND),
            Step::Type(value.into()),
            Step::Key(egui::Key::Enter, egui::Modifiers::NONE),
        ]);
    }
    s.extend([
        click("appearance.apply"),
        Step::Wait(1.0),
        caption(
            "Buttonholes down the left front, as menswear has them: choose the tool, click each",
        ),
    ]);
    menu(&mut s, "Materials", Some("Buttons"), "trim.buttonhole");
    for y in BUTTONS {
        s.push(Step::Click(piece(1, &FRONT, [0.0, y])));
    }
    s.extend([
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        until("four buttonholes", |a| count_trims(a, true) >= 4, 2.),
        caption("Buttons on the right front, then fasten them"),
    ]);
    menu(&mut s, "Materials", Some("Buttons"), "trim.button");
    for y in BUTTONS {
        s.push(Step::Click(mirrored(2, &FRONT, [0.0, y])));
    }
    s.extend([
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        until("four buttons", |a| count_trims(a, false) >= 4, 2.),
    ]);
    menu(&mut s, "Materials", Some("Buttons"), "trim.fasten");
    s.extend([
        click("toolbar.simulate"),
        Step::Wait(5.0),
        click("toolbar.simulate"),
        guard("the vest still hangs on the body", on_the_body),
        caption("Made in FashionCraft · open source"),
    ]);
    // A clean finish: no arrangement points, the high-quality view, the 3D view alone without
    // the inspector, annotations hidden (Clean view), framed, then closer on the vest.
    menu(
        &mut s,
        "Display",
        Some("Avatar"),
        "display.arrangement_points",
    );
    menu(&mut s, "Render", None, "view.high_quality");
    menu(&mut s, "Window", None, "workspace.view_garment");
    menu(&mut s, "Window", None, "workspace.inspector");
    menu(&mut s, "Display", Some("Viewpoint"), "view.front");
    s.extend([
        click("header.3d.settings"),
        click("view.clean"),
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        click("header.3d.fit"),
        Step::Wait(0.8),
    ]);
    let torso = || {
        Target::Find(Box::new(|a: &Atelier| {
            a.authoring
                .selection
                .garment_rect
                .map(|r| r.center() - egui::vec2(0., r.height() * 0.12))
        }))
    };
    for _ in 0..2 {
        s.push(Step::Scroll(torso(), 120.));
    }
    s.extend([
        Step::Wait(0.8),
        Step::Orbit(360., 10.),
        Step::Wait(1.0),
        caption(""),
        Step::Record(false),
    ]);
    s
}
