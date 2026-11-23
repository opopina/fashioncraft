//! Frayed edges: loose threads drawn along a cut edge, with a length and a density. Looks
//! only: the drape and the exported mesh are unchanged.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        CompiledGarment, GarmentTemplate, lines2d,
        presets::{self, GarmentKind},
        trims::{
            FRAY_COLOR, FRAY_INSET_M, Trim, TrimKind, TrimTriangle, fray_triangles, trim_triangles,
        },
    },
    obj_export::{self, ObjOptions, ObjScene},
};
use glam::DVec3;

/// Triangles per thread: two stretches, each two crossed ribbons of two triangles.
const PER_THREAD: usize = 8;

fn frayed(edge: &str, length_m: f64, density_per_m: f64) -> (GarmentTemplate, Trim) {
    let base = presets::basic(GarmentKind::Tshirt, 2);
    let panel = base.panels[0].id.clone();
    let polygon = base.panels[0].polygon.clone().unwrap();
    let path_m = lines2d::offset_lines(&polygon, edge, 1, FRAY_INSET_M, false, false)
        .unwrap()
        .pop()
        .unwrap();
    let trim = Trim {
        id: "fray_1".into(),
        panel_id: panel,
        kind: TrimKind::Fray {
            path_m,
            length_m,
            density_per_m,
        },
    };
    let edit = authoring::apply(&base, Command::SetTrim { trim: trim.clone() }).unwrap();
    // Looks only: the simulation is not reset, as for any trim.
    assert!(!edit.reset_simulation);
    assert!(authoring::only_appearance_changed(&base, &edit.template));
    (edit.template, trim)
}

fn path_length(trim: &Trim) -> f64 {
    let TrimKind::Fray { path_m, .. } = &trim.kind else {
        unreachable!()
    };
    path_m.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Each thread's base (where it leaves the cloth) and tip.
fn threads(triangles: &[TrimTriangle]) -> Vec<(DVec3, DVec3)> {
    triangles
        .chunks(PER_THREAD)
        .map(|t| {
            let base = (t[0].points[0] + t[0].points[1]) * 0.5;
            let tip = (t[6].points[2] + t[7].points[2]) * 0.5;
            (base, tip)
        })
        .collect()
}

#[test]
fn a_frayed_edge_draws_loose_threads_across_it() {
    let (template, trim) = frayed("hem", 0.008, 300.);
    let g: CompiledGarment = template.compile().unwrap();
    let x = &g.initial_state.x;
    let color = [0.2, 0.3, 0.6];
    let drawn = fray_triangles(&trim, &g, x, color);
    // About the asked density along the edge.
    let expected = (path_length(&trim) * 300.).round() as usize;
    assert_eq!(drawn.len(), expected * PER_THREAD);
    assert!(expected > 100, "{expected}");
    // In the fabric's colour, a little lighter or darker thread by thread.
    assert!(drawn.iter().all(|t| {
        let k = t.color[0] / color[0];
        (0.75..=1.2).contains(&k) && t.color.iter().all(|c| c.is_finite())
    }));
    // Every thread runs out across the hem, no longer than asked.
    let hem: Vec<DVec3> = g.panels[0].boundaries["hem"]
        .iter()
        .map(|v| x[*v])
        .collect();
    let middle: DVec3 = g.panels[0].vertices.clone().map(|v| x[v]).sum::<DVec3>()
        / g.panels[0].vertices.len() as f64;
    let edge_middle = hem.iter().sum::<DVec3>() / hem.len() as f64;
    let outward = (edge_middle - middle).normalize();
    let hairs = threads(&drawn);
    for (base, tip) in &hairs {
        let distance = base.distance(*tip);
        assert!(
            distance > 0.25 * 0.008 && distance <= 0.008 + 1e-9,
            "{distance}"
        );
        // Its base lies on the cloth, a millimetre inside the edge.
        let off = hem
            .windows(2)
            .map(|w| {
                let d = w[1] - w[0];
                let t = ((*base - w[0]).dot(d) / d.length_squared()).clamp(0., 1.);
                base.distance(w[0] + d * t)
            })
            .fold(f64::INFINITY, f64::min);
        assert!(off < 0.003, "{off}");
    }
    let across = hairs
        .iter()
        .filter(|(base, tip)| (*tip - *base).dot(outward) > 0.)
        .count();
    assert!(
        across * 10 >= hairs.len() * 9,
        "{across} of {}",
        hairs.len()
    );
    // The same threads every time.
    assert_eq!(drawn, fray_triangles(&trim, &g, x, color));
    // Longer and denser threads when asked.
    let (dense, dense_trim) = frayed("hem", 0.02, 900.);
    let more = fray_triangles(&dense_trim, &dense.compile().unwrap(), x, color);
    assert!(more.len() > 2 * drawn.len());
    let longest = threads(&more)
        .iter()
        .map(|(a, b)| a.distance(*b))
        .fold(0., f64::max);
    assert!(longest > 0.012, "{longest}");
    // Drawn among the trims too (in the neutral thread colour when no fabric is given).
    let all = trim_triangles(std::slice::from_ref(&trim), &g, x);
    assert_eq!(all.len(), drawn.len());
    assert!(all.iter().all(|t| t.color[0] <= FRAY_COLOR[0] * 1.2));
}

#[test]
fn fraying_is_saved_validated_and_not_exported() {
    let (template, trim) = frayed("cuff-left", 0.006, 400.);
    // Saved and opened again.
    let text = serde_json::to_string(&template).unwrap();
    assert!(text.contains("\"type\":\"fray\""));
    let reopened: GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(reopened.trims, template.trims);
    // Lengths and densities out of range are refused.
    let base = presets::basic(GarmentKind::Tshirt, 2);
    for (length_m, density_per_m) in [(0.0, 400.), (0.2, 400.), (0.006, 1.), (0.006, 1e5)] {
        let mut bad = trim.clone();
        bad.kind = match bad.kind {
            TrimKind::Fray { path_m, .. } => TrimKind::Fray {
                path_m,
                length_m,
                density_per_m,
            },
            other => other,
        };
        assert!(
            authoring::apply(&base, Command::SetTrim { trim: bad }).is_err(),
            "{length_m} {density_per_m}"
        );
    }
    // Render only: the exported garment is the same with or without fraying.
    let dir = tempfile::tempdir().unwrap();
    let export = |t: &GarmentTemplate, name: &str| {
        // Same file name in its own folder, so the files can be compared byte for byte.
        let folder = dir.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("garment.obj");
        let g = t.compile().unwrap();
        let summary = obj_export::write(
            &ObjScene {
                template: t,
                garment: &g,
                state: &g.initial_state,
                images: &[],
                avatar: None,
                atlas: None,
            },
            &ObjOptions::default(),
            &file,
        )
        .unwrap();
        (summary, std::fs::read_to_string(&file).unwrap())
    };
    let (plain, plain_text) = export(&base, "plain");
    let (fray, fray_text) = export(&template, "fray");
    assert_eq!(plain, fray);
    assert_eq!(plain_text, fray_text);
    // Removing it is one edit.
    let gone = authoring::apply(&template, Command::RemoveTrim { id: trim.id })
        .unwrap()
        .template;
    assert!(gone.trims.is_empty());
}
