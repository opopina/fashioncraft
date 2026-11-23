//! Grading: sizes, grade rules, proportional points, curve rules, notches, graphics, points of
//! measure, persistence, undo, the size on show in the simulation, and the AAMA/ASTM rule
//! table (`.RUL`) beside an apparel DXF, out and back.
use fashioncraft::{
    authoring::History,
    garment::{
        GarmentTemplate, curves,
        dxf_aama::{self, Dialect, ReadOptions, Units, WriteOptions, template::PieceOptions},
        grading::{
            Grading, NotchMode, NotchRule, Target,
            apply::{self, grade_polygon, point_deltas, template_in_size},
            dxf, eval_mm, pom, rul,
        },
        graphics::{GraphicPlacement, GraphicSide},
        markings::{Notch, NotchKind, polyline_length},
        presets::{TshirtMeasurements, tshirt},
    },
};
use glam::DVec2;
use std::collections::BTreeMap;

const MM: f64 = 0.001;

fn point(panel: &str, edge: &str) -> Target {
    Target::Point {
        panel: panel.into(),
        edge: edge.into(),
    }
}

fn polygon<'a>(
    t: &'a GarmentTemplate,
    panel: &str,
) -> &'a fashioncraft::garment::polygon::PolygonPattern {
    t.panels
        .iter()
        .find(|p| p.id == panel)
        .unwrap()
        .polygon
        .as_ref()
        .unwrap()
}

fn index(t: &GarmentTemplate, panel: &str, edge: &str) -> usize {
    polygon(t, panel)
        .edges
        .iter()
        .position(|e| e == edge)
        .unwrap()
}

/// A T-shirt in S, M (base), L and XL: wider by 20 mm a size (10 mm each side, on both
/// pieces alike), the hem 15 mm lower a size, the neck unchanged.
fn graded_tshirt() -> GarmentTemplate {
    let mut t = tshirt(&TshirtMeasurements::default(), 0).unwrap();
    t.name = "Graded T-shirt".into();
    let mut g = Grading::new("Women", &["S", "M", "L", "XL"], 1).unwrap();
    for panel in ["front", "back"] {
        for (edge, step) in [
            ("shoulder-right", DVec2::new(5., 0.)),
            ("sleeve-top-right", DVec2::new(8., 0.)),
            ("cuff-right", DVec2::new(10., 3.)),
            ("sleeve-under-right", DVec2::new(10., 6.)),
            ("side-right", DVec2::new(10., 6.)),
            ("hem", DVec2::new(10., 15.)),
            ("side-left", DVec2::new(-10., 15.)),
            ("sleeve-under-left", DVec2::new(-10., 6.)),
            ("cuff-left", DVec2::new(-10., 6.)),
            ("sleeve-top-left", DVec2::new(-10., 3.)),
            ("shoulder-left", DVec2::new(-8., 0.)),
        ] {
            g.set_step(&point(panel, edge), step * MM).unwrap();
        }
    }
    t.grading = Some(g);
    t
}

fn size_id(t: &GarmentTemplate, name: &str) -> String {
    let g = &t.grading.as_ref().unwrap().groups[0];
    g.sizes.iter().find(|s| s.name == name).unwrap().id.clone()
}

#[test]
fn step_and_per_size_rules_grow_from_the_base() {
    let mut t = graded_tshirt();
    let (s, l, xl) = (size_id(&t, "S"), size_id(&t, "L"), size_id(&t, "XL"));
    let g = t.grading.as_ref().unwrap();
    let group = &g.groups[0];
    let rules = g.panels.get("front");
    let base = polygon(&t, "front").clone();
    let hem = index(&t, "front", "hem");
    let at = |size: &str| point_deltas(&base, rules, group, size).unwrap()[hem];
    assert_eq!(at(&group.base), DVec2::ZERO);
    assert!((at(&l) - DVec2::new(10., 15.) * MM).length() < 1e-12);
    assert!((at(&xl) - DVec2::new(20., 30.) * MM).length() < 1e-12);
    assert!((at(&s) - DVec2::new(-10., -15.) * MM).length() < 1e-12);
    // Editing one size writes the step out size by size, leaving the others where they were.
    let g = t.grading.as_mut().unwrap();
    g.set_delta(&point("front", "hem"), &xl, DVec2::new(25., 30.) * MM)
        .unwrap();
    let rule = g.rule(&point("front", "hem")).unwrap();
    assert!(rule.step.is_none());
    assert_eq!(rule.deltas[&l], DVec2::new(10., 15.) * MM);
    assert_eq!(rule.deltas[&xl], DVec2::new(25., 30.) * MM);
    // The base size never grows.
    let base_id = g.groups[0].base.clone();
    assert!(
        g.set_delta(&point("front", "hem"), &base_id, DVec2::X)
            .is_err()
    );
}

