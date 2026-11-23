//! Seams that break by themselves: a stitch pulled harder than its seam's breaking strength
//! for a moment opens, as a tear of the seam; below the strength it holds.
use fashioncraft::{
    authoring::{self, Command, History},
    garment::{
        CompiledGarment, GarmentTemplate,
        seam_break::{BREAK_HOLD_S, SeamBreaker, StitchBreak, stitch_pulls},
        template::Placement,
    },
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

/// Two flat pieces side by side, sewn along the line where they meet.
fn flat_pair() -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    for (p, x) in t.panels.iter_mut().zip([-0.4, 0.0]) {
        p.pins.clear();
        p.pattern.nx = 6;
        p.pattern.ny = 6;
        p.grain_degrees = 0.;
        p.placement = Placement {
            translation: DVec3::new(x, 1.0, 0.0),
            ..Default::default()
        };
    }
    t.seams[0].notches.clear();
    t
}

/// The pair compiled, its outer left edge held where it is and its outer right edge held
/// `pull` metres further right: the cloth is stretched across the seam.
fn pulled(t: &GarmentTemplate, pull: f64) -> Arc<CompiledGarment> {
    let mut g = t.clone().compile().unwrap();
    let x = g.initial_state.x.clone();
    let (left, right) = (g.panels[0].vertices.clone(), g.panels[1].vertices.clone());
    let lo = left.clone().map(|v| x[v].x).fold(f64::INFINITY, f64::min);
    let hi = right
        .clone()
        .map(|v| x[v].x)
        .fold(f64::NEG_INFINITY, f64::max);
    for v in left.filter(|v| (x[*v].x - lo).abs() < 1e-9) {
        g.mesh.pins[v] = Some(x[v]);
    }
    for v in right.filter(|v| (x[*v].x - hi).abs() < 1e-9) {
        let target = x[v] + DVec3::X * pull;
        g.mesh.pins[v] = Some(target);
        g.initial_state.x[v] = target;
    }
    Arc::new(g)
}

struct Run {
    template: GarmentTemplate,
    breaks: Vec<StitchBreak>,
    /// Largest pull any stitch carried, N/m.
    peak: f64,
    /// Mean pull of the stitches at the end, N/m.
    last: f64,
    /// Distance across the seam between the two pieces' points at its middle, m.
    middle: f64,
}

/// Simulate the pulled pair for `steps`, breaking stitches as the app does: each break is
/// applied as a command and the garment rebuilt from the same state.
fn run(template: &GarmentTemplate, steps: usize) -> Run {
    let pull = 0.03;
    let mut t = template.clone();
    let mut g = pulled(&t, pull);
    assert!(
        g.max_seam_gap(&g.initial_state.x) < 1e-9,
        "the pieces start sewn"
    );
    let guide = g.seams[0].guides[4];
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g.clone(), None, &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 16,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    let mut breaker = SeamBreaker::default();
    let (mut breaks, mut peak) = (vec![], 0f64);
    for _ in 0..steps {
        solver.step(&mut s, &settings, None).unwrap();
        for (_, p, _) in g.seams.iter().flat_map(|seam| stitch_pulls(seam, &s.x)) {
            peak = peak.max(p);
        }
        let now = breaker.observe(&t.seam_strengths, &g, &s.x, settings.dt);
        if !now.is_empty() {
            t = authoring::apply(
                &t,
                Command::BreakStitches {
                    breaks: now.clone(),
                },
            )
            .unwrap()
            .template;
            g = pulled(&t, pull);
            solver = VbdSolver::new(g.clone(), None, &s).unwrap();
            breaks.extend(now);
        }
    }
    let pulls: Vec<f64> = g
        .seams
        .iter()
        .flat_map(|seam| stitch_pulls(seam, &s.x))
        .map(|(_, p, _)| p)
        .collect();
    let at = |side: [(usize, f64); 2]| side.iter().map(|(v, w)| s.x[*v] * *w).sum::<DVec3>();
    Run {
        template: t,
        breaks,
        peak,
        last: pulls.iter().sum::<f64>() / pulls.len().max(1) as f64,
        middle: at(guide[0]).distance(at(guide[1])),
    }
}

