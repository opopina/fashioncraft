//! Write the T-shirt preset, its UV layout packed and then edited (the front piece moved, turned
//! 30° and shrunk), as OBJ (unified UV and a baked atlas), GLB, FBX, USDA and Alembic into the
//! directory given, with `expected.json`: every exported piece's layout coordinates (v up, as
//! OBJ, FBX, USD and Alembic count it). `scripts/blender-uv-check.py` imports the files into
//! Blender and compares.
//!
//! ```text
//! cargo run --example uv_layout_export -- /tmp/uv
//! Blender -b --factory-startup --python scripts/blender-uv-check.py -- /tmp/uv
//! ```
use fashioncraft::{
    abc_export::{self, AbcScene},
    authoring::{self, Command},
    bake,
    fbx_export::{self, FbxScene},
    fitting::FittingProject,
    garment::{presets::GarmentKind, uv_layout},
    gltf_export::{self, GltfScene},
    obj_export::{self, ObjOptions, ObjScene},
    usd_export::{self, UsdScene},
};
use glam::DVec2;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Give the directory to write into")?,
    );
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let avatar = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    let garment = project.template.compile()?;
    let mut layout = uv_layout::pack_layout(
        &project.template,
        &garment,
        &uv_layout::PackOptions::default(),
    )?;
    let front = project.template.panels[0].id.clone();
    let p = layout.get_mut(&front).ok_or("No front piece")?;
    p.center += DVec2::new(0.02, -0.03);
    p.rotation_degrees = 30.;
    p.scale *= 0.7;
    project.template = authoring::apply(
        &project.template,
        Command::SetUvLayout {
            layout,
            name: "Move UV islands",
        },
    )?
    .template;
    let scene = project.compile()?;
    let garment = &scene.garment;
    let uv = uv_layout::coordinates(&project.template, garment);
    let expected: serde_json::Map<String, serde_json::Value> = garment
        .panels
        .iter()
        .map(|panel| {
            let points: Vec<[f64; 2]> = panel
                .vertices
                .clone()
                .map(|v| [uv[v].x, 1. - uv[v].y])
                .collect();
            (panel.id.clone(), serde_json::json!(points))
        })
        .collect();
    std::fs::write(
        dir.join("expected.json"),
        serde_json::to_string_pretty(&expected).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let obj = ObjScene {
        template: &project.template,
        garment,
        state: &scene.state,
        images: &[],
        avatar: None,
        atlas: None,
    };
    let unified = ObjOptions {
        unified_uv: true,
        per_piece: true,
        units: obj_export::Units::Metres,
        ..Default::default()
    };
    obj_export::write(&obj, &unified, &dir.join("garment.obj"))?;
    let atlas = bake::bake(&project.template, garment, &Default::default(), 512)?;
    std::fs::write(
        dir.join("atlas.png"),
        bake::png(atlas.size, atlas.size, &atlas.rgba),
    )
    .map_err(|e| e.to_string())?;
    obj_export::write(
        &ObjScene {
            atlas: Some(&atlas),
            ..obj
        },
        &ObjOptions {
            per_piece: true,
            units: obj_export::Units::Metres,
            ..Default::default()
        },
        &dir.join("baked.obj"),
    )?;
    gltf_export::write(
        &GltfScene {
            template: &project.template,
            garment,
            state: &scene.state,
            images: &[],
            avatar: None,
            take: None,
        },
        &dir.join("garment.glb"),
    )?;
    fbx_export::write(
        &FbxScene {
            template: &project.template,
            garment,
            state: &scene.state,
            images: &[],
            avatar: None,
            take: None,
        },
        &dir.join("garment.fbx"),
    )?;
    usd_export::write(
        &UsdScene {
            template: &project.template,
            garment,
            state: &scene.state,
            images: &[],
            avatar: None,
            take: None,
        },
        &dir.join("garment.usda"),
    )?;
    abc_export::write(
        &AbcScene {
            template: &project.template,
            garment,
            state: &scene.state,
            avatar: None,
            take: None,
        },
        &dir.join("garment.abc"),
    )?;
    println!(
        "Wrote OBJ, baked OBJ, GLB, FBX, USDA and Alembic with the edited UV layout to {}",
        dir.display()
    );
    Ok(())
}