#[test]
fn proportional_points_follow_their_neighbours_by_length() {
    let mut t = graded_tshirt();
    let l = size_id(&t, "L");
    let g = t.grading.as_mut().unwrap();
    // The hem's start becomes a proportional point between side-right's start and side-left's.
    g.set_grade_point("front", "hem", false);
    assert!(!g.is_grade_point("front", "hem"));
    let group = g.groups[0].clone();
    let base = polygon(&t, "front").clone();
    let deltas = point_deltas(
        &base,
        t.grading.as_ref().unwrap().panels.get("front"),
        &group,
        &l,
    )
    .unwrap();
    let (a, h, b) = (
        index(&t, "front", "side-right"),
        index(&t, "front", "hem"),
        index(&t, "front", "side-left"),
    );
    let len = |i: usize| polyline_length(&base.edge_points(i).unwrap());
    let share = len(a) / (len(a) + len(h));
    let expected = deltas[a] * (1. - share) + deltas[b] * share;
    assert!((deltas[h] - expected).length() < 1e-12, "{:?}", deltas[h]);
    // Making it a grade point again gives it its own (now empty) rule.
    t.grading
        .as_mut()
        .unwrap()
        .set_grade_point("front", "hem", true);
    let deltas = point_deltas(
        &base,
        t.grading.as_ref().unwrap().panels.get("front"),
        &group,
        &l,
    )
    .unwrap();
    assert_eq!(deltas[h], DVec2::ZERO);
}

#[test]
fn a_curve_rule_moves_the_curve_middle_by_its_growth() {
    let mut t = graded_tshirt();
    let l = size_id(&t, "L");
    let i = index(&t, "front", "side-right");
    {
        let poly = t.panels[0].polygon.as_mut().unwrap();
        let (u, h) = (poly.points[i], poly.points[i + 1]);
        poly.curves.insert(
            "side-right".into(),
            [
                u + (h - u) / 3. - DVec2::new(0.015, 0.),
                u + (h - u) * 2. / 3. - DVec2::new(0.015, 0.),
            ],
        );
    }
    let bulge = DVec2::new(-4., 1.) * MM;
    let g = t.grading.as_mut().unwrap();
    g.set_delta(
        &Target::Curve {
            panel: "front".into(),
            edge: "side-right".into(),
        },
        &l,
        bulge,
    )
    .unwrap();
    let group = g.groups[0].clone();
    let base = polygon(&t, "front").clone();
    let graded = grade_polygon(
        &base,
        t.grading.as_ref().unwrap().panels.get("front"),
        &group,
        &l,
    )
    .unwrap();
    assert!(graded.problem.is_none(), "{:?}", graded.problem);
    let cubic = |p: &fashioncraft::garment::polygon::PolygonPattern| {
        let [c, d] = p.curves["side-right"];
        [p.points[i], c, d, p.points[i + 1]]
    };
    let d = point_deltas(
        &base,
        t.grading.as_ref().unwrap().panels.get("front"),
        &group,
        &l,
    )
    .unwrap();
    let mid = curves::cubic(cubic(&graded.polygon), 0.5) - curves::cubic(cubic(&base), 0.5);
    let expected = (d[i] + d[i + 1]) * 0.5 + bulge;
    assert!((mid - expected).length() < 1e-12, "{mid:?} vs {expected:?}");
}

