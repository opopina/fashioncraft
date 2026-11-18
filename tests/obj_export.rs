//! OBJ export: the garment with its material library in the chosen units and up axis, thick
//! or thin, one object or one per piece, packed or real-size texture coordinates, with the
//! avatar; and MDD point caches of a take.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    avatar_animation::Take,
    fitting::FittingProject,
    garment::{
        appearance::{FabricAppearance, FabricTexture},
        presets::GarmentKind,
    },
    obj_export::{self, ObjOptions, ObjScene, Units},
};
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// Lines starting with `tag`, split into their fields.
fn lines<'a>(text: &'a str, tag: &str) -> Vec<Vec<&'a str>> {
    text.lines()
        .filter_map(|l| l.strip_prefix(tag).map(|r| r.split_whitespace().collect()))
        .collect()
}

/// Every index of every face is within the lines written of its kind.
fn indices_are_valid(text: &str) {
    let (v, vt, vn) = (
        lines(text, "v ").len(),
        lines(text, "vt ").len(),
        lines(text, "vn ").len(),
    );
    for face in lines(text, "f ") {
        assert_eq!(face.len(), 3);
        for corner in face {
            let parts: Vec<&str> = corner.split('/').collect();
            for (k, limit) in [(0, v), (1, vt), (2, vn)] {
                if let Some(p) = parts.get(k).filter(|p| !p.is_empty()) {
                    let i: usize = p.parse().unwrap();
                    assert!((1..=limit).contains(&i), "{corner}");
                }
            }
        }
    }
}

#[test]
fn the_garment_is_written_in_the_chosen_units_with_its_material_library() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tee.obj");
    let export = ObjScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: None,
    };
    let summary = obj_export::write(&export, &ObjOptions::default(), &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    indices_are_valid(&text);
    let x = &scene.state.x;
    assert_eq!(summary.vertices, x.len());
    assert_eq!(summary.faces, scene.garment.mesh.triangles.len());
    assert!(text.contains("mtllib tee.mtl"));
    // Centimetres by default.
    let first: Vec<f64> = lines(&text, "v ")[0]
        .iter()
        .map(|v| v.parse().unwrap())
        .collect();
    assert!((first[1] - x[scene.garment.panels[0].vertices.start].y * 100.).abs() < 1e-4);
    // One group per piece in one object, and each fabric in the library.
    assert_eq!(lines(&text, "o ").len(), 1);
    assert_eq!(lines(&text, "g ").len(), scene.garment.panels.len());
    let mtl = std::fs::read_to_string(dir.path().join("tee.mtl")).unwrap();
    for piece in &project.template.panels {
        assert!(mtl.contains(&format!("newmtl {}", piece.fabric)), "{mtl}");
    }
    // The geometry reads back.
    let back = SurfaceMesh::parse_obj(&text).unwrap();
    assert_eq!(back.triangles.len(), scene.garment.mesh.triangles.len());

    // Metres, Z up: y and z trade places.
    let options = ObjOptions {
        units: Units::Metres,
        z_up: true,
        ..Default::default()
    };
    obj_export::write(&export, &options, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let first: Vec<f64> = lines(&text, "v ")[0]
        .iter()
        .map(|v| v.parse().unwrap())
        .collect();
    let p = x[scene.garment.panels[0].vertices.start];
    assert!((first[2] - p.y).abs() < 1e-5 && (first[1] + p.z).abs() < 1e-5);
}

#[test]
fn thick_cloth_has_two_faces_a_rim_and_the_avatar_beside_it() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let avatar = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tee.obj");
    let export = ObjScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: Some(&avatar),
        atlas: None,
    };
    let options = ObjOptions {
        thick: true,
        units: Units::Millimetres,
        per_piece: true,
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    indices_are_valid(&text);
    let n = scene.state.x.len();
    let triangles = scene.garment.mesh.triangles.len();
    assert_eq!(summary.vertices, 2 * n + avatar.vertices.len());
    // Two faces of every triangle, two rim triangles per outline edge, and the avatar.
    let rim = summary.faces - 2 * triangles - avatar.triangles.len();
    assert!(rim > 0 && rim.is_multiple_of(2), "{rim}");
    // One object per piece, then the avatar.
    let objects: Vec<String> = lines(&text, "o ").iter().map(|o| o.join(" ")).collect();
    assert_eq!(objects.len(), scene.garment.panels.len() + 1);
    assert_eq!(objects.last().unwrap(), "avatar");
    // The two faces stand 1 mm apart (no look thickness set).
    let v = lines(&text, "v ");
    let at = |k: usize| {
        DVec3::new(
            v[k][0].parse().unwrap(),
            v[k][1].parse().unwrap(),
            v[k][2].parse().unwrap(),
        )
    };
    let count = scene.garment.panels[0].vertices.len();
    assert!(
        (at(0).distance(at(count)) - 1.).abs() < 1e-3,
        "{}",
        at(0).distance(at(count))
    );
    let mtl = std::fs::read_to_string(dir.path().join("tee.mtl")).unwrap();
    assert!(mtl.contains("newmtl avatar"));
}

