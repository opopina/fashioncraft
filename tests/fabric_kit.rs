//! Fabric kit: the virtual tests against analytic references, and the fit against a
//! synthetic fabric whose parameters are known.
use fashioncraft::fabric_kit::{
    self, Direction, FabricFile, FitOptions, Measurements, Reading, Stage, Test,
};
use fashioncraft::garment::template::FabricProperties;
use std::sync::atomic::AtomicBool;

/// Coarser than the window's meshes, so the tests run quickly in a debug build; every claim
/// below holds at this resolution and is checked at it.
fn options() -> FitOptions {
    FitOptions {
        strip_cells: 6,
        cantilever_cells: 16,
        ..Default::default()
    }
}

fn fabric() -> FabricProperties {
    FabricProperties {
        warp: 2400.,
        weft: 1500.,
        shear: 70.,
        bend_warp: 1.5e-5,
        bend_weft: 0.9e-5,
        density: 0.12,
        thickness: 0.00025,
        ..Default::default()
    }
}

fn reading(load_g: f64) -> Reading {
    Reading {
        load_g,
        length_mm: 0.,
    }
}

/// The procedure's geometry and loads, lengths still to read.
fn template() -> Measurements {
    Measurements {
        warp: vec![reading(500.), reading(1000.), reading(2000.)],
        weft: vec![reading(500.), reading(1000.), reading(2000.)],
        bias: vec![reading(25.), reading(100.)],
        overhang_warp_mm: Some(60.),
        overhang_weft_mm: Some(50.),
        slide_angle_degrees: Some(24.),
        ..Default::default()
    }
}

#[test]
fn virtual_strips_follow_the_constitutive_law() {
    // Warp and weft strips carry their load along one fibre axis: the gauge's strain must be
    // the law's strain at that traction, clamps and the strip's own weight notwithstanding.
    let p = fabric();
    let m = Measurements::default();
    for (d, k) in [(Direction::Warp, p.warp), (Direction::Weft, p.weft)] {
        for load in [2., 10.] {
            let gauge = fabric_kit::virtual_strip(&p, d, &m, load, options()).unwrap();
            let strain = gauge / (m.gauge_mm / 1000.) - 1.;
            let law = fabric_kit::law_strain(load / (m.strip_width_mm / 1000.), k, &p);
            assert!(
                (strain / law - 1.).abs() < 5e-3,
                "{d:?} at {load} N: {strain} against {law}"
            );
        }
    }
}

#[test]
fn virtual_cantilever_matches_the_analytic_bending_length() {
    let p = fabric();
    let q = p.density * fabric_kit::GRAVITY;
    // The ramp: the exact heavy elastica touches 41.5° at λ = qL³/G ≈ 8, so the textile
    // standard's G = w·g·(overhang/2)³ is the elastica's to a tenth of a percent.
    let star = fabric_kit::ramp_lambda();
    assert!((star / 8. - 1.).abs() < 1e-3, "{star}");
    let overhang = 0.06;
    assert!(
        (fabric_kit::rigidity_from_overhang(overhang, 120.)
            / fabric_kit::standard_rigidity(overhang, 120.)
            - 1.)
            .abs()
            < 1e-3
    );
    for d in [Direction::Warp, Direction::Weft] {
        // The strip's own rigidity, from a short overhang that barely droops: linear theory,
        // tip drop qL⁴/(8G), chord angle λ/8.
        let short = 0.012;
        let angle = fabric_kit::virtual_chord_angle(&p, d, short, options()).unwrap();
        assert!(angle < 0.02, "{angle}");
        let rigidity = q * short.powi(3) / (8. * angle);
        // The hinge model's continuum rigidity on the test grid: between the 2 B of a smooth
        // cylinder through equilateral meshes and the 3 B of one through the grid's rows.
        let along = if d == Direction::Warp {
            p.bend_warp
        } else {
            p.bend_weft
        };
        let mixed = fabric_kit::isotropic_mesh_rigidity(&p)[usize::from(d == Direction::Weft)];
        assert!(
            rigidity > 0.9 * mixed && rigidity < 3. * along,
            "{d:?}: {rigidity} against {along}"
        );
        // That rigidity in the exact elastica gives the overhang at which the virtual strip,
        // drooping far past linear theory, touches the 41.5° ramp.
        let analytic = fabric_kit::analytic_overhang(rigidity, 120.);
        let virtual_ = fabric_kit::virtual_cantilever(&p, d, options()).unwrap();
        assert!(
            (virtual_ / analytic - 1.).abs() < 0.01,
            "{d:?}: virtual {virtual_} against analytic {analytic}"
        );
        // And the bending length c = overhang / 2 is the standard's.
        let c = virtual_ / 2.;
        let standard = (rigidity / q).cbrt();
        assert!((c / standard - 1.).abs() < 0.012, "{c} against {standard}");
    }
}