#[test]
fn notches_follow_their_boundary_by_share_or_distance() {
    let mut t = graded_tshirt();
    let l = size_id(&t, "L");
    t.panels[0].polygon.as_mut().unwrap().notches.insert(
        "side-right".into(),
        vec![
            Notch::new(0.25, 0.005, NotchKind::Slit),
            Notch::new(0.5, 0.005, NotchKind::Slit),
            Notch::new(0.75, 0.005, NotchKind::V),
        ],
    );
    let g = t.grading.as_mut().unwrap();
    let rule = |at, mode, extra: Option<(&str, f64)>| NotchRule {
        edge: "side-right".into(),
        at,
        mode,
        deltas: extra
            .map(|(s, v)| BTreeMap::from([(s.to_string(), v)]))
            .unwrap_or_default(),
    };
    g.set_notch_rule("front", rule(0.25, NotchMode::FromStart, None))
        .unwrap();
    g.set_notch_rule("front", rule(0.75, NotchMode::FromEnd, Some((&l, 2. * MM))))
        .unwrap();
    let group = g.groups[0].clone();
    let base = polygon(&t, "front").clone();
    let rules = t.grading.as_ref().unwrap().panels.get("front");
    let graded = grade_polygon(&base, rules, &group, &l).unwrap();
    let len = |p: &fashioncraft::garment::polygon::PolygonPattern| {
        polyline_length(&p.closed_boundary("side-right").unwrap())
    };
    let (b, n) = (len(&base), len(&graded.polygon));
    assert!((b - n).abs() > 5. * MM, "the side seam grows");
    let at: Vec<f64> = graded.polygon.notches["side-right"]
        .iter()
        .map(|n| n.at)
        .collect();
    // Fixed from the start: the same distance from the start.
    assert!((at[0] * n - 0.25 * b).abs() < 1e-12);
    // Proportional: the same share.
    assert!((at[1] - 0.5).abs() < 1e-12);
    // Fixed from the end, 2 mm further from it in L.
    assert!(((1. - at[2]) * n - (0.25 * b + 2. * MM)).abs() < 1e-12);
    // A notch pushed off its boundary makes the size unusable rather than vanishing.
    let g = t.grading.as_mut().unwrap();
    g.set_notch_rule(
        "front",
        rule(0.75, NotchMode::FromEnd, Some((&l, 900. * MM))),
    )
    .unwrap();
    let rules = t.grading.as_ref().unwrap().panels.get("front");
    assert!(
        grade_polygon(&base, rules, &group, &l)
            .unwrap()
            .problem
            .is_some()
    );
}

#[test]
fn seams_graphics_and_the_inside_follow_each_size() {
    let mut t = graded_tshirt();
    let (l, s) = (size_id(&t, "L"), size_id(&t, "S"));
    t.graphics.push(GraphicPlacement {
        id: "logo".into(),
        panel_id: "front".into(),
        asset_id: "asset".into(),
        center_m: DVec2::new(0.62, 0.30),
        size_m: DVec2::new(0.08, 0.05),
        rotation_degrees: 0.,
        side: GraphicSide::Front,
        opacity: 1.,
    });
    let mut rule = fashioncraft::garment::grading::GraphicRule::default();
    rule.scales.insert(l.clone(), 1.25);
    rule.offsets.insert(l.clone(), DVec2::new(0., -3. * MM));
    t.grading
        .as_mut()
        .unwrap()
        .set_graphic_rule("logo", rule)
        .unwrap();
    let big = template_in_size(&t, &t.grading.as_ref().unwrap().groups[0].id, &l).unwrap();
    assert!(big.grading.is_none());
    let logo = &big.graphics[0];
    assert!((logo.size_m - DVec2::new(0.1, 0.0625)).length() < 1e-12);
    // The centre sits right of the middle, so it follows the right side outward.
    assert!(logo.center_m.x > 0.62 + 2. * MM, "{:?}", logo.center_m);
    // Seams keep their boundaries; both sides grow alike here, so none needs easing.
    assert_eq!(big.seams.len(), t.seams.len());
    assert!(big.seams.iter().all(|s| !s.allow_ease));
    big.compile().expect("size L compiles");
    // A rule only the front follows makes the side seam ease in that size, not fail.
    let g = t.grading.as_mut().unwrap();
    g.set_delta(&point("front", "hem"), &s, DVec2::new(-14., -15.) * MM)
        .unwrap();
    let small = template_in_size(&t, &t.grading.as_ref().unwrap().groups[0].id, &s).unwrap();
    assert!(small.seams.iter().any(|s| s.allow_ease));
    small.compile().expect("size S compiles");
    let group = t.grading.as_ref().unwrap().groups[0].clone();
    let diffs = apply::seam_differences(&t, &group, &s, 0.1 * MM).unwrap();
    assert!(
        diffs
            .iter()
            .any(|(id, _, _)| id == "side-right" || id == "hem"),
        "{diffs:?}"
    );
}

