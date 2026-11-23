//! DXF-AAMA and DXF-ASTM: a sample garment goes out in both dialects and comes back with its
//! pieces, boundary lengths (under 0.1 mm apart), seam allowances, notches and grain; files
//! written by hand from the public descriptions read as they should, unsupported entities
//! listed rather than dropped.
use fashioncraft::garment::{
    GarmentTemplate,
    domain::InternalLine,
    dxf_aama::{
        Dialect, NotchShape, PatternFile, Quantity, ReadOptions, Units, WriteOptions, detect, read,
        template::{self, PieceOptions},
        write,
    },
    markings::{Notch, NotchKind, polyline_length},
    meshing::MeshingSettings,
    polygon::PolygonPattern,
    presets::{TshirtMeasurements, tshirt},
};
use glam::{DVec2, DVec3};
use std::{collections::BTreeMap, path::PathBuf};

/// A T-shirt with a scooped neckline and shaped sides, its allowances, slit and V notches,
/// and a bias-cut pocket with a placement line.
fn sample() -> GarmentTemplate {
    let mut t = tshirt(&TshirtMeasurements::default(), 0).unwrap();
    t.name = "Sample T-shirt".into();
    for (panel, name) in t.panels.iter_mut().zip(["Front", "Back"]) {
        panel.name = name.into();
        let poly = panel.polygon.as_mut().unwrap();
        let p = poly.points.clone();
        let (a, b) = (p[0], p[1]);
        poly.curves.insert(
            "neck-left".into(),
            [
                a + DVec2::new(0., (b.y - a.y) * 0.6),
                b - DVec2::new((b.x - a.x) * 0.5, 0.),
            ],
        );
        let (c, d) = (p[2], p[3]);
        poly.curves.insert(
            "neck-right".into(),
            [
                c + DVec2::new((d.x - c.x) * 0.5, 0.),
                d + DVec2::new(0., (c.y - d.y) * 0.6),
            ],
        );
        let (u, h) = (p[7], p[8]);
        poly.curves.insert(
            "side-right".into(),
            [
                u + (h - u) / 3. - DVec2::new(0.015, 0.),
                u + (h - u) * 2. / 3. - DVec2::new(0.015, 0.),
            ],
        );
        let (h, u) = (p[9], p[10]);
        poly.curves.insert(
            "side-left".into(),
            [
                h + (u - h) / 3. + DVec2::new(0.015, 0.),
                h + (u - h) * 2. / 3. + DVec2::new(0.015, 0.),
            ],
        );
        for edge in poly.edges.clone() {
            let width = match edge.as_str() {
                "hem" => 0.025,
                "neck-left" | "neck-bottom" | "neck-right" => 0.006,
                "cuff-left" | "cuff-right" => 0.02,
                _ => 0.01,
            };
            poly.seam_allowances.insert(edge, width);
        }
        let notch = |at, length, kind| Notch::new(at, length, kind);
        poly.notches = BTreeMap::from([
            ("side-right".into(), vec![notch(0.4, 0.005, NotchKind::V)]),
            ("side-left".into(), vec![notch(0.6, 0.005, NotchKind::V)]),
            (
                "shoulder-right".into(),
                vec![notch(0.5, 0.005, NotchKind::Slit)],
            ),
            (
                "shoulder-left".into(),
                vec![notch(0.5, 0.005, NotchKind::Slit)],
            ),
            (
                "neck-bottom".into(),
                vec![notch(0.5, 0.004, NotchKind::Slit)],
            ),
            (
                "hem".into(),
                vec![
                    notch(0.25, 0.005, NotchKind::Slit),
                    notch(0.5, 0.005, NotchKind::V),
                ],
            ),
        ]);
    }
    let mut pocket = t.panels[0].clone();
    pocket.id = "pocket".into();
    pocket.name = "Pocket".into();
    pocket.grain_degrees = 45.;
    pocket.pattern.width = 0.14;
    pocket.pattern.height = 0.16;
    pocket.placement.translation = DVec3::new(1.5, 1.2, 0.3);
    let edges = ["top", "right", "bottom-right", "bottom-left", "left"];
    pocket.polygon = Some(PolygonPattern {
        points: vec![
            DVec2::new(0., 0.),
            DVec2::new(0.14, 0.),
            DVec2::new(0.14, 0.13),
            DVec2::new(0.07, 0.16),
            DVec2::new(0., 0.13),
        ],
        edges: edges.iter().map(|e| e.to_string()).collect(),
        subdivisions: 0,
        meshing: t.panels[0].polygon.as_ref().unwrap().meshing.clone(),
        curves: BTreeMap::new(),
        holes: vec![],
        lines: vec![InternalLine {
            id: "placement".into(),
            points: vec![DVec2::new(0.02, 0.04), DVec2::new(0.12, 0.04)],
            tangents: vec![DVec2::ZERO; 2],
            closed: false,
            group: None,
        }],
        target_lengths: BTreeMap::new(),
        seam_allowances: edges
            .iter()
            .map(|e| (e.to_string(), if *e == "top" { 0.025 } else { 0.01 }))
            .collect(),
        notches: BTreeMap::from([
            (
                "right".into(),
                vec![Notch::new(0.025 / 0.13, 0.005, NotchKind::Slit)],
            ),
            (
                "left".into(),
                vec![Notch::new(1. - 0.025 / 0.13, 0.005, NotchKind::Slit)],
            ),
        ]),
    });
    t.panels.push(pocket);
    t.compile().unwrap();
    t
}

