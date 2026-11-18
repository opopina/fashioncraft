//! Graphics on the 3D cloth: clipped to their image rectangle and to the piece, lifted a
//! little off the side they are printed on, and their image files kept with the project.
use glam::{DVec2, DVec3};
use marvelous_cloth::{
    fitting::{FittingProject, ImageAsset},
    garment::{
        CompiledGarment, GarmentTemplate,
        graphics::{GRAPHIC_LIFT_M, GraphicPlacement, GraphicSide, clip_to_image},
        presets::GarmentKind,
    },
};

fn project() -> FittingProject {
    FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
}

fn uv_area(t: [DVec2; 3]) -> f64 {
    0.5 * (t[1] - t[0]).perp_dot(t[2] - t[0]).abs()
}

fn blend(uv: [DVec2; 3], w: DVec3) -> DVec2 {
    uv[0] * w.x + uv[1] * w.y + uv[2] * w.z
}

#[test]
fn clipping_keeps_what_lies_inside_the_image_square() {
    let inside = [
        DVec2::new(0.1, 0.1),
        DVec2::new(0.9, 0.1),
        DVec2::new(0.1, 0.9),
    ];
    assert_eq!(clip_to_image(inside), vec![[DVec3::X, DVec3::Y, DVec3::Z]]);
    let outside = [
        DVec2::new(1.1, 0.1),
        DVec2::new(1.9, 0.1),
        DVec2::new(1.1, 0.9),
    ];
    assert!(clip_to_image(outside).is_empty());
    // A big right triangle covering the square's lower-left half and more: the part inside
    // is the square minus the corner beyond the hypotenuse x + y = 1.5.
    let big = [DVec2::new(0., 0.), DVec2::new(1.5, 0.), DVec2::new(0., 1.5)];
    let pieces = clip_to_image(big);
    let area: f64 = pieces
        .iter()
        .map(|w| uv_area(w.map(|w| blend(big, w))))
        .sum();
    assert!((area - (1. - 0.5 * 0.5 * 0.5)).abs() < 1e-12, "area {area}");
    for w in pieces.iter().flatten() {
        assert!((w.x + w.y + w.z - 1.).abs() < 1e-12);
        let p = blend(big, *w);
        assert!(p.cmpge(DVec2::splat(-1e-12)).all() && p.cmple(DVec2::splat(1. + 1e-12)).all());
    }
}

/// The demo's front panel alone, flat and upright.
fn front_panel() -> (CompiledGarment, String) {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let id = t.panels[0].id.clone();
    (t.compile().unwrap(), id)
}

fn placement(panel: &str, side: GraphicSide) -> GraphicPlacement {
    GraphicPlacement {
        id: "graphic_1".into(),
        panel_id: panel.into(),
        asset_id: "image_1".into(),
        center_m: DVec2::new(0.1, 0.12),
        size_m: DVec2::new(0.08, 0.04),
        rotation_degrees: 20.,
        side,
    }
}

/// The front normal of the flat panel: away from its faces' winding normal.
fn front_normal(g: &CompiledGarment) -> DVec3 {
    let x = &g.initial_state.x;
    let [a, b, c] = g.mesh.triangles[0].vertices;
    -(x[b] - x[a]).cross(x[c] - x[a]).normalize()
}

#[test]
fn a_graphic_covers_its_whole_image_once_lifted_off_its_side() {
    let (g, panel) = front_panel();
    let x = &g.initial_state.x;
    let front = front_normal(&g);
    let plane = x[g.mesh.triangles[0].vertices[0]];
    for (side, flipped, towards) in [
        (GraphicSide::Front, false, front),
        (GraphicSide::Back, false, -front),
        // Flipping the piece's normals swaps its front and back.
        (GraphicSide::Front, true, -front),
    ] {
        let triangles = placement(&panel, side)
            .graphic_triangles(&g, x, flipped)
            .unwrap();
        assert!(!triangles.is_empty());
        // The image square is covered exactly once: the UV areas add up to one.
        let area: f64 = triangles.iter().map(|t| uv_area(t.uv)).sum();
        assert!((area - 1.).abs() < 1e-9, "{side:?} uv area {area}");
        // The 3D area is the placed rectangle's.
        let area_3d: f64 = triangles
            .iter()
            .map(|t| {
                0.5 * (t.points[1] - t.points[0])
                    .cross(t.points[2] - t.points[0])
                    .length()
            })
            .sum();
        assert!((area_3d - 0.08 * 0.04).abs() < 1e-9, "3D area {area_3d}");
        for t in &triangles {
            for k in 0..3 {
                let lift = (t.points[k] - plane).dot(towards);
                assert!((lift - GRAPHIC_LIFT_M).abs() < 1e-12, "lift {lift}");
                assert!(t.normals[k].dot(towards) > 0.999);
            }
        }
    }
}