#[test]
fn the_size_on_show_is_what_compiles() {
    let mut t = graded_tshirt();
    let xl = size_id(&t, "XL");
    let width = |g: &fashioncraft::garment::CompiledGarment| {
        let (lo, hi) = g
            .mesh
            .uv
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.x), hi.max(p.x))
            });
        hi - lo
    };
    let base = t.compile().unwrap();
    let group = t.grading.as_ref().unwrap().groups[0].id.clone();
    t.grading.as_mut().unwrap().set_active(&group, &xl).unwrap();
    let big = t.compile().unwrap();
    // Two front pieces side by side in uv: each 20 mm per size wider, two sizes up.
    assert!(
        width(&big) > width(&base) + 39. * MM,
        "{} {}",
        width(&big),
        width(&base)
    );
    // Showing another size changes the physics; a rule change at the base size does not.
    let mut shown = t.clone();
    shown
        .grading
        .as_mut()
        .unwrap()
        .set_active(&group, "s2")
        .unwrap();
    assert!(!fashioncraft::authoring::same_physics(&t, &shown));
    let mut edited = shown.clone();
    edited
        .grading
        .as_mut()
        .unwrap()
        .set_delta(&point("front", "hem"), &xl, DVec2::new(1., 1.) * MM)
        .unwrap();
    assert!(fashioncraft::authoring::same_physics(&shown, &edited));
    assert!(fashioncraft::authoring::only_appearance_changed(
        &shown, &edited
    ));
}

#[test]
fn sizes_are_added_inserted_renamed_and_hidden() {
    let mut t = graded_tshirt();
    let (m, l) = (size_id(&t, "M"), size_id(&t, "L"));
    let g = t.grading.as_mut().unwrap();
    let group = g.groups[0].id.clone();
    // Per-size rules interpolate a size put in between.
    g.set_delta(&point("front", "neck-left"), &l, DVec2::new(2., 4.) * MM)
        .unwrap();
    let ml = g.add_size(&group, "ML", Some(&m)).unwrap();
    let names: Vec<&str> = g.groups[0].sizes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["S", "M", "ML", "L", "XL"]);
    let rule = g.rule(&point("front", "neck-left")).unwrap();
    assert_eq!(rule.deltas[&ml], DVec2::new(1., 2.) * MM);
    // A stepped rule is written out, so L keeps its growth.
    let hem = g.rule(&point("front", "hem")).unwrap().clone();
    assert!(hem.step.is_none());
    assert_eq!(hem.deltas[&l], DVec2::new(10., 15.) * MM);
    assert_eq!(hem.deltas[&ml], DVec2::new(5., 7.5) * MM);
    // Appending carries a step on.
    let xxl = g.add_size(&group, "XXL", None).unwrap();
    assert_eq!(g.groups[0].sizes.last().unwrap().id, xxl);
    let group_def = g.groups[0].clone();
    let side = g.rule(&point("back", "side-right")).unwrap();
    assert!(side.step.is_none());
    assert_eq!(side.delta(&group_def, &xxl), DVec2::new(30., 18.) * MM);
    g.rename_size(&group, &ml, "M/L").unwrap();
    assert!(
        g.rename_size(&group, &ml, "l").is_err(),
        "names stay unique"
    );
    g.set_hidden(&group, &xxl, true).unwrap();
    assert!(
        g.set_hidden(&group, &m, true).is_err(),
        "the base stays shown"
    );
    assert!(
        g.set_active(&group, &xxl).is_err(),
        "a hidden size is not shown"
    );
    g.remove_size(&group, &xxl).unwrap();
    assert!(g.remove_size(&group, &m).is_err());
    g.validate().unwrap();
}

