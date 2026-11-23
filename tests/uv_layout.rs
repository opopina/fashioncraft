//! UV layout: each piece's island in the texture square used by exports and the baked atlas.
//! Packing leaves no overlap and keeps its margin; edits are one undo step each, change only
//! the look metadata (never the physics, never the fabric look in the editor), survive the
//! project file, and every export and the atlas use the edited layout.
use fashioncraft::{
    abc_export::{self, AbcScene},
    authoring::{self, Command, History, only_appearance_changed, same_physics},
    bake,
    fbx_export::{self, FbxScene},
    fitting::FittingProject,
    garment::{
        GarmentTemplate,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
        uv_layout::{self, PackOptions, PlacedIsland, UvPlacement},
    },
    gltf_export::{self, GltfScene, texture_coordinates},
    obj_export::{self, ObjOptions, ObjScene},
    usd_export::{self, UsdScene},
};
use glam::DVec2;
use std::{collections::BTreeMap, path::PathBuf};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// The T-shirt with five more pieces of other shapes and sizes, among them curved ones.
fn many_pieces(template: GarmentTemplate) -> GarmentTemplate {
    let mut t = template;
    let fabric = t.panels[0].fabric.clone();
    for (k, (kind, size)) in [
        (Primitive::Rectangle, DVec2::new(0.6, 0.15)),
        (Primitive::Ellipse, DVec2::new(0.25, 0.25)),
        (Primitive::Sector, DVec2::new(0.4, 0.4)),
        (Primitive::Rectangle, DVec2::new(0.08, 0.3)),
        (Primitive::Ellipse, DVec2::new(0.12, 0.05)),
    ]
    .into_iter()
    .enumerate()
    {
        t = authoring::apply(
            &t,
            Command::Primitive {
                kind,
                size,
                sweep_degrees: 120.,
                origin: DVec2::new(2. + k as f64, 0.),
                meshing: MeshingSettings {
                    target_edge_m: 0.04,
                    ..Default::default()
                },
                fabric: fabric.clone(),
                replace: false,
            },
        )
        .unwrap()
        .template;
    }
    t
}

fn placed_islands<'a>(
    shapes: &'a [uv_layout::IslandShape],
    layout: &[UvPlacement],
) -> Vec<PlacedIsland<'a>> {
    shapes
        .iter()
        .zip(layout)
        .map(|(s, p)| PlacedIsland::new(s, p))
        .collect()
}

/// Every island inside the square at least `margin` from its edges, and no two islands
/// overlapping or closer than `margin` by their bounds.
fn check_packed(shapes: &[uv_layout::IslandShape], layout: &[UvPlacement], margin: f64) {
    let islands = placed_islands(shapes, layout);
    assert!(uv_layout::overlaps(&islands).is_empty());
    for island in &islands {
        assert!(island.lo.min_element() >= margin - 1e-9, "{}", island.lo);
        assert!(
            island.hi.max_element() <= 1. - margin + 1e-9,
            "{}",
            island.hi
        );
    }
    for i in 0..islands.len() {
        for j in i + 1..islands.len() {
            let (a, b) = (&islands[i], &islands[j]);
            let gap = (b.lo - a.hi).max(a.lo - b.hi).max_element();
            assert!(gap >= margin - 1e-9, "{i} {j}: {gap}");
        }
    }
}