fn options() -> PieceOptions {
    PieceOptions {
        size: "M".into(),
        quantities: BTreeMap::from([("pocket".into(), Quantity { right: 2, left: 0 })]),
        panels: None,
    }
}

fn write_options(dialect: Dialect, units: Units) -> WriteOptions {
    WriteOptions {
        dialect,
        units,
        style: "Sample T-shirt".into(),
        sample_size: "M".into(),
        author: "FashionCraft;FashionCraft;test".into(),
        created: None,
        grade_rule_table: String::new(),
    }
}

fn export(t: &GarmentTemplate, dialect: Dialect, units: Units) -> String {
    let pieces = template::pieces(t, &options()).unwrap();
    let bytes = write(&pieces, &write_options(dialect, units)).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.is_ascii(), "apparel DXF text is 7-bit ASCII");
    text
}

fn bounds_min(points: &[DVec2]) -> DVec2 {
    points
        .iter()
        .fold(DVec2::splat(f64::INFINITY), |lo, p| lo.min(*p))
}

/// The original panel against the piece read back: the same boundaries, each as long as
/// before, with the same allowance and notches; the same grain axis; the same lines.
/// The worst differences seen, metres: boundary length, allowance, notch position.
#[derive(Default, Debug)]
struct Worst {
    length: f64,
    allowance: f64,
    notch: f64,
}

fn compare(original: &PolygonPattern, grain: f64, piece: &template::Converted, worst: &mut Worst) {
    let back = &piece.polygon;
    let lo = bounds_min(&original.outer_points().unwrap());
    assert_eq!(
        back.points.len(),
        original.points.len(),
        "every boundary comes back as one boundary"
    );
    let n = back.points.len();
    for i in 0..original.points.len() {
        let a = original.points[i] - lo;
        let b = original.points[(i + 1) % original.points.len()] - lo;
        let j = (0..n)
            .find(|j| {
                back.points[*j].distance(a) < 2e-5 && back.points[(j + 1) % n].distance(b) < 2e-5
            })
            .unwrap_or_else(|| panic!("boundary {} not found", original.edges[i]));
        let (name, other) = (&original.edges[i], &back.edges[j]);
        let length = polyline_length(&original.edge_points(i).unwrap());
        let again = polyline_length(&back.edge_points(j).unwrap());
        worst.length = worst.length.max((length - again).abs());
        worst.allowance = worst
            .allowance
            .max((original.allowance_of(name) - back.allowance_of(other)).abs());
        assert!(
            (length - again).abs() < 0.0001,
            "{name}: {:.4} mm became {:.4} mm",
            length * 1000.,
            again * 1000.
        );
        assert!(
            (original.allowance_of(name) - back.allowance_of(other)).abs() < 2e-5,
            "{name}: allowance {} became {}",
            original.allowance_of(name),
            back.allowance_of(other)
        );
        let before = original.notches.get(name).cloned().unwrap_or_default();
        let after = back.notches.get(other).cloned().unwrap_or_default();
        assert_eq!(before.len(), after.len(), "{name}: notches");
        for (x, y) in before.iter().zip(&after) {
            worst.notch = worst.notch.max((x.at * length - y.at * again).abs());
            assert!(
                (x.at * length - y.at * again).abs() < 0.0001,
                "{name}: notch moved"
            );
            assert!(
                (x.length_m - y.length_m).abs() < 1e-5,
                "{name}: notch depth"
            );
            assert_eq!(x.kind, y.kind, "{name}: notch kind");
        }
    }
    let axis = grain.rem_euclid(180.);
    let gap = (axis - piece.grain_degrees).abs();
    assert!(
        gap.min(180. - gap) <= 0.01,
        "grain {grain} became {}",
        piece.grain_degrees
    );
    assert_eq!(back.lines.len(), original.lines.len());
    for (x, y) in original.lines.iter().zip(&back.lines) {
        let (xs, ys) = (x.samples().unwrap(), y.samples().unwrap());
        assert!((xs[0] - lo).distance(ys[0]) < 2e-5);
        assert!((*xs.last().unwrap() - lo).distance(*ys.last().unwrap()) < 2e-5);
    }
}