#[test]
fn points_of_measure_read_every_size() {
    let mut t = graded_tshirt();
    let g = t.grading.as_mut().unwrap();
    g.add_measurement(
        "Chest width",
        pom::distance("front", "side-right", "sleeve-under-left"),
        1. * MM,
    )
    .unwrap();
    g.add_measurement("Side seam", pom::length(&[("front", "side-right")]), 0.)
        .unwrap();
    g.add_measurement(
        "Hem all round",
        pom::length(&[("front", "hem"), ("back", "hem")]),
        2. * MM,
    )
    .unwrap();
    let rows = pom::table(&t);
    assert_eq!(rows.len(), 3);
    let l = size_id(&t, "L");
    let chest = &rows[0];
    // 10 mm each side per size.
    assert!((chest.difference(&l).unwrap() - 20. * MM).abs() < 1e-9);
    let s = size_id(&t, "S");
    assert!((chest.difference(&s).unwrap() + 20. * MM).abs() < 1e-9);
    let hem = &rows[2];
    assert!((hem.difference(&l).unwrap() - 40. * MM).abs() < 1e-9);
    assert_eq!(hem.tolerance_m, 2. * MM);
    let csv = pom::csv(&t);
    assert!(csv.contains("\"Chest width\""), "{csv}");
    assert!(csv.contains("\"M (base)\""), "{csv}");
    // Persisted with the garment.
    let json = serde_json::to_string(&t).unwrap();
    let back: GarmentTemplate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.grading, t.grading);
}

#[test]
fn grading_saves_and_old_files_load_unchanged() {
    let plain = tshirt(&TshirtMeasurements::default(), 0).unwrap();
    let json = serde_json::to_string(&plain).unwrap();
    assert!(!json.contains("grading"));
    let back: GarmentTemplate = serde_json::from_str(&json).unwrap();
    assert!(back.grading.is_none());
    let mut t = graded_tshirt();
    let group = t.grading.as_ref().unwrap().groups[0].id.clone();
    let l = size_id(&t, "L");
    t.grading.as_mut().unwrap().set_active(&group, &l).unwrap();
    let json = serde_json::to_string_pretty(&t).unwrap();
    let back: GarmentTemplate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.grading, t.grading);
    assert_eq!(back.grading.as_ref().unwrap().groups[0].active(), l);
}

#[test]
fn undo_takes_back_rules_but_not_the_size_on_show() {
    let t0 = graded_tshirt();
    let (group, l) = (
        t0.grading.as_ref().unwrap().groups[0].id.clone(),
        size_id(&t0, "L"),
    );
    let mut history = History::default();
    let mut t1 = t0.clone();
    t1.grading
        .as_mut()
        .unwrap()
        .set_delta(&point("front", "hem"), &l, DVec2::new(40., 15.) * MM)
        .unwrap();
    history.record_named(t0.clone(), "Grade rule");
    // Showing L afterwards is not an edit.
    let mut current = t1.clone();
    current
        .grading
        .as_mut()
        .unwrap()
        .set_active(&group, &l)
        .unwrap();
    let undone = history.restore(&current, false).unwrap();
    let g = undone.template.grading.as_ref().unwrap();
    assert_eq!(
        g.rule(&point("front", "hem")).unwrap().step,
        Some(DVec2::new(10., 15.) * MM)
    );
    assert_eq!(g.groups[0].active(), l, "undo keeps the size on show");
    assert!(undone.reset_simulation, "L's hem moved back");
    let redone = history.restore(&undone.template, true).unwrap();
    let g = redone.template.grading.as_ref().unwrap();
    assert_eq!(
        g.rule(&point("front", "hem")).unwrap().deltas[&l],
        DVec2::new(40., 15.) * MM
    );
}

#[test]
fn field_sums_are_evaluated() {
    assert_eq!(eval_mm("10+2"), Some(12.));
    assert_eq!(eval_mm(" 12,5 - 3*2 "), Some(6.5));
    assert_eq!(eval_mm("(4+2)/-2"), Some(-3.));
    assert_eq!(eval_mm("1/0"), None);
    assert_eq!(eval_mm("4+"), None);
    assert_eq!(eval_mm("abc"), None);
}