#[test]
fn a_stitch_breaks_above_its_strength_and_holds_below_it() {
    let base = flat_pair();
    let id = base.seams[0].id.clone();
    // Without a strength nothing breaks: measure what the seam carries.
    let free = run(&base, 120);
    assert!(free.breaks.is_empty() && free.template.seam_tears.is_empty());
    assert!(
        free.last > 1.,
        "the stretched cloth pulls on the seam: {} N/m",
        free.last
    );
    assert!(
        free.middle < 0.01,
        "sewn, the seam stays closed: {} m",
        free.middle
    );

    // Stronger than anything it carries: it holds, and nothing is torn.
    let strong = authoring::apply(
        &base,
        Command::SetSeamStrength {
            seam: id.clone(),
            strength: Some(2. * free.peak),
        },
    )
    .unwrap();
    // A strength is not physics of the compiled garment: setting it keeps the simulation.
    assert!(!strong.reset_simulation);
    let held = run(&strong.template, 120);
    assert!(held.breaks.is_empty(), "{:?}", held.breaks);
    assert!(held.template.seam_tears.is_empty());
    assert!(held.middle < 0.01, "{}", held.middle);

    // Weaker than the pull it carries for good: its stitches break and the seam opens.
    let weak = authoring::apply(
        &base,
        Command::SetSeamStrength {
            seam: id.clone(),
            strength: Some(0.5 * free.last),
        },
    )
    .unwrap()
    .template;
    let torn = run(&weak, 120);
    assert!(!torn.breaks.is_empty());
    assert!(
        torn.breaks
            .iter()
            .all(|b| b.seam == id && b.pull_n_per_m > 0.5 * free.last)
    );
    let tears = &torn.template.seam_tears[&id];
    assert!(!tears.is_empty());
    // Each broken stitch is a stretch of the seam now recorded as torn.
    for b in &torn.breaks {
        assert!(
            tears
                .iter()
                .any(|[lo, hi]| *lo <= b.span[0] + 1e-12 && b.span[1] <= hi + 1e-12),
            "{:?} not within {tears:?}",
            b.span
        );
    }
    assert!(
        torn.middle > 0.02,
        "the torn seam opens under the pull: {} m (sewn {} m)",
        torn.middle,
        free.middle
    );
}

#[test]
fn breaks_are_one_undo_step_and_saved_as_tears() {
    let base = flat_pair();
    let id = base.seams[0].id.clone();
    let free = run(&base, 120);
    let weak = authoring::apply(
        &base,
        Command::SetSeamStrength {
            seam: id.clone(),
            strength: Some(0.5 * free.last),
        },
    )
    .unwrap()
    .template;
    let first = run(&weak, 120).breaks;
    assert!(!first.is_empty());
    // As the app does: the template before the break is recorded, the break applied.
    let mut history = History::default();
    history.record_named(
        weak.clone(),
        Command::BreakStitches { breaks: vec![] }.name(),
    );
    let broken = authoring::apply(
        &weak,
        Command::BreakStitches {
            breaks: first.clone(),
        },
    )
    .unwrap();
    assert!(broken.reset_simulation, "a tear changes the sewing");
    let after = broken.template;
    assert_eq!(history.done().last(), Some("Seam broke"));
    // Undo brings the seam back whole.
    let undone = history.restore(&after, false).unwrap().template;
    assert!(undone.seam_tears.is_empty());
    assert_eq!(undone.seam_strengths.get(&id), Some(&(0.5 * free.last)));
    // Saved and opened again: the tears and the strength are there, the seam sews the same.
    let text = serde_json::to_string(&after).unwrap();
    let reopened: GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(reopened.seam_tears, after.seam_tears);
    assert_eq!(reopened.seam_strengths, after.seam_strengths);
    let sewn = |t: &GarmentTemplate| {
        t.clone().compile().unwrap().seams[0]
            .samples
            .iter()
            .map(|s| s.weight_m)
            .sum::<f64>()
    };
    assert!(sewn(&reopened) < sewn(&weak));
    assert!((sewn(&reopened) - sewn(&after)).abs() < 1e-12);
    // Breaking what is torn already changes nothing.
    assert!(
        authoring::apply(
            &after,
            Command::BreakStitches {
                breaks: first.clone()
            }
        )
        .is_err()
    );
    // Removing the seam takes its strength with it.
    let gone = authoring::apply(&after, Command::RemoveSeam { seam: id })
        .unwrap()
        .template;
    assert!(gone.seam_strengths.is_empty());
}

