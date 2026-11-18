//! Pieces traced on the body: a loop through picked points along the surface, the smaller
//! side it encloses, flattened keeping lengths — exactly for developable patches.
use glam::{DVec2, DVec3};
use marvelous_cloth::{
    avatar::SurfaceMesh,
    trace3d::{enclosed, flatten, surface_loop, trace},
};
use std::path::PathBuf;

/// A closed tube of `around` × `along` quads (two triangles each), radius `r`, length `h`,
/// capped at both ends by a fan to a centre point.
fn cylinder(r: f64, h: f64, around: usize, along: usize) -> SurfaceMesh {
    let mut vertices = vec![];
    for j in 0..=along {
        for i in 0..around {
            let a = i as f64 / around as f64 * std::f64::consts::TAU;
            vertices.push(DVec3::new(
                r * a.cos(),
                h * j as f64 / along as f64,
                r * a.sin(),
            ));
        }
    }
    let id = |i: usize, j: usize| j * around + i % around;
    let mut triangles = vec![];
    for j in 0..along {
        for i in 0..around {
            triangles.push([id(i, j), id(i, j + 1), id(i + 1, j)]);
            triangles.push([id(i + 1, j), id(i, j + 1), id(i + 1, j + 1)]);
        }
    }
    let bottom = vertices.len();
    vertices.push(DVec3::ZERO);
    let top = vertices.len();
    vertices.push(DVec3::new(0., h, 0.));
    for i in 0..around {
        triangles.push([bottom, id(i, 0), id(i + 1, 0)]);
        triangles.push([top, id(i + 1, along), id(i, along)]);
    }
    SurfaceMesh {
        vertices,
        triangles,
    }
}

