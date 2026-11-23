//! A check of grading through the interface, in Spanish, on the T-shirt FashionCraft opens
//! with: sizes made in the grading window, a growth typed as a sum in the grade table and
//! turned into an equal step, the size on show picked in the 2D bar and simulated, every
//! size laid over the base, undo that keeps the size on show, the points of measure, and the
//! pattern exported as DXF-AAMA with its rule table.
//!
//!     FASHIONCRAFT_DEMO=grading fashioncraft
use super::demo::{Step, act, caption, click, until};
use super::grading_ui::{Change, edit};
use super::*;
use fashioncraft::garment::grading::{Target as Row, pom};
use glam::DVec2;

/// Seconds each result stays on screen for a look (and a screenshot).
const LOOK: f64 = 4.;

fn size_id(a: &Atelier, name: &str) -> Option<String> {
    let g = a.project.template.grading.as_ref()?;
    g.groups
        .first()?
        .sizes
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.id.clone())
}

/// The growth of a contour point of the front in size `name`.
fn growth(a: &Atelier, edge: &str, name: &str) -> Option<DVec2> {
    let g = a.project.template.grading.as_ref()?;
    let group = g.groups.first()?;
    let rule = g.rule(&Row::Point {
        panel: "front".into(),
        edge: edge.into(),
    })?;
    Some(rule.delta(group, &size_id(a, name)?))
}

fn shown(a: &Atelier) -> Option<String> {
    let g = a.project.template.grading.as_ref()?;
    let group = g.groups.first()?;
    Some(group.name_of(group.active()).to_string())
}

/// Width of the simulated cloth's material outline, metres.
fn simulated_width(a: &Atelier) -> f64 {
    let (lo, hi) = a
        .scene
        .garment
        .mesh
        .uv
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.x), hi.max(p.x))
        });
    hi - lo
}