#[test]
fn a_rule_table_reads_back_what_was_written() {
    let table = rul::RuleTable {
        name: "TEST".into(),
        units: Units::Millimetres,
        sizes: vec![
            "34".into(),
            "36".into(),
            "38".into(),
            "40".into(),
            "42".into(),
        ],
        sample: "36".into(),
        author: "FashionCraft;FashionCraft;test".into(),
        created: Some(("22-11-2026".into(), "10:00".into())),
        version: rul::ASTM_VERSION.into(),
        rules: BTreeMap::from([
            (1, vec![DVec2::ZERO; 5]),
            (
                7,
                vec![
                    DVec2::new(-5.5, 1.25) * MM,
                    DVec2::ZERO,
                    DVec2::new(5.5, -1.25) * MM,
                    DVec2::new(11., -2.5) * MM,
                    DVec2::new(16.5, -3.75) * MM,
                ],
            ),
        ]),
        notes: vec![],
    };
    let text = rul::write(&table).unwrap();
    assert!(
        text.contains(
            "RULE: DELTA 7\n-5.50, 1.25 0.00, 0.00 5.50, -1.25 11.00, -2.50\n16.50, -3.75\n"
        ),
        "{text}"
    );
    assert!(text.trim_end().ends_with("END"));
    let back = rul::read(&text).unwrap();
    assert_eq!(back.sizes, table.sizes);
    assert_eq!(back.sample, "36");
    assert_eq!(back.version, rul::ASTM_VERSION);
    for (id, growth) in &table.rules {
        for (a, b) in growth.iter().zip(&back.rules[id]) {
            assert!(a.distance(*b) < 1e-9);
        }
    }
    // English units, inches with 4 decimals.
    let inch = rul::RuleTable {
        units: Units::Inches,
        ..table.clone()
    };
    let back = rul::read(&rul::write(&inch).unwrap()).unwrap();
    assert!((back.rules[&7][4].x - 16.5 * MM).abs() < 0.0000254);
}

#[test]
fn a_hand_written_rule_table_is_read() {
    let text = std::fs::read_to_string("tests/fixtures/grading/handmade.rul").unwrap();
    let table = rul::read(&text).unwrap();
    assert_eq!(table.units, Units::Millimetres);
    assert_eq!(table.sizes, ["XS", "S", "M", "L"]);
    assert_eq!(table.sample, "S");
    assert_eq!(table.rules.len(), 3);
    // Values spread over lines and separated by tabs and commas.
    assert_eq!(table.rules[&2][3], DVec2::new(12.5, -4.0) * MM);
    // A sample growth that is not zero is measured from the sample instead.
    assert_eq!(table.rules[&-3][1], DVec2::ZERO);
    assert_eq!(table.rules[&-3][0], DVec2::new(-1.0, 0.) * MM);
    assert!(
        table.notes.iter().any(|n| n.contains("sample")),
        "{:?}",
        table.notes
    );
    assert!(rul::read("UNITS: METRIC\nSIZE LIST: A B\nSAMPLE SIZE: C\nEND\n").is_err());
    assert!(
        rul::read("UNITS: METRIC\nSIZE LIST: A B\nSAMPLE SIZE: A\nRULE: DELTA 1\n0,0\nEND\n")
            .is_err()
    );
}