#[test]
fn packed_coordinates_fit_the_unit_square_and_textures_travel_with_real_size_ones() {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let fabric = project.template.panels[0].fabric.clone();
    project.template.fabric_appearance.insert(
        fabric,
        FabricAppearance {
            color_srgb: [10, 20, 200],
            roughness: 0.5,
            texture: Some(FabricTexture {
                asset_id: "print".into(),
                width_mm: 100.,
                height_mm: 100.,
                rotation_degrees: 0.,
                relief_mm: 0.,
                normal_asset_id: None,
                roughness_asset_id: None,
            }),
            ..Default::default()
        },
    );
    let scene = project.compile().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("source.png");
    std::fs::write(&image, b"png bytes").unwrap();
    let images = [("print".to_string(), image)];
    let export = ObjScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &images,
        avatar: None,
        atlas: None,
    };
    let path = dir.path().join("out").join("tee.obj");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let summary = obj_export::write(&export, &ObjOptions::default(), &path).unwrap();
    assert_eq!(summary.textures, 1);
    let mtl = std::fs::read_to_string(dir.path().join("out/tee.mtl")).unwrap();
    assert!(mtl.contains("map_Kd tee_print.png"));
    assert!(mtl.contains("Kd 0.0392 0.0784 0.7843"));
    assert_eq!(
        std::fs::read(dir.path().join("out/tee_print.png")).unwrap(),
        b"png bytes"
    );

    // Packed: every piece inside the unit square, apart from the others; no tiled texture.
    let options = ObjOptions {
        unified_uv: true,
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    assert_eq!(summary.textures, 0);
    let text = std::fs::read_to_string(&path).unwrap();
    let vt: Vec<[f64; 2]> = lines(&text, "vt ")
        .iter()
        .map(|l| [l[0].parse().unwrap(), l[1].parse().unwrap()])
        .collect();
    assert!(vt.iter().flatten().all(|c| (-1e-9..=1. + 1e-9).contains(c)));
    let mut boxes = Vec::new();
    let mut start = 0;
    for piece in &scene.garment.panels {
        let part = &vt[start..start + piece.vertices.len()];
        start += piece.vertices.len();
        let lo = part
            .iter()
            .fold([f64::INFINITY; 2], |m, p| [m[0].min(p[0]), m[1].min(p[1])]);
        let hi = part.iter().fold([f64::NEG_INFINITY; 2], |m, p| {
            [m[0].max(p[0]), m[1].max(p[1])]
        });
        boxes.push((lo, hi));
    }
    for (a, b) in boxes.iter().zip(boxes.iter().skip(1)) {
        let apart = a.1[0] <= b.0[0] || b.1[0] <= a.0[0] || a.1[1] <= b.0[1] || b.1[1] <= a.0[1];
        assert!(apart, "{a:?} {b:?}");
    }
    // Only the selected piece.
    let options = ObjOptions {
        pieces: Some([project.template.panels[1].id.clone()].into()),
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    assert_eq!(summary.vertices, scene.garment.panels[1].vertices.len());
    let options = ObjOptions {
        pieces: Some(["nothing".to_string()].into()),
        ..Default::default()
    };
    assert!(obj_export::write(&export, &options, &path).is_err());
}

#[test]
fn a_take_is_written_as_an_mdd_point_cache() {
    let take = Take {
        fps: 25.,
        frames: vec![vec![[1., 2., 3.]], vec![[4., 5., 6.]]],
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cloth.mdd");
    take.write_mdd(&path, false).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 8 + 2 * 4 + 2 * 12);
    let int = |at: usize| i32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    let float = |at: usize| f32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!((int(0), int(4)), (2, 1));
    assert_eq!((float(8), float(12)), (0., 0.04));
    assert_eq!([float(16), float(20), float(24)], [1., 2., 3.]);
    take.write_mdd(&path, true).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let float = |at: usize| f32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!([float(28), float(32), float(36)], [4., -6., 5.]);
    assert!(Take::default().write_mdd(&path, false).is_err());
}

#[test]
fn a_baked_look_is_one_material_with_its_image_and_coordinates() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let atlas =
        marvelous_cloth::bake::bake(&project.template, &scene.garment, &Default::default(), 64)
            .unwrap();
    let export = ObjScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: Some(&atlas),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tee.obj");
    let summary = obj_export::write(&export, &ObjOptions::default(), &path).unwrap();
    assert_eq!(summary.textures, 1);
    let mtl = std::fs::read_to_string(dir.path().join("tee.mtl")).unwrap();
    assert!(mtl.contains("newmtl baked") && mtl.contains("map_Kd tee_baked.png"));
    let png = std::fs::read(dir.path().join("tee_baked.png")).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.lines()
            .filter(|l| l.starts_with("usemtl"))
            .all(|l| l == "usemtl baked")
    );
    // Texture coordinates are the atlas's, counted from the image's bottom.
    let vt: Vec<[f64; 2]> = lines(&text, "vt ")
        .iter()
        .map(|l| [l[0].parse().unwrap(), l[1].parse().unwrap()])
        .collect();
    assert_eq!(vt.len(), atlas.uv.len());
    for (t, a) in vt.iter().zip(&atlas.uv) {
        assert!((t[0] - a.x).abs() < 1e-6 && (t[1] - (1. - a.y)).abs() < 1e-6);
    }
}