pub(super) fn grading() -> Vec<Step> {
    let base_width = std::rc::Rc::new(std::cell::Cell::new(0.));
    let remember = base_width.clone();
    let wider = base_width.clone();
    let mut s = vec![
        act("start from the default workspace, in Spanish", |a, ctx| {
            a.prefs.workspace = Default::default();
            a.prefs.garment_display = Default::default();
            a.clean_view = false;
            a.prefs.language = i18n::Language::Spanish;
            a.dispatch_command(commands::CommandId::SplitView, ctx)
        }),
        Step::Wait(1.0),
        Step::Record(true),
        act("select the front piece", move |a, _| {
            a.authoring.selected = Some("front".into());
            remember.set(simulated_width(a));
            Ok(())
        }),
        caption("Grading: 2D Pattern › Grading › Sizes and grade rules"),
    ];
    super::demo_vest::menu(&mut s, "2D Pattern", Some("Grading"), "grading.table");
    s.extend([
        until(
            "the grading window is open",
            |a| a.authoring.grading.open,
            5.,
        ),
        Step::Wait(1.0),
        caption("Sizes S M L XL, base M"),
        click("grading.create"),
        until(
            "the garment has sizes S M L XL, base M",
            |a| {
                a.project.template.grading.as_ref().is_some_and(|g| {
                    let group = &g.groups[0];
                    group.sizes.len() == 4 && group.base_name() == "M"
                })
            },
            5.,
        ),
        Step::Wait(1.0),
        caption("Type a growth as a sum: 10+2 mm across for the hem's corner in L"),
        act(
            "pick the hem's corner, P9, on the board: the table scrolls to its row",
            |a, _| {
                a.authoring.anchor = Some(("front".into(), 8));
                Ok(())
            },
        ),
        Step::Wait(1.0),
        click("grading.row.P9"),
        click("field:front-P9-hem-s3-0"),
        Step::Key(egui::Key::A, egui::Modifiers::COMMAND),
        Step::Type("10+2".into()),
        Step::Key(egui::Key::Enter, egui::Modifiers::NONE),
        until(
            "L's hem corner grows 12 mm across",
            |a| growth(a, "hem", "L").is_some_and(|d| (d - DVec2::new(0.012, 0.)).length() < 1e-9),
            5.,
        ),
        Step::Wait(1.0),
        caption("An equal step between sizes"),
        click("grading.step.P9"),
        until(
            "the corner grows 12 mm a size: 24 mm in XL, -12 mm in S",
            |a| {
                growth(a, "hem", "XL").is_some_and(|d| (d.x - 0.024).abs() < 1e-9)
                    && growth(a, "hem", "S").is_some_and(|d| (d.x + 0.012).abs() < 1e-9)
            },
            5.,
        ),
        act(
            "give the other points their rules (the same table, typed faster)",
            |a, _| {
                a.apply_grading_change(edit("Grade rule", |g, _| {
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
                            g.set_step(
                                &Row::Point {
                                    panel: panel.into(),
                                    edge: edge.into(),
                                },
                                step * 0.001,
                            )?;
                        }
                    }
                    Ok(())
                }));
                Ok(())
            },
        ),
        until(
            "the side seam points grow too",
            |a| growth(a, "side-right", "L").is_some_and(|d| d.x > 0.009),
            5.,
        ),
        Step::Wait(LOOK),
        caption("Show XL in 3D from the 2D bar"),
        click("grading.size.XL"),
        until(
            "XL is shown and the simulated cloth is wider",
            move |a| shown(a).as_deref() == Some("XL") && simulated_width(a) > wider.get() + 0.03,
            10.,
        ),
        Step::Wait(1.0),
        caption("Drape XL"),
        click("toolbar.simulate"),
        until(
            "XL has draped for 1.5 s",
            |a| a.scene.state.time > 1.5,
            180.,
        ),
        click("toolbar.simulate"),
        until("the simulation is paused", |a| !a.playing, 5.),
        Step::Wait(LOOK),
        caption("Every size over the base"),
        click("grading.all_sizes"),
        until(
            "the board shows every size",
            |a| a.authoring.grading.review,
            5.,
        ),
        Step::Wait(LOOK),
        caption("Undo takes the rules back; the size on show stays"),
        click("menu:Edit"),
        click("cmd:edit.undo"),
        until(
            "the side seam's rule is gone and XL is still shown",
            |a| {
                growth(a, "side-right", "L").is_none_or(|d| d == DVec2::ZERO)
                    && shown(a).as_deref() == Some("XL")
            },
            10.,
        ),
        Step::Wait(1.0),
        click("menu:Edit"),
        click("cmd:edit.redo"),
        until(
            "the rules are back",
            |a| growth(a, "side-right", "L").is_some_and(|d| d.x > 0.009),
            10.,
        ),
        caption("Points of measure in every size"),
    ]);
    super::demo_vest::menu(
        &mut s,
        "2D Pattern",
        Some("Grading"),
        "grading.points_of_measure",
    );
    s.extend([
        act("add three points of measure", |a, _| {
            a.apply_grading_change(edit("Add measurement", |g, _| {
                g.add_measurement(
                    "Ancho de pecho",
                    pom::distance("front", "side-right", "sleeve-under-left"),
                    0.002,
                )?;
                g.add_measurement("Costado", pom::length(&[("front", "side-right")]), 0.003)?;
                g.add_measurement(
                    "Contorno de bajo",
                    pom::length(&[("front", "hem"), ("back", "hem")]),
                    0.005,
                )?;
                Ok(())
            }));
            Ok(())
        }),
        until(
            "the chest grows 20 mm a size",
            |a| {
                let rows = pom::table(&a.project.template);
                let Some(l) = size_id(a, "L") else {
                    return false;
                };
                rows.first()
                    .and_then(|r| r.difference(&l))
                    .is_some_and(|d| (d - 0.02).abs() < 1e-6)
            },
            5.,
        ),
        Step::Wait(LOOK),
        act(
            "export the pattern as DXF-AAMA with its rule table, in the session folder",
            |a, _| {
                let dir = fashioncraft::agent::session_dir().join("demo-grading");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                use fashioncraft::garment::{dxf_aama, grading};
                let export = grading::dxf::export(
                    &a.project.template,
                    &Default::default(),
                    dxf_aama::Units::Millimetres,
                    dxf_aama::Dialect::Astm,
                )?;
                let mut table = export.table.ok_or("no rule table was made")?;
                table.name = "TSHIRT".into();
                let bytes = dxf_aama::write(
                    &export.pieces,
                    &dxf_aama::WriteOptions {
                        dialect: dxf_aama::Dialect::Astm,
                        units: dxf_aama::Units::Millimetres,
                        style: a.project.template.name.clone(),
                        sample_size: table.sample.clone(),
                        author: dxf_aama::template::author(),
                        created: Some(dxf_aama::template::now_utc()),
                        grade_rule_table: table.name.clone(),
                    },
                )?;
                std::fs::write(dir.join("tshirt.dxf"), bytes).map_err(|e| e.to_string())?;
                std::fs::write(dir.join("tshirt.rul"), grading::rul::write(&table)?)
                    .map_err(|e| e.to_string())?;
                Ok(())
            },
        ),
        act("back to the base size", |a, _| {
            let g = a.project.template.grading.as_ref().ok_or("no grading")?;
            let group = &g.groups[0];
            a.apply_grading_change(Change::Show(group.id.clone(), group.base.clone()));
            Ok(())
        }),
        until("M is shown again", |a| shown(a).as_deref() == Some("M"), 5.),
        caption(""),
        Step::Wait(1.0),
        Step::Record(false),
    ]);
    s
}