fn sphere(r: f64, rings: usize, segments: usize) -> SurfaceMesh {
    let mut vertices = vec![DVec3::new(0., r, 0.)];
    for j in 1..rings {
        let polar = std::f64::consts::PI * j as f64 / rings as f64;
        for i in 0..segments {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            vertices.push(DVec3::new(
                r * polar.sin() * a.cos(),
                r * polar.cos(),
                r * polar.sin() * a.sin(),
            ));
        }
    }
    let south = vertices.len();
    vertices.push(DVec3::new(0., -r, 0.));
    let id = |i: usize, j: usize| 1 + (j - 1) * segments + i % segments;
    let mut triangles = vec![];
    for i in 0..segments {
        triangles.push([0, id(i + 1, 1), id(i, 1)]);
        triangles.push([south, id(i, rings - 1), id(i + 1, rings - 1)]);
    }
    for j in 1..rings - 1 {
        for i in 0..segments {
            triangles.push([id(i, j), id(i + 1, j), id(i, j + 1)]);
            triangles.push([id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
        }
    }
    SurfaceMesh {
        vertices,
        triangles,
    }
}

fn perimeter(points: &[DVec2]) -> f64 {
    (0..points.len())
        .map(|k| points[k].distance(points[(k + 1) % points.len()]))
        .sum()
}

fn area(points: &[DVec2]) -> f64 {
    (0..points.len())
        .map(|k| points[k].perp_dot(points[(k + 1) % points.len()]))
        .sum::<f64>()
        / 2.
}

#[test]
fn a_patch_of_a_cylinder_unrolls_without_stretching() {
    let (r, h) = (0.2, 0.6);
    let mesh = cylinder(r, h, 48, 24);
    // A quarter turn, from 10 to 40 cm up: corners on the grid.
    let at = |i: usize, j: usize| j * 48 + i;
    let points = [at(0, 4), at(12, 4), at(12, 16), at(0, 16)];
    let cycle = surface_loop(&mesh, &points).unwrap();
    let patch = enclosed(&mesh, &cycle).unwrap();
    // The smaller side: the quarter's 12 × 12 quads.
    assert_eq!(patch.len(), 2 * 12 * 12);
    let flat = flatten(&mesh, &patch, &cycle).unwrap();
    assert!(flat.stretch_max < 1e-6, "{}", flat.stretch_max);
    let outline = flat.outline_points();
    assert!(area(&outline) > 0., "counter-clockwise");
    // The unrolled quarter: chord-wise width (the mesh's straight edges) times 30 cm.
    let chord = 2. * r * (std::f64::consts::PI / 48.).sin();
    let (width, height) = (12. * chord, 0.3);
    assert!((area(&outline) - width * height).abs() < 1e-9);
    assert!((perimeter(&outline) - 2. * (width + height)).abs() < 1e-9);
    // Upright and seen from outside: up the tube is +y; going round it (towards +z from +x)
    // runs to the left of someone looking at it.
    let (low, high) = (at(0, 4), at(0, 16));
    let local = |v: usize| flat.vertices.iter().position(|u| *u == v).unwrap();
    assert!((flat.flat[local(high)].y - flat.flat[local(low)].y - 0.3).abs() < 1e-9);
    assert!(flat.flat[local(at(0, 4))].x > flat.flat[local(at(12, 4))].x);
    // Every flat point has its place on the body: the outline goes back onto the loop.
    for (k, v) in flat.outline.iter().enumerate() {
        let body = flat.on_body(&mesh, flat.flat[*v]);
        assert!(
            body.distance(mesh.vertices[flat.vertices[*v]]) < 1e-9,
            "{k}"
        );
    }
}

#[test]
fn a_patch_of_a_sphere_flattens_with_little_stretch_and_its_area_kept() {
    let r = 0.15;
    let mesh = sphere(r, 40, 64);
    // A square-ish patch around the equator, 40° each way.
    let id = |i: usize, j: usize| 1 + (j - 1) * 64 + i % 64;
    let points = [id(0, 16), id(7, 16), id(7, 24), id(0, 24)];
    let traced = trace(&mesh, &points).unwrap();
    assert!(traced.stretch_rms < 0.03, "{}", traced.stretch_rms);
    let body: f64 = {
        let mut total = 0.;
        let mut seen = std::collections::HashSet::new();
        for t in &traced.triangles {
            let [a, b, c] = t.map(|k| mesh.vertices[traced.vertices[k]]);
            total += (b - a).cross(c - a).length() / 2.;
            seen.insert(t[0]);
        }
        total
    };
    let flat = area(&traced.outline_points());
    assert!((flat / body - 1.).abs() < 0.05, "{flat} vs {body}");
}

#[test]
fn loops_that_cross_themselves_or_are_too_short_are_refused() {
    let mesh = cylinder(0.2, 0.6, 24, 12);
    assert!(surface_loop(&mesh, &[0, 5]).is_err());
    // Back and forth along the same edges.
    assert!(surface_loop(&mesh, &[0, 6, 24 * 3, 6]).is_err());
}

#[test]
fn a_piece_traced_on_the_mannequins_chest_lies_flat_with_its_lengths_kept() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let mesh = SurfaceMesh::load_obj(&path).unwrap();
    // The vertex of the body's front nearest each point: a 16 × 20 cm panel on the chest.
    let front = |x: f64, y: f64| {
        (0..mesh.vertices.len())
            .filter(|i| mesh.vertices[*i].z > 0.)
            .min_by(|a, b| {
                let d = |i: usize| {
                    let p = mesh.vertices[i];
                    (p.x - x).powi(2) + (p.y - y).powi(2) - 0.01 * p.z
                };
                d(*a).total_cmp(&d(*b))
            })
            .unwrap()
    };
    let points = [
        front(-0.08, 1.20),
        front(0.08, 1.20),
        front(0.08, 1.40),
        front(-0.08, 1.40),
    ];
    let traced = trace(&mesh, &points).unwrap();
    assert!(traced.triangles.len() > 50);
    assert!(traced.stretch_rms < 0.05, "{}", traced.stretch_rms);
    let outline = traced.outline_points();
    let size = outline.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let extent = size.1 - size.0;
    // Upright, about the drawn size: 16 cm across the curved chest, 20 cm up it.
    assert!(
        (0.16..0.22).contains(&extent.x) && (0.19..0.23).contains(&extent.y),
        "{extent}"
    );
    // Seen from the front: the avatar's left (+x) is on the right.
    let flat_x = |v: usize| traced.flat[traced.vertices.iter().position(|u| *u == v).unwrap()].x;
    assert!(flat_x(points[1]) > flat_x(points[0]));
    // Pattern points: v grows down the body.
    let pattern = traced.pattern_points();
    let top = traced
        .outline
        .iter()
        .position(|k| traced.vertices[*k] == points[3])
        .unwrap();
    let bottom = traced
        .outline
        .iter()
        .position(|k| traced.vertices[*k] == points[0])
        .unwrap();
    assert!(pattern[top].y < pattern[bottom].y);
}

#[test]
fn a_traced_piece_is_drawn_and_starts_on_the_body_where_it_was_traced() {
    use marvelous_cloth::{
        authoring::{self, Command},
        fitting::FittingProject,
        garment::{meshing::MeshingSettings, presets::GarmentKind},
        trace3d::simplify,
        vbd::VbdSolver,
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, path.clone());
    let scene = project.compile().unwrap();
    let body = &scene.collider.surface;
    let front = |x: f64, y: f64| {
        (0..body.vertices.len())
            .filter(|i| body.vertices[*i].z > 0.)
            .min_by(|a, b| {
                let d = |i: usize| {
                    let p = body.vertices[i];
                    (p.x - x).powi(2) + (p.y - y).powi(2) - 0.01 * p.z
                };
                d(*a).total_cmp(&d(*b))
            })
            .unwrap()
    };
    let picked = [
        front(-0.1, 1.15),
        front(0.1, 1.15),
        front(0.1, 1.35),
        front(-0.1, 1.35),
    ];
    let traced = trace(body, &picked).unwrap();
    let points = simplify(&traced.pattern_points(), 0.0015);
    assert!(points.len() >= 4 && points.len() < traced.outline.len());
    // The simplified outline stays within the tolerance of the traced one.
    for p in traced.pattern_points() {
        let gap = (0..points.len())
            .map(|k| {
                let (a, b) = (points[k], points[(k + 1) % points.len()]);
                let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0., 1.);
                p.distance(a + (b - a) * t)
            })
            .fold(f64::INFINITY, f64::min);
        assert!(gap <= 0.0015 + 1e-12, "{gap}");
    }
    let shift = points
        .iter()
        .copied()
        .fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let edit = authoring::apply(
        &project.template,
        Command::Polygon {
            tangents: vec![DVec2::ZERO; points.len()],
            points,
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: project.template.fabrics[0].id.clone(),
            replace: false,
        },
    )
    .unwrap();
    let garment = &edit.garment;
    let piece = garment.panels.last().unwrap();
    let uv = &garment.mesh.uv[piece.vertices.clone()];
    // On the body the piece keeps the pattern's lengths (the flattening's own stretch).
    let on_body = traced.body_positions(body, uv, shift, 0.);
    let mut squares = 0.;
    let mut count = 0.;
    for t in &garment.mesh.triangles[piece.triangles.clone()] {
        for k in 0..3 {
            let (a, b) = (t.vertices[k], t.vertices[(k + 1) % 3]);
            let (i, j) = (a - piece.vertices.start, b - piece.vertices.start);
            let flat = uv[i].distance(uv[j]);
            let change = (on_body[i].distance(on_body[j]) - flat) / flat;
            squares += change * change;
            count += 1.;
        }
    }
    let rms = (squares / count).sqrt();
    assert!(rms < 0.04, "{rms}");
    // It starts a contact radius and a millimetre out, which the solver accepts.
    let placed = traced.body_positions(body, uv, shift, 0.004);
    // Each point just off the body.
    for p in &placed {
        let nearest = body
            .vertices
            .iter()
            .map(|v| v.distance(*p))
            .fold(f64::INFINITY, f64::min);
        assert!((0.001..0.03).contains(&nearest), "{nearest}");
    }
    // The solver can start from there: nothing intersects the body or the other pieces.
    let mut state = edit.state_after(&scene.garment, &scene.state);
    for (v, p) in piece.vertices.clone().zip(&placed) {
        state.x[v] = *p;
    }
    VbdSolver::new(std::sync::Arc::new(garment.clone()), Some(body), &state).unwrap();
}