/// The T-shirt graded with allowances, notches (one fixed from the end) and a curved side.
fn production_tshirt() -> GarmentTemplate {
    let mut t = graded_tshirt();
    let xl = size_id(&t, "XL");
    for panel in &mut t.panels {
        let poly = panel.polygon.as_mut().unwrap();
        for edge in poly.edges.clone() {
            let width = if edge == "hem" { 0.025 } else { 0.01 };
            poly.seam_allowances.insert(edge, width);
        }
        let i = poly.edges.iter().position(|e| e == "side-right").unwrap();
        let (u, h) = (poly.points[i], poly.points[i + 1]);
        poly.curves.insert(
            "side-right".into(),
            [
                u + (h - u) / 3. - DVec2::new(0.012, 0.),
                u + (h - u) * 2. / 3. - DVec2::new(0.012, 0.),
            ],
        );
        poly.notches.insert(
            "side-right".into(),
            vec![
                Notch::new(0.3, 0.005, NotchKind::Slit),
                Notch::new(0.8, 0.005, NotchKind::V),
            ],
        );
        poly.notches
            .insert("hem".into(), vec![Notch::new(0.5, 0.005, NotchKind::Slit)]);
    }
    let g = t.grading.as_mut().unwrap();
    for panel in ["front", "back"] {
        g.set_notch_rule(
            panel,
            NotchRule {
                edge: "side-right".into(),
                at: 0.8,
                mode: NotchMode::FromEnd,
                deltas: BTreeMap::new(),
            },
        )
        .unwrap();
        g.set_delta(
            &Target::Curve {
                panel: panel.into(),
                edge: "side-right".into(),
            },
            &xl,
            DVec2::new(-3., 0.) * MM,
        )
        .unwrap();
        // The neck's middle follows its ends.
        g.set_grade_point(panel, "neck-bottom", false);
    }
    t
}

fn outline(p: &fashioncraft::garment::polygon::PolygonPattern) -> Vec<DVec2> {
    p.outer_points().unwrap()
}

