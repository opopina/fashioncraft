//! Alembic and USD import, read back from what the exporters write: the garment and the
//! avatar come back with the same points and triangles.
use fashioncraft::{
    abc_export::{self, AbcScene},
    abc_import,
    avatar::SurfaceMesh,
    fitting::FittingProject,
    garment::presets::GarmentKind,
    usd_export::{self, UsdScene},
    usd_import,
};
use std::path::PathBuf;

fn scene() -> (
    FittingProject,
    fashioncraft::fitting::FittingScene,
    SurfaceMesh,
) {
    let avatar = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar.clone());
    let scene = project.compile().unwrap();
    let body = SurfaceMesh::load_obj(&avatar).unwrap();
    (project, scene, body)
}

/// As many points, spanning the same box within `tolerance`.
fn same_points(a: &[glam::DVec3], b: &[glam::DVec3], tolerance: f64) {
    assert_eq!(a.len(), b.len());
    let bounds = |v: &[glam::DVec3]| {
        v.iter().fold(
            (
                glam::DVec3::splat(f64::INFINITY),
                glam::DVec3::splat(f64::NEG_INFINITY),
            ),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        )
    };
    let (alo, ahi) = bounds(a);
    let (blo, bhi) = bounds(b);
    assert!(alo.distance(blo) < tolerance && ahi.distance(bhi) < tolerance);
}

#[test]
fn alembic_written_by_the_exporter_reads_back() {
    let (project, scene, body) = scene();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garment.abc");
    abc_export::write(
        &AbcScene {
            template: &project.template,
            garment: &scene.garment,
            state: &scene.state,
            avatar: Some(&body),
            take: None,
        },
        &path,
    )
    .unwrap();
    let imported = abc_import::abc(&std::fs::read(&path).unwrap()).unwrap();
    let cloth = scene.state.x.len();
    let mut expected = scene.state.x.clone();
    expected.extend(&body.vertices);
    same_points(&expected, &imported.mesh.vertices, 1e-5);
    // The avatar's triangles come back with their own winding.
    let avatar_triangles =
        &imported.mesh.triangles[imported.mesh.triangles.len() - body.triangles.len()..];
    for (got, want) in avatar_triangles.iter().zip(&body.triangles).take(50) {
        assert_eq!(got.map(|i| i - cloth), *want);
    }
    assert!(imported.warnings.iter().any(|w| w.contains("metres")));
}

#[test]
fn usd_text_and_usdz_written_by_the_exporter_read_back() {
    let (project, scene, body) = scene();
    let dir = tempfile::tempdir().unwrap();
    let usd = UsdScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&body),
        take: None,
    };
    let mut expected = scene.state.x.clone();
    expected.extend(&body.vertices);
    for name in ["garment.usda", "garment.usdz"] {
        let path = dir.path().join(name);
        usd_export::write(&usd, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let extension = name.rsplit('.').next().unwrap();
        let imported = usd_import::usd(&bytes, extension).unwrap();
        same_points(&expected, &imported.mesh.vertices, 1e-5);
        assert!(imported.mesh.triangles.len() >= body.triangles.len());
    }
}

#[test]
fn broken_files_are_refused() {
    assert!(abc_import::abc(b"not an alembic file at all").is_err());
    assert!(abc_import::abc(b"Ogawa\xff\x00\x01\x10\x00\x00\x00\x00\x00\x00\x00").is_err());
    assert!(usd_import::usd(b"PXR-USDC\x00\x01", "usd").is_err());
    assert!(usd_import::usd(b"PK\x03\x04", "usdz").is_err());
}

/// Files written by Blender (see tests/fixtures/blender-io): the same world-space mesh as
/// Blender reports, with every closed body wound outward.
#[test]
fn blender_alembic_and_usd_match_its_world_space_mesh() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blender-io");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    let corner = |k: &str| {
        let v: Vec<f64> = expected[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        glam::DVec3::new(v[0], v[1], v[2])
    };
    for (name, imported) in [
        (
            "abc",
            abc_import::abc(&std::fs::read(dir.join("scene.abc")).unwrap()).unwrap(),
        ),
        (
            "usda",
            usd_import::usd(&std::fs::read(dir.join("scene.usda")).unwrap(), "usda").unwrap(),
        ),
    ] {
        let mesh = &imported.mesh;
        assert_eq!(
            mesh.vertices.len() as u64,
            expected["vertices"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            mesh.triangles.len() as u64,
            expected["triangles"].as_u64().unwrap(),
            "{name}"
        );
        let (lo, hi) = mesh.vertices.iter().fold(
            (
                glam::DVec3::splat(f64::INFINITY),
                glam::DVec3::splat(f64::NEG_INFINITY),
            ),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        assert!(lo.distance(corner("lo")) < 1e-5, "{name}: {lo}");
        assert!(hi.distance(corner("hi")) < 1e-5, "{name}: {hi}");
        // Closed bodies wound counter-clockwise from outside enclose a positive volume.
        let volume: f64 = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i]);
                a.dot(b.cross(c)) / 6.
            })
            .sum();
        assert!(volume > 0., "{name}: {volume}");
    }
}
