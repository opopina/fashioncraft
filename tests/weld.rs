//! Welded seams: every seam's two edges become one, vertex to vertex, by adding each side's
//! vertices to the other side's edge and merging what is sewn together.
use fashioncraft::{
    fitting::FittingProject,
    garment::presets::GarmentKind,
    gltf_export::front_triangles,
    obj_export::{self, ObjOptions, ObjScene},
    weld::{Welded, weld},
};
use glam::{DVec2, DVec3};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// The basic shirt with its back meshed finer than its front, so no seam has the same
/// vertices on both sides.
fn shirt() -> FittingProject {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let back = project.template.panels[1].polygon.as_mut().unwrap();
    back.meshing.as_mut().unwrap().target_edge_m = 0.037;
    project
}

/// How many triangles use each edge between groups.
fn edge_uses(w: &Welded, pieces: impl Iterator<Item = usize>) -> HashMap<(usize, usize), usize> {
    let mut uses = HashMap::new();
    for k in pieces {
        for t in &w.triangles[k] {
            for i in 0..3 {
                let (a, b) = (w.group[t[i]], w.group[t[(i + 1) % 3]]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    uses
}

/// Signed area of a triangle in the pattern (material coordinates).
fn area(uv: &[DVec2], t: &[usize; 3]) -> f64 {
    (uv[t[1]] - uv[t[0]]).perp_dot(uv[t[2]] - uv[t[0]]) / 2.
}

#[test]
fn every_seam_point_is_merged_with_its_partner_into_one_closed_surface() {
    let project = shirt();
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let triangles = front_triangles(&project.template, garment);
    let all = vec![true; garment.panels.len()];
    let w = weld(garment, &triangles, &all);
    // The pieces' meshes do not match along the seams: vertices had to be added.
    assert!(!w.added.is_empty());
    let seams: Vec<_> = garment
        .seams
        .iter()
        .filter(|s| !s.id.starts_with("tack:"))
        .collect();
    assert!(!seams.is_empty());
    for seam in &seams {
        assert!(seam.pairs.len() >= 2, "{}", seam.id);
        for [a, b] in &seam.pairs {
            let (a, b) = (w.vertex_at(*a).unwrap(), w.vertex_at(*b).unwrap());
            assert_eq!(w.group[a], w.group[b], "{}", seam.id);
        }
    }
    // Nothing is lost or turned over: each piece covers the same pattern area, with every
    // triangle facing as before.
    let uv = w.extend(&garment.mesh.uv, |a, b, t| a.lerp(b, t));
    for (k, piece) in garment.panels.iter().enumerate() {
        let before: f64 = triangles[piece.triangles.clone()]
            .iter()
            .map(|t| area(&uv, t))
            .sum();
        let turn = if w.flipped[k] { -1. } else { 1. };
        let after: f64 = turn * w.triangles[k].iter().map(|t| area(&uv, t)).sum::<f64>();
        assert!(
            (before - after).abs() < 1e-9 * before.abs(),
            "{}: {before} {after}",
            piece.id
        );
        for t in &w.triangles[k] {
            assert!(
                turn * area(&uv, t) * before.signum() > 1e-12,
                "{}",
                piece.id
            );
        }
        // Every added vertex is a corner of the piece's triangles.
        for v in &w.piece_added[k] {
            assert!(w.triangles[k].iter().any(|t| t.contains(v)));
        }
    }
    // One surface: no edge has more than two triangles, and its Euler characteristic and
    // open outlines are a shirt's (a sphere with neck, hem and two cuffs cut out).
    let uses = edge_uses(&w, 0..garment.panels.len());
    assert!(uses.values().all(|u| *u <= 2));
    let faces: usize = w.triangles.iter().map(Vec::len).sum();
    let corners: BTreeSet<usize> = w
        .triangles
        .iter()
        .flatten()
        .flat_map(|t| t.map(|i| w.group[i]))
        .collect();
    let euler = corners.len() as i64 - uses.len() as i64 + faces as i64;
    let open: Vec<(usize, usize)> = uses
        .iter()
        .filter(|(_, u)| **u == 1)
        .map(|(e, _)| *e)
        .collect();
    let mut next: HashMap<usize, Vec<usize>> = HashMap::new();
    for (a, b) in &open {
        next.entry(*a).or_default().push(*b);
        next.entry(*b).or_default().push(*a);
    }
    assert!(next.values().all(|n| n.len() == 2), "outlines cross");
    let mut seen = BTreeSet::new();
    let mut outlines = 0;
    for start in next.keys() {
        if seen.insert(*start) {
            outlines += 1;
            let mut stack = vec![*start];
            while let Some(v) = stack.pop() {
                for n in &next[&v] {
                    if seen.insert(*n) {
                        stack.push(*n);
                    }
                }
            }
        }
    }
    assert_eq!(outlines, 4);
    assert_eq!(euler, 2 - outlines);
    // One outside: the basic shirt's back is drawn like its front, facing the body, so it
    // is turned over, and then every welded edge runs opposite ways in its two triangles.
    assert_eq!(w.flipped, [false, true]);
    let mut directed = BTreeSet::new();
    for t in w.triangles.iter().flatten() {
        for i in 0..3 {
            assert!(directed.insert((w.group[t[i]], w.group[t[(i + 1) % 3]])));
        }
    }
}

#[test]
fn only_seams_between_exported_pieces_are_welded() {
    let project = shirt();
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let triangles = front_triangles(&project.template, garment);
    let piece = |v: usize| {
        garment
            .panels
            .iter()
            .position(|p| p.vertices.contains(&v))
            .unwrap()
    };
    // The front alone.
    let kept: Vec<bool> = (0..garment.panels.len()).map(|k| k == 0).collect();
    let w = weld(garment, &triangles, &kept);
    assert!(w.added.is_empty());
    for (k, keep) in kept.iter().enumerate() {
        assert_eq!(w.triangles[k].is_empty(), !keep);
    }
    for seam in &garment.seams {
        for [a, b] in &seam.pairs {
            let both = kept[piece(a[0].0)] && kept[piece(b[0].0)];
            let merged = match (w.vertex_at(*a), w.vertex_at(*b)) {
                (Some(a), Some(b)) => w.group[a] == w.group[b],
                _ => false,
            };
            assert_eq!(merged, both, "{}", seam.id);
        }
    }
    // Nothing welded leaves every vertex on its own.
    let none = Welded::none(garment, &triangles);
    assert!(none.added.is_empty());
    assert_eq!(none.groups, garment.mesh.uv.len());
    assert!(none.group.iter().enumerate().all(|(i, g)| i == *g));
}

#[test]
fn a_welded_obj_shares_its_seam_vertices_between_pieces() {
    let project = shirt();
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let triangles = front_triangles(&project.template, garment);
    let w = weld(garment, &triangles, &vec![true; garment.panels.len()]);
    let dir = tempfile::tempdir().unwrap();
    let export = ObjScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: None,
    };
    let path = dir.path().join("welded.obj");
    let options = ObjOptions {
        weld: true,
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let faces: usize = w.triangles.iter().map(Vec::len).sum();
    assert_eq!(summary.faces, faces);
    assert_eq!(summary.vertices, w.groups);
    // Position indices used by more than one piece: the welded seams.
    let mut piece_of: HashMap<usize, BTreeSet<String>> = HashMap::new();
    let mut piece = String::new();
    let mut texture = 0;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("g ") {
            piece = name.to_string();
        } else if line.starts_with("vt ") {
            texture += 1;
        } else if let Some(face) = line.strip_prefix("f ") {
            for corner in face.split_whitespace() {
                let v: usize = corner.split('/').next().unwrap().parse().unwrap();
                assert!((1..=w.groups).contains(&v));
                piece_of.entry(v).or_default().insert(piece.clone());
            }
        }
    }
    // Texture coordinates stay per piece: one per vertex, added ones too.
    assert_eq!(texture, garment.mesh.uv.len() + w.added.len());
    assert!(piece_of.values().filter(|p| p.len() > 1).count() > 50);
    // Sewn points are written once, where both sides met.
    let x = w.extend(&scene.state.x, |a, b, t| a.lerp(b, t));
    let position = w.positions(&x);
    let [a, b] = garment.seams[0].pairs[1];
    let (a, b) = (w.vertex_at(a).unwrap(), w.vertex_at(b).unwrap());
    let members: Vec<DVec3> = (0..x.len())
        .filter(|i| w.group[*i] == w.group[a])
        .map(|i| x[i])
        .collect();
    assert!(members.len() >= 2 && w.group[a] == w.group[b]);
    let mean = members.iter().sum::<DVec3>() / members.len() as f64;
    assert!(position[w.group[a]].distance(mean) < 1e-12);
    let p = position[w.group[a]] * 100.;
    assert!(text.contains(&format!("v {:.6} {:.6} {:.6}", p.x, p.y, p.z)));

    // Thick: the rim runs only along the open outlines, not along the welded seams.
    let options = ObjOptions {
        weld: true,
        thick: true,
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    let open = edge_uses(&w, 0..garment.panels.len())
        .values()
        .filter(|u| **u == 1)
        .count();
    assert_eq!(summary.faces, 2 * faces + 2 * open);
    assert_eq!(summary.vertices, 2 * w.groups);
}