#[test]
fn a_synthetic_fabrics_readings_give_back_its_parameters() {
    let truth = fabric();
    let readings = fabric_kit::virtual_readings(&truth, &template(), options()).unwrap();
    for d in Direction::ALL {
        for r in readings.readings(d) {
            assert!(r.length_mm > readings.gauge_mm, "{d:?}: {r:?}");
        }
    }
    let cancel = AtomicBool::new(false);
    let mut stages = vec![];
    let report = fabric_kit::fit(&readings, options(), &mut |s| stages.push(s), &cancel).unwrap();
    let p = &report.properties;
    // Stated tolerance: every fitted parameter within 1 % of the synthetic fabric's.
    for (name, got, want) in [
        ("warp", p.warp, truth.warp),
        ("weft", p.weft, truth.weft),
        ("shear", p.shear, truth.shear),
        ("bend_warp", p.bend_warp, truth.bend_warp),
        ("bend_weft", p.bend_weft, truth.bend_weft),
        ("density", p.density, truth.density),
        ("thickness", p.thickness, truth.thickness),
    ] {
        assert!(
            (got / want - 1.).abs() < 0.01,
            "{name}: {got} against {want}"
        );
    }
    assert!(report.notes.is_empty(), "{:?}", report.notes);
    assert!((report.friction.unwrap() - 24f64.to_radians().tan()).abs() < 1e-12);
    // Every reading is checked, and the fitted fabric reproduces it.
    assert_eq!(report.checks.len(), 3 + 3 + 2 + 2);
    for check in &report.checks {
        let tolerance = match check.test {
            Test::Strip { .. } => 0.02,
            Test::Cantilever { .. } => 0.2,
        };
        assert!(
            (check.simulated_mm - check.measured_mm).abs() < tolerance,
            "{check:?}"
        );
    }
    assert!(stages.contains(&Stage::Strip(Direction::Bias)));
    assert_eq!(stages.last(), Some(&Stage::Checking));
}

#[test]
fn missing_tests_are_filled_in_and_said() {
    let truth = fabric();
    let mut m = template();
    m.weft.clear();
    m.bias.clear();
    m.overhang_weft_mm = None;
    m.slide_angle_degrees = None;
    let readings = fabric_kit::virtual_readings(&truth, &m, options()).unwrap();
    let cancel = AtomicBool::new(false);
    let report = fabric_kit::fit(&readings, options(), &mut |_| {}, &cancel).unwrap();
    let p = &report.properties;
    assert!((p.warp / truth.warp - 1.).abs() < 0.01);
    assert_eq!(p.weft, p.warp);
    assert_eq!(p.bend_weft, p.bend_warp);
    assert!((p.shear / (0.04 * p.warp) - 1.).abs() < 1e-9);
    assert_eq!(report.notes.len(), 3, "{:?}", report.notes);
    assert!(report.friction.is_none());
}

#[test]
fn coarse_readings_and_impossible_ones_are_reported() {
    let cancel = AtomicBool::new(false);
    // A stiff woven read with a ruler: 1 mm of stretch on 100 mm.
    let mut m = Measurements {
        warp: vec![Reading {
            load_g: 500.,
            length_mm: 101.,
        }],
        ..Default::default()
    };
    let report = fabric_kit::fit(&m, options(), &mut |_| {}, &cancel).unwrap();
    assert!(report.notes.iter().any(|n| matches!(
        n,
        fabric_kit::Note::CoarseReading { percent, .. } if *percent > 40.
    )));
    // A bias strip stiffer than its warp and weft allow at any shear.
    m.warp[0].length_mm = 110.;
    m.weft = m.warp.clone();
    m.bias = vec![Reading {
        load_g: 500.,
        length_mm: 102.,
    }];
    let report = fabric_kit::fit(&m, options(), &mut |_| {}, &cancel).unwrap();
    assert!(report.notes.contains(&fabric_kit::Note::AtLimit {
        parameter: fabric_kit::Parameter::Shear
    }));
}

