//! A body moving into cloth pushes it ahead instead of passing through it. A cylinder (a leg,
//! say) sweeps into a free square of cloth, stepped the way `avatar_animation::run_stepping`
//! records a take: the obstacle moves at most `BODY_STEP_M` per step, the cloth follows.
//! The CPU check runs with the ordinary suite; the GPU one is an explicit hardware regression.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_animation::{BODY_STEP_M, MIN_ITERATIONS},
    garment::{CompiledGarment, GarmentTemplate},
    model::{Pattern, State},
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

const RADIUS: f64 = 0.06;
const HEIGHT: f64 = 0.5;
const SIDES: usize = 32;
/// How far the cylinder travels along +z, and in how long: a shin's swing in a walk.
const TRAVEL: f64 = 0.15;
const DURATION: f64 = 0.2;
/// The sweep stops at full speed, before the square has wrapped round the cylinder far enough
/// to fold onto itself in front of it.
const SWEPT: f64 = 0.09;

/// A closed cylinder standing on y = 0, its axis through (0, y, `z`).
fn cylinder(z: f64) -> SurfaceMesh {
    let mut vertices = Vec::new();
    for y in [0., HEIGHT] {
        for k in 0..SIDES {
            let a = k as f64 * std::f64::consts::TAU / SIDES as f64;
            vertices.push(DVec3::new(RADIUS * a.cos(), y, z + RADIUS * a.sin()));
        }
    }
    vertices.push(DVec3::new(0., 0., z));
    vertices.push(DVec3::new(0., HEIGHT, z));
    let (bottom, top) = (2 * SIDES, 2 * SIDES + 1);
    let mut triangles = Vec::new();
    for k in 0..SIDES {
        let j = (k + 1) % SIDES;
        triangles.push([k, j, SIDES + j]);
        triangles.push([k, SIDES + j, SIDES + k]);
        triangles.push([bottom, j, k]);
        triangles.push([top, SIDES + k, SIDES + j]);
    }
    SurfaceMesh {
        vertices,
        triangles,
    }
}

/// Where the cylinder's axis is at time `t`: at rest, then along +z on an ease-in-out curve
/// whose top speed is 1.5·TRAVEL/DURATION (about 1.1 m/s), then at rest again.
fn axis_z(t: f64) -> f64 {
    let s = (t / DURATION).clamp(0., 1.);
    TRAVEL * s * s * (3. - 2. * s)
}

/// A free 24 cm square of cloth, upright and facing the cylinder 8 mm in front of it.
fn scene() -> (Arc<CompiledGarment>, State) {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pin_points.clear();
    p.pattern = Pattern {
        width: 0.24,
        height: 0.24,
        nx: 16,
        ny: 16,
    };
    // The panel's local (u, −v, 0) placed upright in the xy plane.
    p.placement.rotation_degrees = DVec3::ZERO;
    p.placement.translation = DVec3::new(-0.12, 0.37, RADIUS + 0.008);
    let g = Arc::new(t.compile().unwrap());
    let state = g.initial_state.clone();
    (g, state)
}

/// Cloth vertices inside the cylinder whose axis is at `z`, and the deepest of them (m).
fn inside(x: &[DVec3], z: f64) -> (usize, f64) {
    // The polygon's inscribed radius, less a tenth of a millimetre.
    let r = RADIUS * (std::f64::consts::PI / SIDES as f64).cos() - 1e-4;
    x.iter()
        .filter(|p| (0. ..HEIGHT).contains(&p.y))
        .map(|p| r - DVec2::new(p.x, p.z - z).length())
        .filter(|d| *d > 0.)
        .fold((0, 0.), |(n, deepest), d| (n + 1, f64::max(deepest, d)))
}

struct Outcome {
    inside: usize,
    deepest: f64,
    /// How far the middle of the cloth, where the cylinder pushes it, moved along +z.
    carried: f64,
    steps: usize,
}

