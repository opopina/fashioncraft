//! The demo for the promo video: wide trousers designed from scratch in the new interface, every
//! change made with a visible gesture, the way a designer works. Each piece is drawn on the 2D
//! board with the polygon tool, placed on its point of the body, sewn edge to edge, simulated,
//! adjusted at the hem, given a fabric, and coloured by the Assistant asked in Spanish.
//!
//!     FASHIONCRAFT_DEMO=cargo FASHIONCRAFT_DEMO_RECORD=1 FASHIONCRAFT_GPU_COMPUTE=0 fashioncraft
//!
//! The Assistant step talks to a stand-in server on this computer (the demo stops before it
//! when the preferences point anywhere else). No step changes the garment without a gesture:
//! the only `act`s read where the board is, to aim the drawing.
use super::demo::{Step, Target, act, caption, click, guard, until};
use super::demo_vest::{Origin, anchor, draw, menu, piece, placed};
use super::*;
use glam::DVec2;
use std::{cell::Cell, rc::Rc};

type Outline = [([f64; 2], Option<[f64; 2]>); 7];

/// One front leg, metres, y down from the waist: the crotch on the left, the side seam on the
/// right, as the leg anchors lay a piece (its right goes to the outside of the leg on either
/// leg). Waist 22.4 cm, hip 33.8 cm across, a 7 cm crotch extension 30 cm down, a wide straight
/// leg to a 31 cm hem, 100 cm long.
const FRONT: Outline = [
    ([0.091, 0.0], None),
    ([0.3147, 0.0], None),
    ([0.3381, 0.21], None),
    ([0.3246, 1.0], None),
    ([0.0131, 1.0], None),
    ([0.0, 0.30], None),
    ([0.0676, 0.21], None),
];
/// One back leg: the same, with a longer crotch extension (11 cm) and more seat.
const BACK: Outline = [
    ([0.1316, 0.0], None),
    ([0.3553, 0.0], None),
    ([0.3787, 0.21], None),
    ([0.3652, 1.0], None),
    ([0.0537, 1.0], None),
    ([0.0, 0.30], None),
    ([0.1082, 0.21], None),
];
/// The pieces once the starting rectangle is gone.
const FRONT_RIGHT: usize = 0;
const FRONT_LEFT: usize = 1;
const BACK_RIGHT: usize = 2;
const BACK_LEFT: usize = 3;

/// What the Assistant is asked, typed letter by letter.
const ASK: &str = "Pon el pantalón en color crudo y vuelve a simular la caída.";

/// A point `f` of the way along an edge from its waist end `top` to its other end.
fn along(top: [f64; 2], other: [f64; 2], f: f64) -> [f64; 2] {
    [
        top[0] + (other[0] - top[0]) * f,
        top[1] + (other[1] - top[1]) * f,
    ]
}

/// The outline's corner `k`.
fn corner(outline: &Outline, k: usize) -> [f64; 2] {
    outline[k].0
}

/// Whether the trousers hang on the 1.80 m avatar: the waist above 0.95 m and the cloth's centre
/// between the shins and the hips. Trousers that slid would sit low.
fn trousers_on_body(a: &Atelier) -> bool {
    let x = &a.scene.state.x;
    if x.is_empty() {
        return false;
    }
    let centre = x.iter().map(|p| p.y).sum::<f64>() / x.len() as f64;
    let highest = x.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    highest > 0.95 && (0.35..0.95).contains(&centre)
}

/// The cargo pocket on a front leg, stitched on three sides (its top is the opening): the
/// corners, and its button.
const POCKET: [[f64; 2]; 4] = [[0.12, 0.42], [0.12, 0.60], [0.26, 0.60], [0.26, 0.42]];
const POCKET_BUTTON: [f64; 2] = [0.19, 0.445];

/// Internal lines on piece `i`.
fn lines(a: &Atelier, i: usize) -> usize {
    a.project
        .template
        .panels
        .get(i)
        .and_then(|p| p.polygon.as_ref())
        .map_or(0, |p| p.lines.len())
}

/// Topstitches on the garment.
fn topstitches(a: &Atelier) -> usize {
    use fashioncraft::garment::trims::TrimKind;
    a.project
        .template
        .trims
        .iter()
        .filter(|t| matches!(t.kind, TrimKind::Topstitch { .. }))
        .count()
}