#[test]
fn packing_fits_every_island_in_the_square_without_overlap() {
    let template = many_pieces(presets::basic(GarmentKind::Tshirt, 1));
    let garment = template.compile().unwrap();
    assert_eq!(garment.panels.len(), 7);
    let shapes = uv_layout::shapes(&garment);
    // The automatic layout is packed already.
    check_packed(
        &shapes,
        &uv_layout::resolve(&template, &garment),
        uv_layout::DEFAULT_MARGIN,
    );
    for rotate in [false, true] {
        for margin in [0., 0.005, 0.03] {
            let options = PackOptions {
                margin,
                rotate,
                keep_relative_scale: false,
            };
            let layout = uv_layout::pack_layout(&template, &garment, &options).unwrap();
            assert_eq!(layout.len(), 7);
            let placed: Vec<UvPlacement> = shapes.iter().map(|s| layout[&s.id]).collect();
            check_packed(&shapes, &placed, margin);
            // One texel density, and the square used well: islands' bounds cover at least a
            // third of it.
            assert!(
                placed
                    .iter()
                    .all(|p| (p.scale - placed[0].scale).abs() < 1e-9)
            );
            let covered: f64 = placed_islands(&shapes, &placed)
                .iter()
                .map(|i| (i.hi.x - i.lo.x) * (i.hi.y - i.lo.y))
                .sum();
            assert!(covered > 0.33, "{covered}");
            if !rotate {
                // Turns kept as they were.
                assert!(placed.iter().all(|p| p.rotation_degrees == 0.));
            }
        }
    }
    // Relative sizes kept when asked: one island made twice as dense stays twice as dense.
    let mut edited = uv_layout::pack_layout(&template, &garment, &PackOptions::default()).unwrap();
    edited.get_mut(&shapes[2].id).unwrap().scale *= 2.;
    let mut doubled = template.clone();
    doubled.uv_layout = edited;
    let options = PackOptions {
        keep_relative_scale: true,
        ..Default::default()
    };
    let repacked = uv_layout::pack_layout(&doubled, &garment, &options).unwrap();
    let ratio = repacked[&shapes[2].id].scale / repacked[&shapes[0].id].scale;
    assert!((ratio - 2.).abs() < 1e-9, "{ratio}");
    let placed: Vec<UvPlacement> = shapes.iter().map(|s| repacked[&s.id]).collect();
    check_packed(&shapes, &placed, uv_layout::DEFAULT_MARGIN);
}

#[test]
fn a_piece_added_after_editing_goes_into_free_space() {
    let template = presets::basic(GarmentKind::Tshirt, 1);
    let garment = template.compile().unwrap();
    let mut laid_out = template.clone();
    laid_out.uv_layout =
        uv_layout::pack_layout(&template, &garment, &PackOptions::default()).unwrap();
    // Shrink the two pieces into the top half, leaving room below.
    for p in laid_out.uv_layout.values_mut() {
        p.scale *= 0.5;
        p.center *= 0.5;
    }
    let grown = many_pieces(laid_out.clone());
    let garment = grown.compile().unwrap();
    let shapes = uv_layout::shapes(&garment);
    let layout = uv_layout::resolve(&grown, &garment);
    // The placed pieces stay; the new ones fit around them without overlap.
    for (s, p) in shapes.iter().zip(&layout) {
        if let Some(kept) = laid_out.uv_layout.get(&s.id) {
            assert_eq!(p, kept);
        }
    }
    let islands = placed_islands(&shapes, &layout);
    assert!(uv_layout::overlaps(&islands).is_empty());
    assert!(islands.iter().all(|i| i.inside_square()));
}

fn set_layout(
    template: &GarmentTemplate,
    layout: BTreeMap<String, UvPlacement>,
) -> authoring::Edit {
    authoring::apply(
        template,
        Command::SetUvLayout {
            layout,
            name: "Move UV islands",
        },
    )
    .unwrap()
}

/// An edited layout: packed, then the first piece moved, turned and scaled down.
fn edited(template: &GarmentTemplate) -> BTreeMap<String, UvPlacement> {
    let garment = template.compile().unwrap();
    let mut layout = uv_layout::pack_layout(template, &garment, &PackOptions::default()).unwrap();
    let first = layout.get_mut(&template.panels[0].id).unwrap();
    first.center += DVec2::new(0.013, -0.021);
    first.rotation_degrees = 30.;
    first.scale *= 0.8;
    layout
}