#[test]
fn measurements_are_checked_before_fitting() {
    let mut m = template();
    assert!(m.validate().is_err(), "readings at 0 mm are not readings");
    for r in &mut m.warp {
        r.length_mm = 101.;
    }
    for r in m.weft.iter_mut().chain(&mut m.bias) {
        r.length_mm = 102.;
    }
    assert!(m.validate().is_ok());
    m.strip_length_mm = 110.;
    assert!(m.validate().is_err(), "marks too close to the clamps");
    m.strip_length_mm = 200.;
    m.overhang_weft_mm = Some(2.);
    assert!(m.validate().is_err());
    m.overhang_weft_mm = None;
    m.warp.clear();
    m.weft.clear();
    assert!(m.validate().is_err(), "neither warp nor weft");
    let cancel = AtomicBool::new(true);
    let mut m = template();
    for r in m.warp.iter_mut().chain(&mut m.weft).chain(&mut m.bias) {
        r.length_mm = 105.;
    }
    assert_eq!(
        fabric_kit::fit(&m, options(), &mut |_| {}, &cancel).unwrap_err(),
        "Cancelled"
    );
}

#[test]
fn a_measured_fabric_file_keeps_its_measurements() {
    let truth = fabric();
    let m = Measurements {
        warp: vec![Reading {
            load_g: 1000.,
            length_mm: 104.,
        }],
        ..Default::default()
    };
    let report = fabric_kit::FitReport {
        properties: truth.clone(),
        friction: Some(0.4),
        checks: vec![],
        notes: vec![fabric_kit::Note::ShearEstimated],
        solves: 3,
    };
    let file = FabricFile::new("  Poplin 120  ", &m, &report);
    assert_eq!(file.name, "Poplin 120");
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join("Fabrics")
        .join(fabric_kit::file_name(&file.name));
    file.save(&path).unwrap();
    assert_eq!(path.extension().unwrap(), fabric_kit::FILE_EXTENSION);
    let back = FabricFile::load(&path).unwrap();
    assert_eq!(back, file);
    assert_eq!(back.measurements, m);
    assert_eq!(back.properties, truth);
    let text = file
        .to_json()
        .unwrap()
        .replace("fashioncraft.fabric", "something.else");
    assert!(FabricFile::from_json(&text).is_err());
}

/// Prints the readings the window's example uses, at the window's resolution:
/// `cargo test --test fabric_kit print_example_readings -- --ignored --nocapture`.
#[test]
#[ignore]
fn print_example_readings() {
    let m = fabric_kit::virtual_readings(&fabric(), &template(), FitOptions::default()).unwrap();
    println!("{m:#?}");
}

#[test]
fn readings_as_a_ruler_gives_them_still_give_the_fabric() {
    // The synthetic fabric's readings rounded as the procedure reads them: strips to the
    // ruler's 0.5 mm, overhangs to 1 mm.
    let truth = fabric();
    let mut m = fabric_kit::virtual_readings(&truth, &template(), options()).unwrap();
    for d in Direction::ALL {
        for r in m.readings_mut(d) {
            r.length_mm = (r.length_mm * 2.).round() / 2.;
        }
    }
    m.overhang_warp_mm = m.overhang_warp_mm.map(f64::round);
    m.overhang_weft_mm = m.overhang_weft_mm.map(f64::round);
    let cancel = AtomicBool::new(false);
    let p = fabric_kit::fit(&m, options(), &mut |_| {}, &cancel)
        .unwrap()
        .properties;
    for (name, got, want) in [
        ("warp", p.warp, truth.warp),
        ("weft", p.weft, truth.weft),
        ("shear", p.shear, truth.shear),
        ("bend_warp", p.bend_warp, truth.bend_warp),
        ("bend_weft", p.bend_weft, truth.bend_weft),
    ] {
        assert!(
            (got / want - 1.).abs() < 0.03,
            "{name}: {got} against {want}"
        );
    }
}

/// Numbers for docs/fabric-kit-fit.md:
/// `cargo test --release --test fabric_kit numbers_for_the_notes -- --ignored --nocapture`.
#[test]
#[ignore]
fn numbers_for_the_notes() {
    let p = FabricProperties {
        bend_weft: fabric().bend_warp,
        ..fabric()
    };
    let q = p.density * fabric_kit::GRAVITY;
    for cells in [16, 24, 32, 48] {
        let opts = FitOptions {
            cantilever_cells: cells,
            ..Default::default()
        };
        let short = 0.012;
        let angle = fabric_kit::virtual_chord_angle(&p, Direction::Warp, short, opts).unwrap();
        println!(
            "cells {cells}: G/B {:.3}",
            q * short.powi(3) / (8. * angle) / p.bend_warp
        );
    }
    let cancel = AtomicBool::new(false);
    let m = fabric_kit::virtual_readings(&fabric(), &template(), FitOptions::default()).unwrap();
    let t = std::time::Instant::now();
    let r = fabric_kit::fit(&m, FitOptions::default(), &mut |_| {}, &cancel).unwrap();
    println!("fit: {} solves in {:?}", r.solves, t.elapsed());
}
