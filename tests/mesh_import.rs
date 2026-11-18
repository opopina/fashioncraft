//! Mesh import: STL and PLY in every encoding, scaled from their size when they declare no
//! unit, and COLLADA scenes with their units, up axis, node transforms and skins.
use glam::DVec3;
use marvelous_cloth::{avatar::SurfaceMesh, avatar_import::Candidate, mesh_import};

/// A tetrahedron `size` units across, as loose triangles.
fn tetrahedron(size: f32) -> Vec<[[f32; 3]; 3]> {
    let p = [[0., 0., 0.], [size, 0., 0.], [0., size, 0.], [0., 0., size]];
    [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]
        .iter()
        .map(|t| t.map(|i| p[i]))
        .collect()
}

fn binary_stl(triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
    let mut b = vec![0u8; 80];
    b.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for t in triangles {
        b.extend_from_slice(&[0u8; 12]);
        for p in t {
            for c in p {
                b.extend_from_slice(&c.to_le_bytes());
            }
        }
        b.extend_from_slice(&[0u8; 2]);
    }
    b
}

fn bounds(mesh: &SurfaceMesh) -> (DVec3, DVec3) {
    mesh.vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

#[test]
fn stl_corners_are_joined_and_millimetres_become_metres() {
    let binary = mesh_import::stl(&binary_stl(&tetrahedron(1700.))).unwrap();
    assert_eq!(binary.mesh.vertices.len(), 4);
    assert_eq!(binary.mesh.triangles.len(), 4);
    assert!((bounds(&binary.mesh).1.y - 1.7).abs() < 1e-9);
    assert!(binary.warnings.iter().any(|w| w.contains("millimetres")));

    let mut ascii = String::from("solid body\n");
    for t in tetrahedron(170.) {
        ascii.push_str("facet normal 0 0 0\n outer loop\n");
        for p in t {
            ascii.push_str(&format!("  vertex {} {} {}\n", p[0], p[1], p[2]));
        }
        ascii.push_str(" endloop\nendfacet\n");
    }
    ascii.push_str("endsolid body\n");
    let ascii = mesh_import::stl(ascii.as_bytes()).unwrap();
    assert_eq!(ascii.mesh.vertices.len(), 4);
    assert!((bounds(&ascii.mesh).1.y - 1.7).abs() < 1e-9);
    assert!(ascii.warnings.iter().any(|w| w.contains("centimetres")));

    // A triangle with two corners in the same place is left out, and said so.
    let mut flat = tetrahedron(1.7);
    flat.push([[0., 0., 0.], [0., 0., 0.], [1., 1., 1.]]);
    let flat = mesh_import::stl(&binary_stl(&flat)).unwrap();
    assert_eq!(flat.mesh.triangles.len(), 4);
    assert!(flat.warnings.iter().any(|w| w.contains("1 degenerate")));
    assert!(mesh_import::stl(b"not a mesh").is_err());
}

/// A unit square pyramid: four triangles and a quad base, in a PLY of the given format.
fn ply(format: &str) -> Vec<u8> {
    let header = format!(
        "ply\nformat {format} 1.0\ncomment made by hand\nelement vertex 5\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nelement face 5\nproperty list uchar int vertex_indices\nend_header\n"
    );
    let points = [
        [0., 0., 0.],
        [1., 0., 0.],
        [1., 0., 1.],
        [0., 0., 1.],
        [0.5, 1.7, 0.5],
    ];
    let faces: [&[i32]; 5] = [
        &[0, 1, 2, 3],
        &[0, 4, 1],
        &[1, 4, 2],
        &[2, 4, 3],
        &[3, 4, 0],
    ];
    let mut out = header.into_bytes();
    if format == "ascii" {
        for p in points {
            out.extend_from_slice(format!("{} {} {} 200\n", p[0], p[1], p[2]).as_bytes());
        }
        for f in faces {
            let ids: Vec<String> = f.iter().map(|i| i.to_string()).collect();
            out.extend_from_slice(format!("{} {}\n", f.len(), ids.join(" ")).as_bytes());
        }
    } else {
        let little = format == "binary_little_endian";
        let f32b = |v: f32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let i32b = |v: i32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        for p in points {
            for c in p {
                out.extend_from_slice(&f32b(c as f32));
            }
            out.push(200);
        }
        for f in faces {
            out.push(f.len() as u8);
            for i in f {
                out.extend_from_slice(&i32b(*i));
            }
        }
    }
    out
}

#[test]
fn ply_reads_every_encoding_and_splits_polygons() {
    for format in ["ascii", "binary_little_endian", "binary_big_endian"] {
        let imported = mesh_import::ply(&ply(format)).unwrap();
        assert_eq!(imported.mesh.vertices.len(), 5, "{format}");
        assert_eq!(imported.mesh.triangles.len(), 6, "{format}");
        assert!((bounds(&imported.mesh).1.y - 1.7).abs() < 1e-6, "{format}");
        assert!(
            imported.warnings.iter().any(|w| w.contains("1 polygons")),
            "{format}"
        );
        assert!(
            imported.warnings.iter().any(|w| w.contains("metres")),
            "{format}"
        );
    }
    let mut broken = ply("binary_little_endian");
    broken.truncate(broken.len() - 3);
    assert!(mesh_import::ply(&broken).is_err());
}

#[test]
fn collada_scenes_keep_units_up_axis_transforms_and_skins() {
    // A 100 cm cube, Z up, written as quads, placed 50 cm up by its node, and the same cube
    // again through a skin whose bind shape halves it.
    let cube = r##"<?xml version="1.0" encoding="utf-8"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <asset><unit name="centimeter" meter="0.01"/><up_axis>Z_UP</up_axis></asset>
  <library_geometries>
    <geometry id="cube-mesh"><mesh>
      <source id="cube-positions">
        <float_array id="cube-array" count="24">0 0 0 100 0 0 100 100 0 0 100 0 0 0 100 100 0 100 100 100 100 0 100 100</float_array>
        <technique_common><accessor source="#cube-array" count="8" stride="3"/></technique_common>
      </source>
      <vertices id="cube-vertices"><input semantic="POSITION" source="#cube-positions"/></vertices>
      <polylist count="6">
        <input semantic="VERTEX" source="#cube-vertices" offset="0"/>
        <vcount>4 4 4 4 4 4</vcount>
        <p>0 3 2 1 4 5 6 7 0 1 5 4 1 2 6 5 2 3 7 6 3 0 4 7</p>
      </polylist>
    </mesh></geometry>
  </library_geometries>
  <library_controllers>
    <controller id="skin"><skin source="#cube-mesh">
      <bind_shape_matrix>0.5 0 0 0 0 0.5 0 0 0 0 0.5 0 0 0 0 1</bind_shape_matrix>
    </skin></controller>
  </library_controllers>
  <library_visual_scenes>
    <visual_scene id="scene">
      <node id="body"><translate>0 0 50</translate><instance_geometry url="#cube-mesh"/></node>
      <node id="rig"><node id="skinned"><instance_controller url="#skin"/></node></node>
    </visual_scene>
  </library_visual_scenes>
  <scene><instance_visual_scene url="#scene"/></scene>
</COLLADA>"##;
    let imported = mesh_import::dae(cube).unwrap();
    // Two cubes, twelve triangles each.
    assert_eq!(imported.mesh.triangles.len(), 24);
    let (lo, hi) = bounds(&imported.mesh);
    // Z up became Y up: the placed cube spans 0.5 to 1.5 m in height.
    assert!((hi.y - 1.5).abs() < 1e-9 && lo.y.abs() < 1e-9, "{lo} {hi}");
    // Z up turns +y into -z.
    assert!((lo.z + 1.).abs() < 1e-9 && hi.z.abs() < 1e-9, "{lo} {hi}");
    assert!(imported.warnings.iter().any(|w| w.contains("bind pose")));
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("6 polygons") || w.contains("12 polygons"))
    );
    assert!(mesh_import::dae("<svg/>").is_err());
}

