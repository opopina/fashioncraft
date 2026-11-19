//! Baking a garment's look into one image: each piece packed into the unit square and painted
//! with its fabric's colour, times its texture, with the front prints over it.
use fashioncraft::{
    authoring::{self, Command},
    bake::{self, Image},
    garment::{
        GarmentTemplate,
        appearance::{FabricAppearance, FabricTexture},
        graphics::{GraphicPlacement, GraphicSide},
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
};
use glam::DVec2;
use std::collections::HashMap;

/// A lone 40 × 60 cm piece of red fabric with a 10 cm blue print in its middle.
fn piece() -> (GarmentTemplate, DVec2) {
    let mut t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.4, 0.6),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.03,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let fabric = t.panels[0].fabric.clone();
    t.fabric_appearance.insert(
        fabric,
        FabricAppearance {
            color_srgb: [255, 0, 0],
            ..Default::default()
        },
    );
    let polygon = t.panels[0].polygon.as_ref().unwrap();
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    let centre = lo + DVec2::new(0.2, 0.3);
    t.graphics.push(GraphicPlacement {
        id: "logo".into(),
        panel_id: t.panels[0].id.clone(),
        asset_id: "blue".into(),
        center_m: centre,
        size_m: DVec2::splat(0.1),
        rotation_degrees: 0.,
        side: GraphicSide::Front,
        opacity: 1.,
    });
    (t, centre)
}

fn solid(rgba: [u8; 4]) -> Vec<u8> {
    rgba.repeat(16)
}

/// The atlas texel under garment vertex `v`.
fn texel(atlas: &bake::Atlas, v: usize) -> [u8; 4] {
    let p = (atlas.uv[v] * atlas.size as f64).floor();
    let k = (p.y as usize * atlas.size as usize + p.x as usize) * 4;
    atlas.rgba[k..k + 4].try_into().unwrap()
}

#[test]
fn the_fabric_colour_and_the_print_are_painted_where_they_are() {
    let (t, centre) = piece();
    let g = t.compile().unwrap();
    let blue = solid([0, 0, 255, 255]);
    let images = HashMap::from([(
        "blue".to_string(),
        Image {
            size: [4, 4],
            rgba: &blue,
        },
    )]);
    let atlas = bake::bake(&t, &g, &images, 256).unwrap();
    assert!(
        atlas
            .uv
            .iter()
            .all(|p| (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y))
    );
    // The vertex nearest the print's centre is blue; one at a corner of the piece is red.
    let nearest = |q: DVec2| {
        (0..g.mesh.uv.len())
            .min_by(|a, b| {
                g.mesh.uv[*a]
                    .distance(q)
                    .total_cmp(&g.mesh.uv[*b].distance(q))
            })
            .unwrap()
    };
    assert_eq!(texel(&atlas, nearest(centre)), [0, 0, 255, 255]);
    let corner = g.mesh.uv.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    assert_eq!(
        texel(&atlas, nearest(corner + DVec2::splat(0.01))),
        [255, 0, 0, 255]
    );
    // Half-transparent, the print lets half the fabric through (mixed in linear light).
    let mut faint = t.clone();
    faint.graphics[0].opacity = 0.5;
    let half = bake::bake(&faint, &g, &images, 256).unwrap();
    let [r, gg, b, _] = texel(&half, nearest(centre));
    assert!(r > 150 && b > 150 && gg == 0, "{r} {gg} {b}");
}

#[test]
fn the_fabric_texture_multiplies_the_colour() {
    let (mut t, _) = piece();
    t.graphics.clear();
    let fabric = t.panels[0].fabric.clone();
    let look = t.fabric_appearance.get_mut(&fabric).unwrap();
    look.color_srgb = [255, 255, 255];
    look.texture = Some(FabricTexture {
        asset_id: "green".into(),
        width_mm: 50.,
        height_mm: 50.,
        rotation_degrees: 0.,
        relief_mm: 0.,
        normal_asset_id: None,
        roughness_asset_id: None,
    });
    let g = t.compile().unwrap();
    let green = solid([0, 200, 0, 255]);
    let images = HashMap::from([(
        "green".to_string(),
        Image {
            size: [4, 4],
            rgba: &green,
        },
    )]);
    let atlas = bake::bake(&t, &g, &images, 128).unwrap();
    for v in [0, g.mesh.uv.len() / 2] {
        assert_eq!(texel(&atlas, v), [0, 200, 0, 255]);
    }
}

#[test]
fn a_png_of_the_baked_image_is_well_formed() {
    let (t, _) = piece();
    let g = t.compile().unwrap();
    let atlas = bake::bake(&t, &g, &HashMap::new(), 64).unwrap();
    let png = bake::png(64, 64, &atlas.rgba);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 64);
    assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    #[cfg(feature = "desktop")]
    {
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.as_raw(), &atlas.rgba);
    }
}

#[test]
fn bad_sizes_and_images_are_refused() {
    let (t, _) = piece();
    let g = t.compile().unwrap();
    assert!(bake::bake(&t, &g, &HashMap::new(), 8).is_err());
    let short = vec![0u8; 3];
    let images = HashMap::from([(
        "blue".to_string(),
        Image {
            size: [4, 4],
            rgba: &short,
        },
    )]);
    assert!(bake::bake(&t, &g, &images, 64).is_err());
}