/// Type `text` letter by letter into the focused field.
fn type_slowly(steps: &mut Vec<Step>, text: &str) {
    for c in text.chars() {
        steps.push(Step::Type(c.to_string()));
        steps.push(Step::Wait(if c == ' ' { 0.09 } else { 0.05 }));
    }
}

/// The centre of the 3D view, offset by `dx` points.
fn view3d(dx: f32) -> Target {
    Target::Find(Box::new(move |a: &Atelier| {
        a.authoring
            .selection
            .garment_rect
            .map(|r| r.center() + egui::vec2(dx, 0.))
    }))
}

/// The centre of the 2D board.
fn board_centre() -> Target {
    Target::Find(Box::new(|a: &Atelier| {
        a.authoring.selection.board_rect.map(|r| r.center())
    }))
}

/// Sew edge `a` of piece `pa` to edge `b` of piece `pb`: the sewing tool, a click near the waist
/// end of each (the app joins the ends clicked near), and the confirmation.
fn sew(
    steps: &mut Vec<Step>,
    base: &Rc<Cell<usize>>,
    k: usize,
    (pa, oa, a): (usize, &'static Outline, [f64; 2]),
    (pb, ob, b): (usize, &'static Outline, [f64; 2]),
) {
    steps.push(Step::Click(piece(pa, oa, a)));
    steps.push(Step::Wait(0.25));
    steps.push(Step::Click(piece(pb, ob, b)));
    steps.push(Step::Wait(0.25));
    steps.push(click("sew.confirm"));
    let base = base.clone();
    steps.push(until(
        &format!("seam {}", k + 1),
        move |a| a.project.template.seams.len() > base.get() + k,
        4.,
    ));
}

pub(super) fn cargo() -> Vec<Step> {
    let origin: Origin = Rc::new(Cell::new(DVec2::ZERO));
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
        caption("FashionCraft · un pantalón ancho desde cero"),
        Step::Wait(2.0),
        caption("Un proyecto nuevo"),
    ];
    menu(&mut s, "File", None, "file.new");
    s.extend([Step::Wait(0.8), caption("Un avatar de la biblioteca")]);
    menu(&mut s, "Avatar", None, "avatar.library");
    s.extend([
        Step::Wait(2.0),
        click("library.use.5"),
        Step::Wait(1.0),
        click("avatar.confirm"),
        until(
            "the library avatar is in place",
            |a| fashioncraft::avatar_look::look_file(&a.project.avatar_path).is_some(),
            20.,
        ),
        Step::Wait(1.2),
        caption("El tablero 2D a pantalla completa para dibujar"),
    ]);
    menu(&mut s, "Window", None, "workspace.view_patterns");
    s.extend([
        Step::Wait(0.8),
        click("header.2d.fit"),
        Step::Wait(0.6),
        Step::Scroll(board_centre(), -220.),
        Step::Wait(0.4),
        Step::Scroll(board_centre(), -220.),
        Step::Wait(0.8),
        // Read where the board is (no change): the pieces are drawn around its centre.
        act("aim the drawing at the board's centre", {
            let origin = origin.clone();
            move |a, _| {
                origin.set(a.authoring.board.center);
                Ok(())
            }
        }),
        guard("the board shows the whole drawing area", |a| {
            (250.0..=700.0).contains(&a.authoring.board.scale)
        }),
        caption("Herramienta polígono: clic en cada esquina del delantero"),
        click("menu:2D Pattern"),
        Step::Hover(Target::widget("sub:Create")),
        click("cmd:tool.draw"),
    ]);
    // Front and back of each leg, side by side.
    let at = [
        DVec2::new(-0.86, -0.5),
        DVec2::new(-0.46, -0.5),
        DVec2::new(-0.04, -0.5),
        DVec2::new(0.40, -0.5),
    ];
    draw(&mut s, &origin, at[0], &FRONT);
    s.extend([
        until(
            "the first front",
            |a| a.project.template.panels.len() >= 2,
            3.,
        ),
        Step::Wait(0.4),
        caption("El otro delantero: el avatar le da la vuelta al colocarlo"),
    ]);
    draw(&mut s, &origin, at[1], &FRONT);
    s.extend([
        until(
            "the second front",
            |a| a.project.template.panels.len() >= 3,
            3.,
        ),
        Step::Wait(0.4),
        caption("Los traseros, con más tiro"),
    ]);
    draw(&mut s, &origin, at[2], &BACK);
    s.push(until(
        "the first back",
        |a| a.project.template.panels.len() >= 4,
        3.,
    ));
    s.push(Step::Wait(0.3));
    draw(&mut s, &origin, at[3], &BACK);
    s.extend([
        until("four legs", |a| a.project.template.panels.len() >= 5, 3.),
        Step::Wait(0.6),
        caption("El rectángulo inicial sobra: seleccionarlo y Supr"),
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
            "the four legs only",
            |a| a.project.template.panels.len() == 4,
            3.,
        ),
        Step::Wait(0.6),
        caption("De vuelta a la vista dividida: 3D y 2D"),
    ]);
    menu(&mut s, "Window", None, "workspace.view_split");
    s.extend([
        Step::Wait(0.8),
        click("header.2d.fit"),
        Step::Wait(0.6),
        caption("Los puntos de colocación del avatar"),
    ]);
    menu(
        &mut s,
        "Display",
        Some("Avatar"),
        "display.arrangement_points",
    );
    s.extend([
        Step::Wait(0.8),
        caption("Cada pieza a su punto: clic en la pieza, luego en el muslo"),
        Step::Click(piece(FRONT_RIGHT, &FRONT, [0.17, 0.5])),
        Step::Click(anchor("thigh-right-front")),
        Step::Wait(0.8),
        Step::Click(piece(FRONT_LEFT, &FRONT, [0.17, 0.5])),
        Step::Click(anchor("thigh-left-front")),
        Step::Wait(0.8),
        guard("both fronts sit in front of the legs", |a| {
            [FRONT_RIGHT, FRONT_LEFT]
                .iter()
                .all(|&i| placed(a, i).is_some_and(|t| t.z > 0.))
        }),
        caption("8 gira la cámara a la espalda"),
        Step::Key(egui::Key::Num8, egui::Modifiers::NONE),
        until(
            "the camera faces the back",
            |a| super::demo_vest::facing(a, crate::render::PresetView::Back),
            3.,
        ),
        Step::Wait(0.8),
        Step::Click(piece(BACK_RIGHT, &BACK, [0.19, 0.5])),
        Step::Click(anchor("thigh-right-back")),
        Step::Wait(0.8),
        Step::Click(piece(BACK_LEFT, &BACK, [0.19, 0.5])),
        Step::Click(anchor("thigh-left-back")),
        Step::Wait(0.8),
        guard("both backs sit behind the legs", |a| {
            [BACK_RIGHT, BACK_LEFT]
                .iter()
                .all(|&i| placed(a, i).is_some_and(|t| t.z < 0.))
        }),
        caption("2 vuelve al frente"),
        Step::Key(egui::Key::Num2, egui::Modifiers::NONE),
        Step::Wait(0.8),
    ]);
    menu(
        &mut s,
        "Display",
        Some("Avatar"),
        "display.arrangement_points",
    );
    s.extend([
        Step::Wait(0.5),
        act("count the seams there are", {
            let base = base_seams.clone();
            move |a, _| {
                base.set(a.project.template.seams.len());
                Ok(())
            }
        }),
        caption("Coser: clic cerca de la cintura en un borde, luego en su pareja"),
    ]);
    menu(&mut s, "Sewing", None, "tool.sew");
    s.push(Step::Wait(0.4));
    // Each leg: the side (two edges) and the inside leg, front to back; then the two fronts at
    // the centre and the crotch, and the two backs the same.
    let f = &FRONT;
    let b = &BACK;
    let seams: Vec<(
        (usize, &'static Outline, [f64; 2]),
        (usize, &'static Outline, [f64; 2]),
    )> = vec![
        // Side, upper: waist corner (1) down to the hip (2).
        (
            (FRONT_RIGHT, &FRONT, along(corner(f, 1), corner(f, 2), 0.15)),
            (BACK_RIGHT, &BACK, along(corner(b, 1), corner(b, 2), 0.15)),
        ),
        // Side, lower: hip (2) down to the hem (3).
        (
            (FRONT_RIGHT, &FRONT, along(corner(f, 2), corner(f, 3), 0.15)),
            (BACK_RIGHT, &BACK, along(corner(b, 2), corner(b, 3), 0.15)),
        ),
        // Inside leg: crotch point (5) down to the hem (4).
        (
            (FRONT_RIGHT, &FRONT, along(corner(f, 5), corner(f, 4), 0.15)),
            (BACK_RIGHT, &BACK, along(corner(b, 5), corner(b, 4), 0.15)),
        ),
        (
            (FRONT_LEFT, &FRONT, along(corner(f, 1), corner(f, 2), 0.15)),
            (BACK_LEFT, &BACK, along(corner(b, 1), corner(b, 2), 0.15)),
        ),
        (
            (FRONT_LEFT, &FRONT, along(corner(f, 2), corner(f, 3), 0.15)),
            (BACK_LEFT, &BACK, along(corner(b, 2), corner(b, 3), 0.15)),
        ),
        (
            (FRONT_LEFT, &FRONT, along(corner(f, 5), corner(f, 4), 0.15)),
            (BACK_LEFT, &BACK, along(corner(b, 5), corner(b, 4), 0.15)),
        ),
        // Centre front: waist (0) down to the crotch seam (6); then the crotch (6) to (5).
        (
            (FRONT_RIGHT, &FRONT, along(corner(f, 0), corner(f, 6), 0.15)),
            (FRONT_LEFT, &FRONT, along(corner(f, 0), corner(f, 6), 0.15)),
        ),
        (
            (FRONT_RIGHT, &FRONT, along(corner(f, 6), corner(f, 5), 0.15)),
            (FRONT_LEFT, &FRONT, along(corner(f, 6), corner(f, 5), 0.15)),
        ),
        // Centre back and the back crotch.
        (
            (BACK_RIGHT, &BACK, along(corner(b, 0), corner(b, 6), 0.15)),
            (BACK_LEFT, &BACK, along(corner(b, 0), corner(b, 6), 0.15)),
        ),
        (
            (BACK_RIGHT, &BACK, along(corner(b, 6), corner(b, 5), 0.15)),
            (BACK_LEFT, &BACK, along(corner(b, 6), corner(b, 5), 0.15)),
        ),
    ];
    for (k, (a, b)) in seams.into_iter().enumerate() {
        sew(&mut s, &base_seams, k, a, b);
    }
    s.extend([
        {
            let base = base_seams.clone();
            until(
                "ten seams",
                move |a| a.project.template.seams.len() >= base.get() + 10,
                3.,
            )
        },
        Step::Wait(0.6),
    ]);
    // Cargo pockets: an internal line on each front leg, over the outside of the thigh, then a
    // free topstitch along it (finishing a line leaves it selected).
    s.push(caption(
        "Bolsillos cargo: una línea interna en cada delantero, y su pespunte",
    ));
    // Closer to the two fronts to draw them (the wheel zooms toward the cursor).
    s.push(Step::Scroll(piece(FRONT_RIGHT, &FRONT, [0.45, 0.5]), 260.));
    s.push(Step::Wait(0.8));
    for (k, leg) in [FRONT_RIGHT, FRONT_LEFT].into_iter().enumerate() {
        s.push(click("menu:2D Pattern"));
        s.push(Step::Hover(Target::widget("sub:Create")));
        s.push(click("cmd:tool.internal_line"));
        // A click on the piece's side seam chooses it; then the pocket's corners; Enter.
        s.push(Step::Click(piece(
            leg,
            &FRONT,
            along(corner(&FRONT, 2), corner(&FRONT, 3), 0.45),
        )));
        s.push(Step::Wait(0.3));
        for p in POCKET {
            s.push(Step::Click(piece(leg, &FRONT, p)));
            s.push(Step::Wait(0.2));
        }
        s.push(Step::Key(egui::Key::Enter, egui::Modifiers::NONE));
        s.push(until(
            &format!("pocket {}", k + 1),
            move |a| lines(a, leg) >= 1,
            3.,
        ));
        s.push(Step::Wait(0.4));
        menu(
            &mut s,
            "Materials",
            Some("Topstitch"),
            "materials.free_topstitch",
        );
        s.push(until(
            &format!("pocket topstitch {}", k + 1),
            move |a| topstitches(a) > k,
            3.,
        ));
        s.push(Step::Wait(0.4));
    }
    s.push(caption("Un botón en cada bolsillo"));
    menu(&mut s, "Materials", Some("Buttons"), "trim.button");
    for leg in [FRONT_RIGHT, FRONT_LEFT] {
        s.push(Step::Click(piece(leg, &FRONT, POCKET_BUTTON)));
        s.push(Step::Wait(0.3));
    }
    s.extend([
        Step::Key(egui::Key::Escape, egui::Modifiers::NONE),
        until(
            "two buttons",
            |a| super::demo_vest::count_trims(a, false) >= 2,
            2.,
        ),
        click("header.2d.fit"),
        Step::Wait(0.6),
        caption("Pespunte de pretina en la cintura de cada pieza"),
        click("menu:2D Pattern"),
        Step::Hover(Target::widget("sub:Edit")),
        click("cmd:tool.edit"),
    ]);
    for (k, (leg, outline)) in [
        (FRONT_RIGHT, &FRONT),
        (FRONT_LEFT, &FRONT),
        (BACK_RIGHT, &BACK),
        (BACK_LEFT, &BACK),
    ]
    .into_iter()
    .enumerate()
    {
        s.push(Step::Click(piece(
            leg,
            outline,
            along(corner(outline, 0), corner(outline, 1), 0.5),
        )));
        s.push(Step::Wait(0.3));
        menu(&mut s, "Materials", Some("Topstitch"), "trims.topstitch");
        s.push(until(
            &format!("waist topstitch {}", k + 1),
            move |a| topstitches(a) > 2 + k,
            3.,
        ));
    }
    s.extend([
        Step::Wait(0.5),
        caption("Un ajuste antes de simular: el bajo 4 cm más ancho, arrastrando su esquina"),
        Step::Drag(
            piece(FRONT_RIGHT, &FRONT, corner(&FRONT, 3)),
            piece(
                FRONT_RIGHT,
                &FRONT,
                [corner(&FRONT, 3)[0] + 0.04, corner(&FRONT, 3)[1]],
            ),
        ),
        Step::Wait(0.5),
        Step::Drag(
            piece(FRONT_LEFT, &FRONT, corner(&FRONT, 3)),
            piece(
                FRONT_LEFT,
                &FRONT,
                [corner(&FRONT, 3)[0] + 0.04, corner(&FRONT, 3)[1]],
            ),
        ),
        Step::Wait(0.8),
        caption("Algodón sobre la piel: fricción 0,6 en la pestaña Simulación"),
        click("simulation_tab"),
        Step::Wait(0.8),
        Step::Scroll(
            // The inspector: right of the 2D board.
            Target::Find(Box::new(|a: &Atelier| {
                a.authoring
                    .selection
                    .board_rect
                    .map(|r| egui::pos2(r.right() + 130., r.center().y))
            })),
            -600.,
        ),
        Step::Wait(1.0),
        click("friction.value"),
        Step::Key(egui::Key::A, egui::Modifiers::COMMAND),
        Step::Type("0.6".into()),
        Step::Key(egui::Key::Enter, egui::Modifiers::NONE),
        Step::Wait(0.5),
        guard("the friction is 0.6", |a| {
            (a.settings.friction - 0.6).abs() < 1e-6
        }),
        Step::Wait(0.6),
        caption("Simular: el pantalón cae sobre el avatar"),
        click("toolbar.simulate"),
        Step::Wait(32.0),
        click("toolbar.simulate"),
        guard("the trousers hang on the body", trousers_on_body),
        Step::Wait(0.8),
        caption("La tela: algodón ligero en todas las piezas"),
        Step::Click(piece(FRONT_RIGHT, &FRONT, [0.17, 0.5])),
        click("fabric_tab"),
        click("fabric_presets"),
        click("fabric_preset_cotton"),
        click("fabric_all"),
        click("fabric_apply"),
        Step::Wait(1.0),
        caption("El Asistente: se le pide en español"),
    ]);
    menu(&mut s, "Window", None, "window.assistant");
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
        Step::Wait(0.4),
    ]);
    type_slowly(&mut s, ASK);
    s.extend([
        Step::Wait(0.5),
        Step::Key(egui::Key::Enter, egui::Modifiers::NONE),
        until(
            "the Assistant answered",
            {
                let entries = entries.clone();
                move |a| !a.assistant_busy() && a.assistant_entries() > entries.get() + 1
            },
            90.,
        ),
        Step::Wait(3.0),
        guard(
            "the trousers hang on the body after the Assistant",
            trousers_on_body,
        ),
        caption("Girar la cámara: arrastrar con el botón derecho"),
        Step::DragRight(view3d(-160.), view3d(160.)),
        Step::Wait(0.4),
        Step::DragRight(view3d(-160.), view3d(160.)),
        Step::Wait(0.4),
        Step::DragRight(view3d(-160.), view3d(160.)),
        Step::Wait(0.4),
        Step::DragRight(view3d(-160.), view3d(160.)),
        Step::Wait(1.0),
        caption("Hecho en FashionCraft · open source"),
        Step::Wait(2.5),
        caption(""),
        Step::Record(false),
    ]);
    s
}