fn round_trip(dialect: Dialect, units: Units) -> PatternFile {
    let t = sample();
    let text = export(&t, dialect, units);
    let file = read(&text, ReadOptions::default()).unwrap();
    assert_eq!(file.dialect, dialect);
    assert_eq!(file.units, Some(units));
    assert_eq!(file.units_source, "Units text");
    assert!(file.issues.is_empty(), "{:#?}", file.issues);
    assert_eq!(file.style, "Sample T-shirt");
    assert_eq!(file.sample_size, "M");
    assert_eq!(file.pieces.len(), 3);
    let mut worst = Worst::default();
    for (panel, piece) in t.panels.iter().zip(&file.pieces) {
        assert_eq!(piece.name, panel.name);
        assert_eq!(piece.size, "M");
        assert_eq!(piece.material, t.fabrics[0].name);
        let expected = if panel.id == "pocket" { 2 } else { 1 };
        assert_eq!(
            piece.quantity,
            Quantity {
                right: expected,
                left: 0
            }
        );
        assert!(piece.sew.is_some(), "the sew line is written apart");
        let converted = template::convert(piece).unwrap();
        assert!(converted.notes.is_empty(), "{:?}", converted.notes);
        compare(
            panel.polygon.as_ref().unwrap(),
            panel.grain_degrees,
            &converted,
            &mut worst,
        );
    }
    eprintln!(
        "{dialect:?} {units:?}: worst boundary length {:.4} mm, allowance {:.4} mm, notch {:.4} mm",
        worst.length * 1000.,
        worst.allowance * 1000.,
        worst.notch * 1000.
    );
    // The whole import adds the three pieces as panels that compile.
    let edit = template::prepare(
        &t,
        &file,
        &[0, 1, 2],
        &t.fabrics[0].id,
        MeshingSettings {
            target_edge_m: 0.03,
            ..Default::default()
        },
        DVec3::new(3., 1.5, 0.),
    )
    .unwrap();
    assert_eq!(edit.template.panels.len(), 6);
    file
}

#[test]
fn aama_round_trip_in_millimetres() {
    round_trip(Dialect::Aama, Units::Millimetres);
}

#[test]
fn astm_round_trip_in_millimetres() {
    round_trip(Dialect::Astm, Units::Millimetres);
}

#[test]
fn both_round_trip_in_inches() {
    round_trip(Dialect::Aama, Units::Inches);
    round_trip(Dialect::Astm, Units::Inches);
}

#[test]
fn the_dialects_differ_in_the_version_line_only_for_these_pieces() {
    let t = sample();
    let aama = export(&t, Dialect::Aama, Units::Millimetres);
    let astm = export(&t, Dialect::Astm, Units::Millimetres);
    assert!(!aama.contains("ASTM"));
    assert!(astm.contains("ASTM/D13 Proposal 1 Version: D 6673-04"));
    assert_eq!(detect(&aama), None);
    assert_eq!(detect(&astm), Some(Dialect::Astm));
    for text in [&aama, &astm] {
        assert!(text.contains("Units: METRIC"));
        assert!(text.contains("Piece Name: Front"));
        assert!(text.contains("Quantity: 2,0"));
        assert!(text.contains("AC1009"));
    }
    // V notches carry their opening; slits none.
    let file = read(&astm, ReadOptions::default()).unwrap();
    let front = &file.pieces[0];
    let v = front
        .notches
        .iter()
        .filter(|n| n.shape == NotchShape::V)
        .count();
    let slit = front
        .notches
        .iter()
        .filter(|n| n.shape == NotchShape::Slit)
        .count();
    assert_eq!((v, slit), (3, 4));
}

#[test]
fn accents_are_written_without_them_and_names_stay_unique() {
    let mut t = sample();
    t.panels[0].name = "Delantero puño".into();
    t.panels[1].name = "Delantero puño".into();
    let text = export(&t, Dialect::Aama, Units::Millimetres);
    let file = read(&text, ReadOptions::default()).unwrap();
    assert_eq!(file.pieces[0].name, "Delantero puno");
    assert_eq!(file.pieces[1].name, "Delantero puno 2");
}