#[test]
fn a_graphic_beyond_the_piece_is_cut_at_its_outline() {
    let (g, panel) = front_panel();
    let mut p = placement(&panel, GraphicSide::Front);
    p.rotation_degrees = 0.;
    // Centred on the panel's lower-left corner: only a quarter of it lies on the cloth.
    let uv = &g.mesh.uv;
    let corner = uv.iter().fold(DVec2::splat(f64::MAX), |m, p| m.min(*p));
    p.center_m = corner;
    let triangles = p.graphic_triangles(&g, &g.initial_state.x, false).unwrap();
    let area: f64 = triangles.iter().map(|t| uv_area(t.uv)).sum();
    assert!((area - 0.25).abs() < 1e-9, "uv area {area}");
}

#[test]
fn graphic_images_are_copied_with_the_project_and_unused_ones_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("logo.png");
    std::fs::write(&image, b"not really a png, but bytes to copy").unwrap();
    let stray = dir.path().join("stray.png");
    std::fs::write(&stray, b"unused").unwrap();
    let mut project = project();
    let panel = project.template.panels[0].id.clone();
    project.template.graphics = vec![placement(&panel, GraphicSide::Front)];
    project.image_assets = vec![
        ImageAsset {
            id: "image_1".into(),
            path: image.clone(),
        },
        ImageAsset {
            id: "image_2".into(),
            path: stray,
        },
    ];
    let file = dir.path().join("saved").join("garment.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    project.save(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("image_2"));
    assert!(text.contains("garment.json.assets"));
    let loaded = FittingProject::load(&file).unwrap();
    assert_eq!(loaded.image_assets.len(), 1);
    let asset = &loaded.image_assets[0];
    assert_eq!(asset.id, "image_1");
    assert!(asset.path.starts_with(file.parent().unwrap()));
    assert_eq!(
        std::fs::read(&asset.path).unwrap(),
        std::fs::read(&image).unwrap()
    );
}

#[test]
fn duplicate_image_asset_ids_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project();
    project.image_assets = vec![
        ImageAsset {
            id: "image_1".into(),
            path: dir.path().join("a.png"),
        },
        ImageAsset {
            id: "image_1".into(),
            path: dir.path().join("b.png"),
        },
    ];
    let error = project.save(&dir.path().join("p.json")).unwrap_err();
    assert!(error.contains("Duplicate image asset"), "{error}");
}

#[test]
fn images_used_by_fabric_textures_are_kept_with_the_project() {
    use marvelous_cloth::garment::appearance::{FabricAppearance, FabricTexture};
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("plaid.png");
    std::fs::write(&image, b"pixels").unwrap();
    let mut project = project();
    let fabric = project.template.fabrics[0].id.clone();
    project.template.fabric_appearance.insert(
        fabric,
        FabricAppearance {
            texture: Some(FabricTexture {
                asset_id: "image_1".into(),
                width_mm: 30.,
                height_mm: 30.,
                rotation_degrees: 0.,
                relief_mm: 0.,
            }),
            ..Default::default()
        },
    );
    project.image_assets = vec![ImageAsset {
        id: "image_1".into(),
        path: image,
    }];
    let file = dir.path().join("p.json");
    project.save(&file).unwrap();
    let loaded = FittingProject::load(&file).unwrap();
    assert_eq!(loaded.image_assets.len(), 1);
    assert!(loaded.image_assets[0].path.exists());
}