#[test]
fn quads_join_triangle_pairs_without_losing_or_turning_any() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let triangles =
        marvelous_cloth::gltf_export::front_triangles(&project.template, &scene.garment);
    let x = &scene.state.x;
    let piece = &scene.garment.panels[0];
    let tris = &triangles[piece.triangles.clone()];
    let faces = obj_export::quads(x, tris);
    let area = |a: usize, b: usize, c: usize| (x[b] - x[a]).cross(x[c] - x[a]);
    let quads = faces.iter().filter(|f| f.len() == 4).count();
    let singles = faces.iter().filter(|f| f.len() == 3).count();
    // Every triangle once: in a quad or on its own.
    assert_eq!(2 * quads + singles, tris.len());
    let joined = 2. * quads as f64 / tris.len() as f64;
    assert!(
        joined > 0.7,
        "only {joined:.2} of the triangles are in quads"
    );
    // The same surface, facing the same way.
    let total: DVec3 = tris.iter().map(|t| area(t[0], t[1], t[2])).sum();
    let mut covered = DVec3::ZERO;
    for f in &faces {
        let n = area(f[0], f[1], f[2])
            + if f.len() == 4 {
                area(f[0], f[2], f[3])
            } else {
                DVec3::ZERO
            };
        covered += n;
        if f.len() == 4 {
            let mut distinct = f.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), 4);
            assert!(area(f[0], f[1], f[2]).dot(area(f[0], f[2], f[3])) > 0.);
        }
    }
    assert!((covered - total).length() < 1e-12 * total.length().max(1.));
    // Written as faces of four corners.
    let export = ObjScene {
        template: &project.template,
        garment: &scene.garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: None,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tee.obj");
    let options = ObjOptions {
        quads: true,
        ..Default::default()
    };
    let summary = obj_export::write(&export, &options, &path).unwrap();
    assert!(summary.faces < triangles.len());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(lines(&text, "f ").iter().any(|f| f.len() == 4));
}