#[test]
fn a_file_without_units_asks_for_a_scale() {
    let text = export(&sample(), Dialect::Aama, Units::Millimetres)
        .replace("Units: METRIC", "Note: none")
        .replace("$INSUNITS\n 70\n4", "$INSUNITS\n 70\n0");
    let error = read(&text, ReadOptions::default()).unwrap_err();
    assert!(error.contains("units"), "{error}");
    let file = read(
        &text,
        ReadOptions {
            metres_per_unit: Some(0.001),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(file.units_source, "chosen scale");
}

#[test]
fn a_plain_drawing_is_not_taken_for_a_pattern() {
    let plain =
        "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDSEC\n0\nEOF\n";
    let error = read(plain, ReadOptions::default()).unwrap_err();
    assert!(error.contains("No pattern pieces"), "{error}");
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dxf-aama")
        .join(name)
}

fn load(name: &str) -> PatternFile {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    read(&text, ReadOptions::default()).unwrap()
}

fn reasons(file: &PatternFile) -> Vec<String> {
    file.issues
        .iter()
        .map(|i| {
            format!(
                "{} | {} | {} | {} | {}",
                i.piece, i.layer, i.entity, i.reason, i.count
            )
        })
        .collect()
}

/// Written by hand from the public descriptions, not by FashionCraft: an AAMA file in
/// millimetres with an arc on the outline, notches as points and as a line, and entities
/// FashionCraft does not take, which must be listed.
#[test]
fn handmade_aama_file() {
    let file = load("handmade-aama.dxf");
    assert_eq!(file.dialect, Dialect::Aama);
    assert_eq!(
        file.dialect_reason,
        "no ASTM version text or ASTM-only layers"
    );
    assert_eq!(file.units, Some(Units::Millimetres));
    assert_eq!(file.style, "Camisa prueba");
    assert_eq!(file.pieces.len(), 2);

    let front = &file.pieces[0];
    assert_eq!(front.name, "Delantero");
    assert_eq!(front.size, "M", "the sample size when the piece names none");
    assert_eq!(front.quantity, Quantity { right: 1, left: 1 });
    assert_eq!(front.material, "Denim");
    assert_eq!(front.annotations, vec!["Lavar a 30 C".to_string()]);
    assert_eq!(front.drill_holes.len(), 1);
    assert_eq!(front.notches.len(), 3);
    let piece = template::convert(front).unwrap();
    let poly = &piece.polygon;
    // 200 × 300 mm sew line with a 50 mm round corner: four straight boundaries and the arc
    // as one cubic.
    assert_eq!(poly.points.len(), 5);
    assert_eq!(poly.curves.len(), 1);
    let perimeter: f64 = (0..5)
        .map(|i| polyline_length(&poly.edge_points(i).unwrap()))
        .sum();
    let expected = 0.2 + 0.25 + 0.15 + 0.3 + std::f64::consts::FRAC_PI_2 * 0.05;
    assert!((perimeter - expected).abs() < 0.0002, "{perimeter}");
    for width in poly.seam_allowances.values() {
        assert!((width - 0.01).abs() < 2e-5, "{width}");
    }
    assert_eq!(poly.seam_allowances.len(), 5);
    let mut kinds: Vec<(f64, NotchKind)> = poly
        .notches
        .values()
        .flatten()
        .map(|n| ((n.length_m * 1e4).round() / 10., n.kind))
        .collect();
    kinds.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(
        kinds,
        vec![
            (5., NotchKind::Slit),
            (5., NotchKind::Slit),
            (6., NotchKind::V)
        ]
    );
    assert!((piece.grain_degrees - 90.).abs() < 1e-9);
    assert_eq!(poly.lines.len(), 1);
    assert!(piece.notes.iter().any(|n| n.contains("drill hole")));
    assert!(piece.notes.iter().any(|n| n.contains("Lavar a 30 C")));

    let pocket = &file.pieces[1];
    assert_eq!(pocket.name, "Bolsillo");
    assert!(pocket.sew.is_none(), "cut line only: no allowance");
    let piece = template::convert(pocket).unwrap();
    assert_eq!(
        piece.polygon.points.len(),
        4,
        "boundary drawn as four lines"
    );
    assert!(piece.polygon.seam_allowances.is_empty());
    assert!((piece.grain_degrees - 90.).abs() < 1e-9);

    let listed = reasons(&file);
    for expected in [
        "Bolsillo | 6 | LINE | Mirror (fold) lines are not imported; the piece comes in as drawn | 1",
        "Bolsillo | 8 | SPLINE | SPLINE entities are not part of the apparel layers | 1",
        "Bolsillo | CORTE | LINE | Not on a numbered AAMA/ASTM layer | 1",
    ] {
        assert!(
            listed.iter().any(|l| l == expected),
            "{expected}\n{listed:#?}"
        );
    }
    // Grade rule numbers on the turn points' layer are read as such; they grade the piece
    // with a rule table beside the file (tests/grading.rs).
    let rules: Vec<i64> = file.pieces[0].grade_points.iter().map(|g| g.rule).collect();
    assert_eq!(rules, [1, 2]);
    assert!(
        !listed.iter().any(|l| l.contains("grade rule numbers")),
        "{listed:#?}"
    );
}

/// Written by hand: an ASTM file in inches with its version line, a piece inserted turned
/// 90°, ASTM notch shapes, a cutout and a quality curve.
#[test]
fn handmade_astm_file() {
    let file = load("handmade-astm.dxf");
    assert_eq!(file.dialect, Dialect::Astm);
    assert_eq!(file.dialect_reason, "ASTM version text");
    assert_eq!(file.units, Some(Units::Inches));
    assert_eq!(file.pieces.len(), 2);

    let yoke = &file.pieces[0];
    assert_eq!(yoke.name, "Back Yoke");
    assert_eq!(yoke.size, "8");
    assert_eq!(yoke.quantity, Quantity { right: 2, left: 0 });
    let shapes: Vec<NotchShape> = yoke.notches.iter().map(|n| n.shape).collect();
    assert_eq!(
        shapes,
        vec![NotchShape::T, NotchShape::Check, NotchShape::Slit]
    );
    let piece = template::convert(yoke).unwrap();
    // 12 × 4 in sew line, inserted turned 90°: its size is swapped on the board.
    assert!(
        (piece.size.x - 4. * 0.0254).abs() < 1e-6,
        "{:?}",
        piece.size
    );
    assert!((piece.size.y - 12. * 0.0254).abs() < 1e-6);
    for width in piece.polygon.seam_allowances.values() {
        assert!((width - 0.375 * 0.0254).abs() < 2e-5, "{width}");
    }
    assert_eq!(piece.polygon.notches.values().flatten().count(), 3);
    assert!(
        piece
            .notes
            .iter()
            .any(|n| n.contains("A T notch came in as a slit"))
    );
    // Horizontal in the block, turned 90° by its insert: vertical.
    assert!(
        (piece.grain_degrees - 90.).abs() < 1e-6,
        "{}",
        piece.grain_degrees
    );
    assert!(piece.notes.iter().any(|n| n.contains("Rotation: 0")));

    let cuff = &file.pieces[1];
    assert_eq!(cuff.cutouts.len(), 1);
    let piece = template::convert(cuff).unwrap();
    assert_eq!(piece.polygon.holes.len(), 1);
    assert!(
        reasons(&file)
            .iter()
            .any(|l| l.contains("Quality validation curves are not imported"))
    );
    let t = sample();
    let edit = template::prepare(
        &t,
        &file,
        &[0, 1],
        &t.fabrics[0].id,
        MeshingSettings {
            target_edge_m: 0.02,
            ..Default::default()
        },
        DVec3::new(3., 1.5, 0.),
    )
    .unwrap();
    assert_eq!(edit.template.panels.len(), 5);
}

/// The sample garment as FashionCraft writes it, kept in the repository for workshops and
/// other CAD to open. Set `DXF_AAMA_BLESS=1` to write them again after a format change.
#[test]
fn sample_files_are_current_and_reopen() {
    let t = sample();
    for (dialect, name) in [
        (Dialect::Aama, "sample-tshirt-aama.dxf"),
        (Dialect::Astm, "sample-tshirt-astm.dxf"),
    ] {
        let text = export(&t, dialect, Units::Millimetres);
        let path = fixture(name);
        if std::env::var_os("DXF_AAMA_BLESS").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        let kept = std::fs::read_to_string(&path).unwrap();
        assert!(
            kept == text,
            "{name} is out of date: run with DXF_AAMA_BLESS=1"
        );
        let file = read(&kept, ReadOptions::default()).unwrap();
        assert_eq!(file.dialect, dialect);
        assert_eq!(file.pieces.len(), 3);
    }
}