/// Largest distance from any point of `a` to polyline ring `b`.
fn gap(a: &[DVec2], b: &[DVec2]) -> f64 {
    a.iter()
        .map(|x| {
            (0..b.len())
                .map(|i| {
                    let (p, q) = (b[i], b[(i + 1) % b.len()]);
                    let t = ((*x - p).dot(q - p) / p.distance_squared(q)).clamp(0., 1.);
                    (p + (q - p) * t).distance(*x)
                })
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0., f64::max)
}

#[test]
fn dxf_with_its_rule_table_comes_back_graded() {
    let t = production_tshirt();
    let export = dxf::export(
        &t,
        &PieceOptions::default(),
        Units::Millimetres,
        Dialect::Astm,
    )
    .unwrap();
    let table = export
        .table
        .clone()
        .expect("a graded garment writes a rule table");
    assert_eq!(table.sizes, ["S", "M", "L", "XL"]);
    assert_eq!(table.sample, "M");
    let mut pieces = export.pieces.clone();
    assert!(pieces.iter().all(|p| p.size == "M"));
    assert!(
        pieces
            .iter()
            .all(|p| !p.grade_points.is_empty() && p.grade_reference.is_some())
    );
    let bytes = dxf_aama::write(
        &pieces,
        &WriteOptions {
            dialect: Dialect::Astm,
            units: Units::Millimetres,
            style: t.name.clone(),
            sample_size: table.sample.clone(),
            author: "test".into(),
            created: None,
            grade_rule_table: table.name.clone(),
        },
    )
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains(&format!("Grade Rule Table: {}", table.name)));
    assert!(text.contains("# 1"));
    let rul_text = rul::write(&table).unwrap();
    let file = dxf_aama::read(&text, ReadOptions::default()).unwrap();
    let back_table = rul::read(&rul_text).unwrap();
    for (a, b) in file.pieces.iter().zip(&mut pieces) {
        assert_eq!(a.grade_points.len(), b.grade_points.len());
        b.grade_points.clear();
    }
    // Import both pieces into an empty-ish garment and attach the rules.
    let mut target = tshirt(&TshirtMeasurements::default(), 0).unwrap();
    target.panels.clear();
    target.seams.clear();
    let fabric = target.fabrics[0].id.clone();
    let selected: Vec<usize> = (0..file.pieces.len()).collect();
    let edit = dxf_aama::template::prepare(
        &target,
        &file,
        &selected,
        &fabric,
        Default::default(),
        glam::DVec3::ZERO,
    )
    .unwrap();
    let mut imported = edit.template;
    let ids: Vec<String> = imported.panels.iter().map(|p| p.id.clone()).collect();
    let notes = dxf::attach(&mut imported, &file, &selected, &ids, &back_table).unwrap();
    let g = imported.grading.as_ref().expect("grading came in");
    let names: Vec<&str> = g.groups[0].sizes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["S", "M", "L", "XL"], "{notes:?}");
    assert_eq!(g.groups[0].base_name(), "M");
    // Every size's outline and notches match the original's, to the table's 0.01 mm.
    let group = t.grading.as_ref().unwrap().groups[0].clone();
    let back_group = g.groups[0].clone();
    let mut worst: f64 = 0.;
    let mut worst_notch: f64 = 0.;
    let mut worst_corner: f64 = 0.;
    let mut base_gap: f64 = 0.;
    for (k, panel) in ["front", "back"].iter().enumerate() {
        for (size, back_size) in group.sizes.iter().zip(&back_group.sizes) {
            let a = apply::panel_in_size(&t, panel, &size.id)
                .unwrap()
                .unwrap()
                .polygon;
            let b = apply::panel_in_size(&imported, &ids[k], &back_size.id)
                .unwrap()
                .unwrap()
                .polygon;
            // The import puts the piece's corner at the origin: compare in each one's frame.
            let lo = |p: &[DVec2]| p.iter().fold(DVec2::splat(f64::INFINITY), |m, q| m.min(*q));
            let base_a = outline(polygon(&t, panel));
            let base_b = outline(polygon(&imported, &ids[k]));
            let shift = lo(&base_a) - lo(&base_b);
            let moved: Vec<DVec2> = outline(&b).iter().map(|p| *p + shift).collect();
            let d = gap(&outline(&a), &moved).max(gap(&moved, &outline(&a)));
            if group.is_base(&size.id) {
                base_gap = base_gap.max(d);
            }
            for (p, q) in a.points.iter().zip(&b.points) {
                worst_corner = worst_corner.max(p.distance(*q + shift));
            }
            worst = worst.max(d);
            for (edge, list) in &a.notches {
                let boundary = a.closed_boundary(edge).unwrap();
                for notch in list {
                    let p = curves::arclength_point(&boundary, notch.at);
                    let nearest = b
                        .notches
                        .iter()
                        .flat_map(|(e, l)| {
                            let bb = b.closed_boundary(e).unwrap();
                            l.iter()
                                .map(move |n| curves::arclength_point(&bb, n.at) + shift)
                                .collect::<Vec<_>>()
                        })
                        .map(|q| q.distance(p))
                        .fold(f64::INFINITY, f64::min);
                    worst_notch = worst_notch.max(nearest);
                }
            }
        }
    }
    println!(
        "worst corner {:.4} mm, worst outline gap {:.4} mm (base size alone {:.4} mm), worst notch {:.4} mm",
        worst_corner / MM,
        worst / MM,
        base_gap / MM,
        worst_notch / MM
    );
    // Corners carry their rules exactly, to the table's 0.01 mm.
    assert!(
        worst_corner < 0.01 * MM,
        "a corner moved {} mm; {notes:?}",
        worst_corner / MM
    );
    // The importer refits curves (0.14 mm here in the base size); a curve rule then bows
    // the refitted curve, parameterised a little differently: 0.34 mm in XL, whose side
    // bows 3 mm.
    assert!(
        worst < base_gap + 0.25 * MM,
        "outline differs by {} mm; {notes:?}",
        worst / MM
    );
    assert!(
        worst_notch < 0.25 * MM,
        "notch moved by {} mm",
        worst_notch / MM
    );
}

#[test]
fn export_refuses_pieces_of_two_size_groups() {
    let mut t = graded_tshirt();
    let g = t.grading.as_mut().unwrap();
    let petite = g.add_group("Petite", &["PS", "PM"], 1).unwrap();
    g.assign(&["back".into()], &petite).unwrap();
    assert!(
        dxf::export(
            &t,
            &PieceOptions::default(),
            Units::Millimetres,
            Dialect::Aama
        )
        .is_err()
    );
    let front_only = PieceOptions {
        panels: Some(["front".to_string()].into()),
        ..Default::default()
    };
    let export = dxf::export(&t, &front_only, Units::Millimetres, Dialect::Aama).unwrap();
    assert_eq!(export.table.unwrap().sizes.len(), 4);
    // Each group shows its own size; the back follows Petite.
    let id = petite.clone();
    let g = t.grading.as_mut().unwrap();
    g.set_active(&id, "s1").unwrap();
    assert_eq!(g.active_size("back"), Some("s1"));
    assert_eq!(g.active_size("front"), None);
}
