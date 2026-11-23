//! Why does a sewn seam stay open? At a saved fitting state, find the widest seam probes and
//! list every contact term that touches their vertices, with its distance and push.
//!
//! Usage: cargo run --release --example seam_gap_probe -- fitting-project.json [probes]
use fashioncraft::{
    fitting::FittingProject,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;

fn panel_of(g: &fashioncraft::garment::CompiledGarment, v: usize) -> String {
    g.panels
        .iter()
        .find(|p| p.vertices.contains(&v))
        .map_or("?".into(), |p| p.id.clone())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Pass fitting-project.json")?;
    let shown: usize = std::env::args()
        .nth(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(3);
    let project = FittingProject::load(std::path::Path::new(&path))?;
    let settings = project.simulation.clone().unwrap_or(VbdSettings {
        dt: 1. / 60.,
        ..Default::default()
    });
    let scene = project.compile()?;
    let g = scene.garment.clone();
    let x = scene.state.x.clone();
    let mut solver = VbdSolver::new(g.clone(), Some(&scene.collider.surface), &scene.state)?;
    let world_x = solver.world.positions(&x);
    let r = settings.contact.radius;
    let contacts = solver.world.detect(&world_x, 4. * r, &settings.contact)?;
    let mut probes: Vec<_> = g
        .seams
        .iter()
        .flat_map(|s| s.probes.iter().map(move |p| (s.id.clone(), p)))
        .map(|(id, p)| {
            let gap: DVec3 = p.terms.iter().map(|(v, w)| x[*v] * *w).sum();
            (gap.length(), id, p)
        })
        .collect();
    probes.sort_by(|a, b| b.0.total_cmp(&a.0));
    let n = x.len();
    for (gap, id, p) in probes.iter().take(shown) {
        println!("seam {id}: gap {:.3} mm, terms {:?}", gap * 1000., p.terms);
        for (v, _) in &p.terms {
            let panel = panel_of(&g, *v);
            println!(
                "  vertex {v} (panel {panel}) bound {:.3} mm",
                contacts.radii[*v] * 1000.
            );
            for &c in &contacts.incident[*v] {
                let contact = &contacts.contacts[c];
                let rc = contacts.radius(c, r);
                let value = contact.value(&world_x, rc, settings.contact.stiffness);
                let (_, force, _) = contact.block(&world_x, *v, rc, settings.contact.stiffness);
                if value > 0. {
                    let ids: Vec<_> = contact.vertices[..contact.count]
                        .iter()
                        .map(|i| {
                            if *i < n {
                                format!("{i}:{}", panel_of(&g, *i))
                            } else {
                                format!("avatar{}", i - n)
                            }
                        })
                        .collect();
                    println!(
                        "    {:?} {ids:?} energy {value:.3e} J, force {:.3} N",
                        contact.kind,
                        force.length()
                    );
                }
            }
        }
    }
    Ok(())
}
