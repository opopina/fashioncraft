//! Local comparison helper: arrange a 0.4 × 0.6 m panel on an anchor and write its sampled points as OBJ.
use fashioncraft::{
    avatar::SurfaceMesh,
    garment::arrangement::{arrange, load_sidecar},
};
use glam::{DVec2, DVec3};
use std::{fmt::Write, path::Path};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let anchor = args.next().unwrap_or_else(|| "torso-middle-front".into());
    let out = args
        .next()
        .unwrap_or_else(|| "output/arrangement-compare.obj".into());
    let wrap = args.next().is_none_or(|w| w != "flat");
    let avatar_path = Path::new("assets/makehuman/mannequin.obj");
    let avatar = SurfaceMesh::load_obj(avatar_path)?;
    let sidecar = load_sidecar(avatar_path)?.ok_or("no anchors")?;
    let a = sidecar
        .set
        .anchors
        .iter()
        .find(|a| a.id == anchor)
        .ok_or("unknown anchor")?;
    let frame = sidecar.frame(&anchor)?;
    let rect = [
        DVec2::ZERO,
        DVec2::new(0.4, 0.0),
        DVec2::new(0.4, 0.6),
        DVec2::new(0.0, 0.6),
    ];
    let r = arrange(
        &rect,
        &frame,
        0.05,
        wrap.then_some(a.max_wrap_degrees),
        Some(&avatar),
    )?;
    let mut obj = String::new();
    for i in 0..=40 {
        for j in 0..=60 {
            let local = DVec3::new(0.01 * i as f64, -0.01 * j as f64, 0.0);
            let local = r.wrap.map_or(local, |w| w.bend(local));
            let p = r.placement.transform(local);
            writeln!(obj, "v {} {} {}", p.x, p.y, p.z).unwrap();
        }
    }
    std::fs::write(&out, obj).map_err(|e| e.to_string())?;
    println!(
        "{anchor}: skin {:?} m, radius {:.3} m, wrap {:?} → {out}",
        r.skin_m, r.radius_m, r.wrap
    );
    Ok(())
}