#[test]
fn a_layout_edit_is_one_undo_step_and_changes_only_the_export_look() {
    let template = presets::basic(GarmentKind::Tshirt, 1);
    let layout = edited(&template);
    let edit = set_layout(&template, layout.clone());
    assert!(!edit.reset_simulation);
    assert!(same_physics(&template, &edit.template));
    assert!(only_appearance_changed(&template, &edit.template));
    assert_eq!(edit.template.uv_layout, layout);
    // The fabric look (first texture coordinates, what the editor draws) does not move.
    assert_eq!(
        texture_coordinates(&template, &edit.garment),
        texture_coordinates(&edit.template, &edit.garment)
    );
    // The same layout again is no edit.
    assert!(
        authoring::apply(
            &edit.template,
            Command::SetUvLayout {
                layout: layout.clone(),
                name: "Move UV islands",
            },
        )
        .is_err()
    );
    // Undo puts the automatic layout back, redo the edited one.
    let mut history = History::default();
    history.record_named(template.clone(), "Move UV islands");
    assert_eq!(history.done().collect::<Vec<_>>(), vec!["Move UV islands"]);
    let undone = history.restore(&edit.template, false).unwrap();
    assert!(undone.template.uv_layout.is_empty());
    assert!(!undone.reset_simulation);
    let redone = history.restore(&undone.template, true).unwrap();
    assert_eq!(redone.template.uv_layout, layout);
    // Bad placements and unknown pieces are refused.
    let mut bad = layout.clone();
    bad.values_mut().next().unwrap().scale = 0.;
    assert!(
        authoring::apply(
            &template,
            Command::SetUvLayout {
                layout: bad,
                name: "x"
            }
        )
        .is_err()
    );
    let mut unknown = layout.clone();
    unknown.insert("nope".into(), *layout.values().next().unwrap());
    assert!(
        authoring::apply(
            &template,
            Command::SetUvLayout {
                layout: unknown,
                name: "x"
            }
        )
        .is_err()
    );
    // Deleting a piece takes its island with it.
    let removed = authoring::apply(
        &edit.template,
        Command::RemovePanel {
            panel: template.panels[0].id.clone(),
        },
    )
    .unwrap()
    .template;
    assert!(!removed.uv_layout.contains_key(&template.panels[0].id));
    assert_eq!(removed.uv_layout.len(), layout.len() - 1);
}

#[test]
fn the_layout_survives_the_project_file_and_old_files_still_open() {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let plain = serde_json::to_value(&project.template).unwrap();
    // An automatic layout is not written, so files without one stay as they were.
    assert!(plain.get("uv_layout").is_none());
    let layout = edited(&project.template);
    project.template = set_layout(&project.template, layout.clone()).template;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tee.json");
    project.save(&path).unwrap();
    let back = FittingProject::load(&path).unwrap();
    assert_eq!(back.template.uv_layout, layout);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"uv_layout\""));
    // An unknown piece in the file is refused when the garment compiles, as other per-piece
    // settings are.
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let islands = value["template"]["uv_layout"].as_object_mut().unwrap();
    let first = islands.values().next().unwrap().clone();
    islands.insert("ghost".into(), first);
    std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
    assert!(FittingProject::load(&path).unwrap().compile().is_err());
}