#[test]
fn the_avatar_importer_takes_stl_ply_and_collada_files() {
    let dir = tempfile::tempdir().unwrap();
    let stl = dir.path().join("body.stl");
    std::fs::write(&stl, binary_stl(&tetrahedron(1700.))).unwrap();
    let candidate = Candidate::load(&stl).unwrap();
    assert_eq!((candidate.vertices, candidate.triangles), (4, 4));
    assert!((candidate.bounds[1].y - 1.7).abs() < 1e-9);
    let ply_path = dir.path().join("body.ply");
    std::fs::write(&ply_path, ply("ascii")).unwrap();
    assert_eq!(Candidate::load(&ply_path).unwrap().triangles, 6);
    let other = dir.path().join("body.fbx");
    std::fs::write(&other, b"x").unwrap();
    assert!(Candidate::load(&other).is_err());
}

#[test]
fn props_load_in_their_own_units_and_travel_with_the_project() {
    use marvelous_cloth::{
        fitting::{FittingProject, SceneObject},
        garment::presets::GarmentKind,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stool.stl");
    std::fs::write(&path, binary_stl(&tetrahedron(450.))).unwrap();
    // As it is: no unit guess for props.
    let raw = mesh_import::load(&path).unwrap();
    assert!((bounds(&raw).1.y - 450.).abs() < 1e-9);
    let mut project = FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    );
    project.scene_objects.push(SceneObject {
        id: "object_1".into(),
        name: "Stool".into(),
        path: path.clone(),
        translation: [0.6, 0., 0.],
        rotation_degrees: [0.; 3],
        scale: 0.001,
        keys: vec![],
    });
    let placed = project.scene_objects[0].mesh().unwrap();
    assert!((bounds(&placed).1.y - 0.45).abs() < 1e-9);
    let saved = dir.path().join("p.json");
    project.save(&saved).unwrap();
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(
        text.contains("p.json.assets/object-") && text.contains(".stl"),
        "{text}"
    );
    let opened = FittingProject::load(&saved).unwrap();
    assert!((bounds(&opened.scene_objects[0].mesh().unwrap()).1.y - 0.45).abs() < 1e-9);
    assert!(mesh_import::load(&dir.path().join("x.fbx")).is_err());
}