#[test]
fn a_stitch_breaks_only_once_sewn_and_only_after_holding_the_pull() {
    let t = flat_pair();
    let g = t.clone().compile().unwrap();
    let id = g.seams[0].id.clone();
    let strengths = std::collections::BTreeMap::from([(id.clone(), 10.)]);
    let dt = 1. / 240.;
    // The right piece 1 cm off the left one along the seam's normal: every stitch carries
    // stiffness × 1 cm, well above 10 N/m.
    let mut apart = g.initial_state.x.clone();
    for v in g.panels[1].vertices.clone() {
        apart[v].z += 0.01;
    }
    let pull = stitch_pulls(&g.seams[0], &apart)[0].1;
    assert!(pull > 10., "{pull}");
    // Never sewn (the pieces are still being drawn together): nothing breaks however long.
    let mut breaker = SeamBreaker::default();
    for _ in 0..200 {
        assert!(breaker.observe(&strengths, &g, &apart, dt).is_empty());
    }
    // Sewn, then pulled for less than the hold time: nothing breaks.
    let closed = g.initial_state.x.clone();
    let mut breaker = SeamBreaker::default();
    assert!(breaker.observe(&strengths, &g, &closed, dt).is_empty());
    let short = ((BREAK_HOLD_S / dt).ceil() as usize).saturating_sub(2);
    for _ in 0..short {
        assert!(breaker.observe(&strengths, &g, &apart, dt).is_empty());
    }
    // A moment back under the strength starts the count over.
    assert!(breaker.observe(&strengths, &g, &closed, dt).is_empty());
    for _ in 0..short {
        assert!(breaker.observe(&strengths, &g, &apart, dt).is_empty());
    }
    // Held long enough: every stitch of the seam breaks, once.
    let mut broke = vec![];
    for _ in 0..10 {
        broke.extend(breaker.observe(&strengths, &g, &apart, dt));
    }
    let stitches = stitch_pulls(&g.seams[0], &apart).len();
    assert_eq!(broke.len(), stitches);
    assert!(broke.iter().all(|b| b.seam == id && b.pull_n_per_m > 10.));
    // All of them broken, the whole seam is one tear.
    let after = authoring::apply(&t, Command::BreakStitches { breaks: broke })
        .unwrap()
        .template;
    assert_eq!(after.seam_tears[&id], vec![[0., 1.]]);
    // A seam with no strength is never watched.
    let mut breaker = SeamBreaker::default();
    let none = std::collections::BTreeMap::new();
    breaker.observe(&none, &g, &closed, dt);
    for _ in 0..100 {
        assert!(breaker.observe(&none, &g, &apart, dt).is_empty());
    }
}

#[test]
fn strengths_are_validated() {
    let base = flat_pair();
    let id = base.seams[0].id.clone();
    for bad in [0., -1., f64::NAN, 1e9] {
        assert!(
            authoring::apply(
                &base,
                Command::SetSeamStrength {
                    seam: id.clone(),
                    strength: Some(bad),
                },
            )
            .is_err(),
            "{bad}"
        );
    }
    assert!(
        authoring::apply(
            &base,
            Command::SetSeamStrength {
                seam: "nope".into(),
                strength: Some(100.),
            },
        )
        .is_err()
    );
    let set = authoring::apply(
        &base,
        Command::SetSeamStrength {
            seam: id.clone(),
            strength: Some(100.),
        },
    )
    .unwrap()
    .template;
    let cleared = authoring::apply(
        &set,
        Command::SetSeamStrength {
            seam: id.clone(),
            strength: None,
        },
    )
    .unwrap()
    .template;
    assert!(cleared.seam_strengths.is_empty());
    let mut bad = set.clone();
    bad.seam_strengths.insert("ghost".into(), 100.);
    assert!(bad.compile().is_err());
}

#[test]
fn a_seam_torn_where_its_sides_touch_still_simulates() {
    // The right piece folded square to the left one along the seam, as a garment's front and
    // back meet at a side seam: the two sides touch all along it.
    let mut t = flat_pair();
    t.panels[1].placement.rotation_degrees = DVec3::new(0., 90., 0.);
    let g = Arc::new(t.clone().compile().unwrap());
    assert!(
        g.max_seam_gap(&g.initial_state.x) < 1e-9,
        "the sides start together"
    );
    VbdSolver::new(g.clone(), None, &g.initial_state).unwrap();
    // Torn all along where the sides still touch (as a break leaves them): the solver starts
    // from that state and steps.
    let id = t.seams[0].id.clone();
    let torn = authoring::apply(
        &t,
        Command::SetSeamTears {
            seam: id,
            tears: vec![[0., 1.]],
        },
    )
    .unwrap()
    .garment;
    assert!(torn.seams[0].samples.is_empty());
    let torn = Arc::new(torn);
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(torn.clone(), None, &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 8,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..5 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    assert!(s.x.iter().all(|p| p.is_finite()));
}