#[test]
fn every_export_and_the_atlas_use_the_edited_layout() {
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let layout = edited(&project.template);
    project.template = set_layout(&project.template, layout.clone()).template;
    let scene = project.compile().unwrap();
    let garment = &scene.garment;
    let expected = uv_layout::coordinates(&project.template, garment);
    // The layout's coordinates are the placements applied to the material points.
    let shapes = uv_layout::shapes(garment);
    for (panel, shape) in garment.panels.iter().zip(&shapes) {
        let p = layout[&panel.id];
        for v in panel.vertices.clone() {
            let uv = p.apply(shape.anchor, garment.mesh.uv[v]);
            assert!((uv - expected[v]).length() < 1e-12);
        }
    }
    let dir = tempfile::tempdir().unwrap();

    // Baked atlas.
    let atlas = bake::bake(&project.template, garment, &Default::default(), 64).unwrap();
    assert_eq!(atlas.uv, expected);

    // OBJ with unified coordinates: the layout, counted from the image's bottom.
    let obj = dir.path().join("tee.obj");
    let export = ObjScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: None,
    };
    let options = ObjOptions {
        unified_uv: true,
        ..Default::default()
    };
    obj_export::write(&export, &options, &obj).unwrap();
    let vt: Vec<DVec2> = std::fs::read_to_string(&obj)
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("vt "))
        .map(|l| {
            let f: Vec<f64> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            DVec2::new(f[0], f[1])
        })
        .collect();
    assert_eq!(vt.len(), expected.len());
    for (t, e) in vt.iter().zip(&expected) {
        assert!((*t - DVec2::new(e.x, 1. - e.y)).length() < 2e-6, "{t} {e}");
    }

    // glTF: the fabric's coordinates first, the layout second.
    let gltf = GltfScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        take: None,
    };
    let bytes = gltf_export::glb(&gltf).unwrap();
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    let bin = &bytes[20 + json_len + 8..];
    let floats = |accessor: u64| -> Vec<f32> {
        let a = &json["accessors"][accessor as usize];
        let view = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let start = view["byteOffset"].as_u64().unwrap() as usize;
        let len = view["byteLength"].as_u64().unwrap() as usize;
        bin[start..start + len]
            .chunks(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect()
    };
    let attributes = &json["meshes"][0]["primitives"][0]["attributes"];
    let first = floats(attributes["TEXCOORD_0"].as_u64().unwrap());
    let second = floats(attributes["TEXCOORD_1"].as_u64().unwrap());
    let fabric = texture_coordinates(&project.template, garment);
    for (k, (f, e)) in fabric.iter().zip(&expected).enumerate() {
        assert!((first[2 * k] as f64 - f.x).abs() < 1e-5);
        assert!((first[2 * k + 1] as f64 - f.y).abs() < 1e-5);
        assert!((second[2 * k] as f64 - e.x).abs() < 1e-6);
        assert!((second[2 * k + 1] as f64 - e.y).abs() < 1e-6);
    }

    // USD: `uvLayout` beside `st`, counted from the image's bottom.
    let usd = UsdScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        take: None,
    };
    let (text, _) = usd_export::usda(&usd).unwrap();
    let marker = "texCoord2f[] primvars:uvLayout = [";
    let list = &text[text.find(marker).unwrap() + marker.len()..];
    let list = &list[..list.find(']').unwrap()];
    let pairs: Vec<DVec2> = list
        .split("), (")
        .map(|p| {
            let f: Vec<f64> = p
                .trim_matches(|c| c == '(' || c == ')')
                .split(", ")
                .map(|x| x.parse().unwrap())
                .collect();
            DVec2::new(f[0], f[1])
        })
        .collect();
    let first_piece = garment.panels[0].vertices.clone();
    assert_eq!(pairs.len(), first_piece.len());
    for (p, v) in pairs.iter().zip(first_piece) {
        let e = expected[v];
        assert!((*p - DVec2::new(e.x, 1. - e.y)).length() < 2e-6);
    }

    // FBX and Alembic name the second set.
    let fbx = FbxScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        take: None,
    };
    let (bytes, _) = fbx_export::fbx(&fbx, "tee").unwrap();
    assert!(bytes.windows(8).any(|w| w == b"UVLayout"));
    let abc = dir.path().join("tee.abc");
    abc_export::write(
        &AbcScene {
            template: &project.template,
            garment,
            state: &scene.state,
            avatar: None,
            take: None,
        },
        &abc,
    )
    .unwrap();
    let bytes = std::fs::read(&abc).unwrap();
    assert!(bytes.windows(14).any(|w| w == b".arbGeomParams"));
    assert!(bytes.windows(8).any(|w| w == b"UVLayout"));
}
