//! Elastic and tape along a line of a piece: tension-only springs between its consecutive
//! vertices. An elastic at 70 % draws a free edge of a weightless sheet in while the opposite
//! edge keeps its length; tape on the sides of a heavy hanging sheet keeps them from
//! stretching.
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate, seam::Side, template::LineSpring},
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

fn far_away() -> SurfaceMesh {
    SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    }
}

fn sheet(pinned: bool, density: f64) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    t.fabrics[0].properties.density = density;
    let p = &mut t.panels[0];
    if !pinned {
        p.pins.clear();
    }
    p.pattern = marvelous_cloth::model::Pattern {
        width: 0.2,
        height: 0.2,
        nx: 8,
        ny: 8,
    };
    p.placement.translation = DVec3::new(-0.1, 1., 0.);
    t
}

fn with(t: &GarmentTemplate, spring: LineSpring) -> GarmentTemplate {
    authoring::apply(
        t,
        Command::SetLineSpring {
            panel: spring.panel.clone(),
            line: spring.line.clone(),
            spring: Some(spring),
        },
    )
    .unwrap()
    .template
}

fn run(t: &GarmentTemplate, gravity: DVec3, seconds: f64) -> (Arc<CompiledGarment>, State) {
    let g = Arc::new(t.clone().compile().unwrap());
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g.clone(), Some(&far_away()), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 3.,
        gravity,
        ..Default::default()
    };
    for _ in 0..(seconds * 240.) as usize {
        solver.step(&mut s, &settings, None).unwrap();
    }
    (g, s)
}

/// Length of a side's vertex chain at positions `x`, over its cut length.
fn side_ratio(t: &GarmentTemplate, side: Side, x: &[DVec3], cut: &[DVec3]) -> f64 {
    let chain = t.panels[0].boundary_vertices(side, 0);
    let length = |x: &[DVec3]| -> f64 { chain.windows(2).map(|w| x[w[0]].distance(x[w[1]])).sum() };
    length(x) / length(cut)
}

#[test]
fn an_elastic_draws_a_free_edge_in() {
    let t = sheet(false, 0.2);
    let id = t.panels[0].id.clone();
    let elastic = LineSpring {
        ratio: 70.,
        strength: 200.,
        ..LineSpring::elastic(&id, "top")
    };
    let gathered = with(&t, elastic);
    let (g, s) = run(&gathered, DVec3::ZERO, 1.5);
    let cut = &g.initial_state.x;
    let chain = t.panels[0].boundary_vertices(Side::Top, 0);
    let ends = |x: &[DVec3]| x[chain[0]].distance(x[*chain.last().unwrap()]);
    let top = ends(&s.x) / ends(cut);
    let bottom = side_ratio(&t, Side::Bottom, &s.x, cut);
    assert!(top < 0.8, "the elastic edge is drawn in: {top}");
    assert!(bottom > 0.95, "the far edge keeps its length: {bottom}");
}

#[test]
fn tape_keeps_the_sides_of_a_hanging_sheet_from_stretching() {
    // A heavy sheet hanging from its top corners stretches down its sides; taped sides hold.
    let t = sheet(true, 2.);
    let id = t.panels[0].id.clone();
    let taped = with(
        &with(&t, LineSpring::tape(&id, "left")),
        LineSpring::tape(&id, "right"),
    );
    let gravity = DVec3::new(0., -9.81, 0.);
    let (g, plain) = run(&t, gravity, 1.);
    let (_, held) = run(&taped, gravity, 1.);
    let cut = &g.initial_state.x;
    let free = side_ratio(&t, Side::Left, &plain.x, cut) - 1.;
    let tape = side_ratio(&t, Side::Left, &held.x, cut) - 1.;
    assert!(free > 0.01, "the plain side stretches: {free}");
    assert!(tape < 0.3 * free, "the taped side holds: {tape} vs {free}");
}

#[test]
fn elastics_are_validated_and_removable() {
    let t = sheet(false, 0.2);
    let id = t.panels[0].id.clone();
    let mut wrong = t.clone();
    wrong.line_springs.push(LineSpring::elastic(&id, "nowhere"));
    assert!(wrong.compile().is_err(), "unknown line");
    let mut stretched_tape = LineSpring::tape(&id, "top");
    stretched_tape.ratio = 90.;
    assert!(
        authoring::apply(
            &t,
            Command::SetLineSpring {
                panel: id.clone(),
                line: "top".into(),
                spring: Some(stretched_tape)
            }
        )
        .is_err()
    );
    let elastic = with(&t, LineSpring::elastic(&id, "top"));
    assert_eq!(
        elastic.compile().unwrap().lines.len(),
        8,
        "one spring per edge"
    );
    let removed = authoring::apply(
        &elastic,
        Command::SetLineSpring {
            panel: id.clone(),
            line: "top".into(),
            spring: None,
        },
    )
    .unwrap()
    .template;
    assert!(removed.line_springs.is_empty());
    assert!(!serde_json::to_string(&t).unwrap().contains("line_springs"));
}