/// Sweep the cylinder through the cloth, each step taken by `step`, as the recorder does: the
/// body moves at most `BODY_STEP_M` per step, and a shorter step iterates proportionally less.
fn sweep(
    mut step: impl FnMut(&mut VbdSolver, &mut State, &VbdSettings) -> Result<(), String>,
) -> Outcome {
    let (g, mut state) = scene();
    let middle: Vec<_> = (0..state.x.len())
        .filter(|&i| state.x[i].x.abs() < 0.02)
        .collect();
    let start = state.x.clone();
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 16,
        gravity: DVec3::ZERO,
        friction: 0.3,
        ..Default::default()
    };
    let mut solver = VbdSolver::new(g, Some(&cylinder(0.)), &state).unwrap();
    let (mut time, mut steps, mut worst) = (0., 0, (0, 0.));
    while time < SWEPT {
        let mut end = (time + settings.dt).min(SWEPT);
        while axis_z(end) - axis_z(time) > BODY_STEP_M {
            end = time + (end - time) * 0.9 * BODY_STEP_M / (axis_z(end) - axis_z(time));
        }
        let z = axis_z(end);
        solver.set_obstacle(&state.x, &cylinder(z)).unwrap();
        let mut part = settings.clone();
        part.dt = end - time;
        part.iterations = ((settings.iterations as f64 * part.dt / settings.dt).ceil() as usize)
            .clamp(MIN_ITERATIONS, settings.iterations);
        step(&mut solver, &mut state, &part).unwrap();
        assert!(state.x.iter().all(|p| p.is_finite()));
        let (n, d) = inside(&state.x, z);
        worst = (worst.0.max(n), f64::max(worst.1, d));
        time = end;
        steps += 1;
    }
    Outcome {
        inside: worst.0,
        deepest: worst.1,
        carried: middle
            .iter()
            .map(|&i| state.x[i].z - start[i].z)
            .sum::<f64>()
            / middle.len() as f64,
        steps,
    }
}

fn report(label: &str, o: &Outcome) {
    eprintln!(
        "{label}: {} steps, at most {} cloth vertices inside the cylinder (deepest {:.1} mm), \
         cloth carried {:.1} cm of the cylinder's {:.1} cm",
        o.steps,
        o.inside,
        o.deepest * 1000.,
        o.carried * 100.,
        axis_z(SWEPT) * 100.
    );
}

#[test]
fn only_a_real_move_of_the_body_counts_as_one() {
    let (g, state) = scene();
    let mut solver = VbdSolver::new(g, Some(&cylinder(0.)), &state).unwrap();
    assert!(!solver.world.take_fixed_moved());
    // A recorder sets the body every step, also while it stands still, posed again with
    // rounding differences.
    solver.set_obstacle(&state.x, &cylinder(0.)).unwrap();
    assert!(!solver.world.take_fixed_moved());
    solver.set_obstacle(&state.x, &cylinder(1e-12)).unwrap();
    assert!(!solver.world.take_fixed_moved());
    solver.set_obstacle(&state.x, &cylinder(0.001)).unwrap();
    assert!(solver.world.take_fixed_moved());
    assert!(!solver.world.take_fixed_moved());
}

#[test]
fn a_moving_cylinder_pushes_the_cloth_ahead_on_the_cpu() {
    let o = sweep(|solver, state, step| solver.step(state, step, None).map(|_| ()));
    report("CPU", &o);
    assert_eq!(o.inside, 0, "deepest {} m", o.deepest);
    assert!(o.carried > 0.75 * axis_z(SWEPT), "carried {} m", o.carried);
}

#[cfg(feature = "desktop")]
#[test]
#[ignore = "uses a hardware GPU; run explicitly with --ignored"]
fn a_moving_cylinder_pushes_the_cloth_ahead_on_the_gpu() {
    use eframe::wgpu;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = fashioncraft::gpu_vbd::GpuVbd::new(device, queue);
    let o = sweep(|solver, state, step| gpu.step(solver, state, step, None).map(|_| ()));
    report("GPU", &o);
    assert_eq!(o.inside, 0, "deepest {} m", o.deepest);
    assert!(o.carried > 0.75 * axis_z(SWEPT), "carried {} m", o.carried);
}
