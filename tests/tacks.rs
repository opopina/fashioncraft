//! Tacks: two material points held together by a stiff zero-length spring (basting), compiled
//! as a one-sample seam. Two separate weightless squares tacked edge to edge are drawn
//! together; a button fastened to the buttonhole on an overlapping flap meets it; unfastening
//! releases them.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        CompiledGarment, GarmentTemplate,
        template::{FASTEN_PREFIX, Tack, TackPoint},
        trims::{Trim, TrimKind},
    },
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

/// Two upright 10 cm squares, the second `dx` to the right and `dz` in front of the first.
fn pair(dx: f64, dz: f64) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.seams.clear();
    for (k, p) in t.panels.iter_mut().enumerate() {
        p.pins.clear();
        p.placement.rotation_degrees = DVec3::ZERO;
        p.pattern = fashioncraft::model::Pattern {
            width: 0.1,
            height: 0.1,
            nx: 5,
            ny: 5,
        };
        p.placement.translation = DVec3::new(dx * k as f64, 1., dz * k as f64);
    }
    t
}

fn world(g: &CompiledGarment, s: &State, p: &TackPoint) -> DVec3 {
    g.material_point(&p.panel, p.uv)
        .unwrap()
        .iter()
        .map(|(v, w)| s.x[*v] * *w)
        .sum()
}

fn settle(t: &GarmentTemplate, seconds: f64) -> (Arc<CompiledGarment>, State) {
    let g = Arc::new(t.clone().compile().unwrap());
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..(seconds * 240.) as usize {
        solver.step(&mut s, &settings, None).unwrap();
    }
    (g, s)
}

#[test]
fn a_tack_draws_two_squares_together() {
    let t = pair(0.15, 0.);
    let (a, b) = (t.panels[0].id.clone(), t.panels[1].id.clone());
    let tack = Tack {
        id: "baste".into(),
        a: TackPoint {
            panel: a,
            uv: DVec2::new(0.1, 0.05),
        },
        b: TackPoint {
            panel: b,
            uv: DVec2::new(0., 0.05),
        },
    };
    let tacked = authoring::apply(&t, Command::AddTack { tack: tack.clone() })
        .unwrap()
        .template;
    let (g, start) = (
        Arc::new(tacked.clone().compile().unwrap()),
        tacked.clone().compile().unwrap().initial_state,
    );
    let before = world(&g, &start, &tack.a).distance(world(&g, &start, &tack.b));
    let (g, s) = settle(&tacked, 1.5);
    let after = world(&g, &s, &tack.a).distance(world(&g, &s, &tack.b));
    assert!(before > 0.04, "{before}");
    assert!(after < 0.005, "drawn together: {after}");
    // Untacked, they stay apart.
    let (g, s) = settle(&t, 0.5);
    assert!(world(&g, &s, &tack.a).distance(world(&g, &s, &tack.b)) > 0.04);
}

#[test]
fn a_button_fastens_to_the_buttonhole_on_the_other_flap() {
    // Two flaps overlapping by 4 cm, 1 cm apart: a button on the front one, a buttonhole on
    // the back one, roughly facing each other.
    let mut t = pair(0.06, 0.01);
    let (a, b) = (t.panels[0].id.clone(), t.panels[1].id.clone());
    t.trims = vec![
        Trim {
            id: "button".into(),
            panel_id: b.clone(),
            kind: TrimKind::Button {
                center_m: DVec2::new(0.02, 0.05),
                diameter_m: 0.012,
            },
        },
        Trim {
            id: "hole".into(),
            panel_id: a.clone(),
            kind: TrimKind::Buttonhole {
                start_m: DVec2::new(0.07, 0.05),
                end_m: DVec2::new(0.09, 0.05),
                width_m: 0.004,
            },
        },
    ];
    let g = t.clone().compile().unwrap();
    let edit = authoring::prepare_fasten_edit(&t, &g, &g.initial_state, true).unwrap();
    assert!(edit.reset_simulation);
    let tack = edit
        .template
        .tacks
        .iter()
        .find(|t| t.id.starts_with(FASTEN_PREFIX))
        .cloned()
        .expect("one fastening");
    assert_eq!(
        (tack.a.panel.as_str(), tack.b.panel.as_str()),
        (b.as_str(), a.as_str())
    );
    let (g, s) = settle(&edit.template, 1.5);
    let gap = world(&g, &s, &tack.a).distance(world(&g, &s, &tack.b));
    assert!(gap < 0.006, "button on its buttonhole: {gap}");
    let open = authoring::prepare_fasten_edit(&edit.template, &edit.garment, &s, false).unwrap();
    assert!(open.template.tacks.is_empty());
    assert!(authoring::prepare_fasten_edit(&open.template, &open.garment, &s, false).is_err());
}

#[test]
fn tacks_are_validated_and_removable() {
    let t = pair(0.15, 0.);
    let id = t.panels[0].id.clone();
    let point = TackPoint {
        panel: id.clone(),
        uv: DVec2::new(0.05, 0.05),
    };
    let tack = Tack {
        id: "t".into(),
        a: point.clone(),
        b: TackPoint {
            panel: "nowhere".into(),
            uv: DVec2::ZERO,
        },
    };
    assert!(authoring::apply(&t, Command::AddTack { tack }).is_err());
    let ok = Tack {
        id: "t".into(),
        a: point.clone(),
        b: TackPoint {
            panel: t.panels[1].id.clone(),
            uv: DVec2::new(0.05, 0.05),
        },
    };
    let one = authoring::apply(&t, Command::AddTack { tack: ok.clone() })
        .unwrap()
        .template;
    assert!(
        authoring::apply(&one, Command::AddTack { tack: ok }).is_err(),
        "same id"
    );
    let g = one.clone().compile().unwrap();
    assert_eq!(
        g.seams.iter().filter(|s| s.id.starts_with("tack:")).count(),
        1
    );
    let none = authoring::apply(
        &one,
        Command::RemoveTacks {
            ids: vec!["t".into()],
        },
    )
    .unwrap()
    .template;
    assert!(none.tacks.is_empty());
    assert!(!serde_json::to_string(&t).unwrap().contains("tacks"));
}
