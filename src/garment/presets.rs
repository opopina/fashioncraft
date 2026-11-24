//! Independently drafted engineering fixtures, not production sewing patterns.
use super::{
    GarmentTemplate,
    polygon::PolygonPattern,
    seam::{BoundaryRef, SeamSpec, Side},
    template::{Fabric, FabricProperties, Panel, Placement},
};
use crate::model::Pattern;
use clap::ValueEnum;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GarmentKind {
    Tshirt,
    Dress,
}
impl GarmentKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Tshirt => "tshirt",
            Self::Dress => "dress",
        }
    }
}
/// A garment with no pieces yet, the start of an empty project. It keeps the fixtures' fabric,
/// so the first piece drawn has one to be cut from.
pub fn empty() -> GarmentTemplate {
    let mut template = basic(GarmentKind::Tshirt, 0);
    template.name = "Untitled".into();
    template.panels.clear();
    template.seams.clear();
    template
}

pub fn basic(kind: GarmentKind, subdivisions: u32) -> GarmentTemplate {
    let (points, edges, sewn, height): (Vec<[f64; 2]>, Vec<&str>, Vec<usize>, f64) = match kind {
        GarmentKind::Tshirt => (
            vec![
                [0.42, 0.0],
                [0.46, 0.065],
                [0.54, 0.065],
                [0.58, 0.0],
                [0.72, 0.04],
                [0.98, 0.24],
                [0.84, 0.40],
                [0.72, 0.29],
                [0.78, 0.66],
                [0.22, 0.66],
                [0.28, 0.29],
                [0.16, 0.40],
                [0.02, 0.24],
                [0.28, 0.04],
            ],
            vec![
                "neck-left",
                "neck-bottom",
                "neck-right",
                "shoulder-right",
                "sleeve-top-right",
                "cuff-right",
                "sleeve-under-right",
                "side-right",
                "hem",
                "side-left",
                "sleeve-under-left",
                "cuff-left",
                "sleeve-top-left",
                "shoulder-left",
            ],
            vec![3, 4, 6, 7, 9, 10, 12, 13],
            0.66,
        ),
        GarmentKind::Dress => (
            vec![
                [0.42, 0.0],
                [0.46, 0.065],
                [0.54, 0.065],
                [0.58, 0.0],
                [0.72, 0.035],
                [0.74, 0.23],
                [0.94, 1.08],
                [0.06, 1.08],
                [0.26, 0.23],
                [0.28, 0.035],
            ],
            vec![
                "neck-left",
                "neck-bottom",
                "neck-right",
                "shoulder-right",
                "armhole-right",
                "side-right",
                "hem",
                "side-left",
                "armhole-left",
                "shoulder-left",
            ],
            vec![3, 5, 7, 9],
            1.08,
        ),
    };
    let polygon = PolygonPattern {
        holes: vec![],
        lines: vec![],
        curves: Default::default(),
        target_lengths: Default::default(),
        seam_allowances: Default::default(),
        notches: Default::default(),
        points: points.into_iter().map(DVec2::from_array).collect(),
        edges: edges.iter().map(|s| s.to_string()).collect(),
        subdivisions,
        meshing: Some(super::meshing::MeshingSettings {
            target_edge_m: 0.1 / 2.0_f64.powi(subdivisions as i32),
            ..Default::default()
        }),
    };
    let panels = [("front", 0.38), ("back", -0.15)]
        .into_iter()
        .map(|(id, z)| Panel {
            id: id.into(),
            name: format!("{} {id}", kind.name()),
            pattern: Pattern {
                width: 1.0,
                height,
                nx: 1,
                ny: 1,
            },
            polygon: Some(polygon.clone()),
            fabric: "woven-reference".into(),
            grain_degrees: 90.0,
            placement: Placement {
                translation: DVec3::new(-0.5, 1.50, z),
                ..Default::default()
            },
            pins: vec![],
            pin_points: vec![],
            layout_2d: None,
            layout_rotation_degrees: 0.,
        })
        .collect();
    let seams = sewn
        .into_iter()
        .map(|i| SeamSpec {
            a_tail: vec![],
            b_tail: vec![],
            id: edges[i].into(),
            a: BoundaryRef {
                reversed: false,
                panel: "front".into(),
                side: Side::Top,
                edge: Some(edges[i].into()),
                range: [0.0, 1.0],
            },
            b: BoundaryRef {
                reversed: false,
                panel: "back".into(),
                side: Side::Top,
                edge: Some(edges[i].into()),
                range: [0.0, 1.0],
            },
            reverse_b: false,
            notches: vec![],
            stiffness: super::seam::DEFAULT_SEAM_STIFFNESS,
            allow_ease: false,
        })
        .collect();
    GarmentTemplate {
        format: super::template::TEMPLATE_FORMAT.into(),
        mirror_links: vec![],
        pin_targets: vec![],
        frozen_panels: vec![],
        deactivated_panels: vec![],
        flipped_panels: Default::default(),
        archive: Vec::new(),
        seam_folds: Default::default(),
        seam_zippers: Default::default(),
        inactive_seams: Default::default(),
        seam_tears: Default::default(),
        seam_strengths: Default::default(),
        texture_offsets: Default::default(),
        uv_layout: Default::default(),
        steam: vec![],
        tacks: vec![],
        line_folds: vec![],
        line_springs: vec![],
        panel_shrink: Default::default(),
        panel_pressure: Default::default(),
        strengthened_panels: Default::default(),
        solidified_panels: vec![],
        panel_layers: Default::default(),
        panel_wraps: Default::default(),
        locked_panels: Default::default(),
        internal_lines_locked: false,
        pins_attached: false,
        grading: None,
        version: 2,
        name: format!("{} / MakeHuman 1.75 m engineering fixture", kind.name()),
        fabric_appearance: Default::default(),
        graphics: vec![],
        trims: vec![],
        fabrics: vec![Fabric {
            id: "woven-reference".into(),
            name: "Uncalibrated woven reference".into(),
            properties: FabricProperties::default(),
        }],
        panels,
        seams,
    }
}

/// Measurements a T-shirt is drafted from, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TshirtMeasurements {
    /// Bust girth of the body it is for.
    pub bust_m: f64,
    /// Hip girth of the body it is for.
    pub hip_m: f64,
    /// Added to both girths: how loose the shirt is.
    pub ease_m: f64,
    /// From the shoulder line to the hem.
    pub length_m: f64,
    /// Across the shoulders, seam to seam.
    pub shoulder_m: f64,
    /// Along the top of the sleeve.
    pub sleeve_m: f64,
    /// Around the sleeve's opening.
    pub cuff_m: f64,
    /// Height of the shoulder line above the floor, to place the pieces at.
    pub top_m: f64,
}

impl Default for TshirtMeasurements {
    /// Close to the bundled T-shirt on the 1.75 m mannequin.
    fn default() -> Self {
        Self {
            bust_m: 0.88,
            hip_m: 1.02,
            ease_m: 0.10,
            length_m: 0.66,
            shoulder_m: 0.44,
            sleeve_m: 0.33,
            cuff_m: 0.43,
            top_m: 1.50,
        }
    }
}

impl TshirtMeasurements {
    pub fn validate(&self) -> Result<(), String> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.bust_m, 0.5, 2.0)
            || !within(self.hip_m, 0.5, 2.2)
            || !within(self.ease_m, 0.0, 0.4)
            || !within(self.length_m, 0.3, 1.2)
            || !within(self.shoulder_m, 0.25, 0.7)
            || !within(self.sleeve_m, 0.08, 0.8)
            || !within(self.cuff_m, 0.2, 0.8)
            || !within(self.top_m, 0.5, 2.5)
        {
            return Err("T-shirt measurements are out of range".into());
        }
        Ok(())
    }
}

/// A T-shirt drafted from `m`: a front and a back of the same outline, kimono sleeves, sewn
/// at the shoulders, along the sleeves and down the sides, like the bundled one.
pub fn tshirt(m: &TshirtMeasurements, subdivisions: u32) -> Result<GarmentTemplate, String> {
    m.validate()?;
    let cx = 0.5;
    let chest = (m.bust_m + m.ease_m) / 4.;
    let hem = ((m.hip_m + m.ease_m) / 4.).max(chest);
    let shoulder = m.shoulder_m / 2.;
    if shoulder >= chest + 0.12 {
        return Err("The shoulders are much wider than the chest".into());
    }
    let neck = (0.08 * m.shoulder_m / 0.44).min(shoulder - 0.04);
    let (neck_depth, drop) = (0.065, 0.04);
    let armhole = ((m.bust_m + m.ease_m) * 0.26).max(0.16);
    // The sleeve slopes down and out like the bundled shirt's.
    let along = DVec2::new(0.26, 0.20).normalize();
    let across = DVec2::new(-along.y, along.x);
    let side = |sign: f64| {
        let shoulder_at = DVec2::new(cx + sign * shoulder, drop);
        let cuff_top = shoulder_at + DVec2::new(sign * along.x, along.y) * m.sleeve_m;
        let cuff_bottom = cuff_top + DVec2::new(sign * across.x, across.y) * (m.cuff_m / 2.);
        let underarm = DVec2::new(cx + sign * chest, drop + armhole);
        (shoulder_at, cuff_top, cuff_bottom, underarm)
    };
    let (rs, rt, rb, ru) = side(1.);
    let (ls, lt, lb, lu) = side(-1.);
    if (rb.x - ru.x) < 0.02 {
        return Err("The sleeve is too short for this armhole".into());
    }
    let points: Vec<[f64; 2]> = vec![
        [cx - neck, 0.0],
        [cx - neck / 2., neck_depth],
        [cx + neck / 2., neck_depth],
        [cx + neck, 0.0],
        rs.to_array(),
        rt.to_array(),
        rb.to_array(),
        ru.to_array(),
        [cx + hem, m.length_m],
        [cx - hem, m.length_m],
        lu.to_array(),
        lb.to_array(),
        lt.to_array(),
        ls.to_array(),
    ];
    // The outline starts at x = 0 and its centre line stays over the avatar's.
    let (left, right) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[0]), hi.max(p[0]))
        });
    let shift = 0.01 - left;
    let mut template = basic(GarmentKind::Tshirt, subdivisions);
    for panel in &mut template.panels {
        if let Some(polygon) = panel.polygon.as_mut() {
            polygon.points = points
                .iter()
                .map(|p| DVec2::new(p[0] + shift, p[1]))
                .collect();
        }
        panel.pattern.width = right + shift + 0.01;
        panel.pattern.height = m.length_m;
        panel.placement.translation.x = -(cx + shift);
        panel.placement.translation.y = m.top_m;
    }
    template.name = format!(
        "T-shirt for a {:.0} cm bust, {:.0} cm hip, {:.0} cm ease",
        m.bust_m * 100.,
        m.hip_m * 100.,
        m.ease_m * 100.
    );
    template.compile()?;
    Ok(template)
}

/// Measurements a skirt is drafted from, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkirtMeasurements {
    /// Waist girth of the body it is for.
    pub waist_m: f64,
    /// Hip girth of the body it is for.
    pub hip_m: f64,
    /// Added to both girths: how loose the skirt is.
    pub ease_m: f64,
    /// From the waist down to the widest hip.
    pub hip_depth_m: f64,
    /// From the waist to the hem.
    pub length_m: f64,
    /// How much wider the hem is than the hip, all round: 0 is a straight skirt.
    pub flare_m: f64,
    /// Height of the waist above the floor, to place the pieces at.
    pub waist_height_m: f64,
}

impl Default for SkirtMeasurements {
    /// A knee-length A-line skirt for the 1.75 m mannequin.
    fn default() -> Self {
        Self {
            waist_m: 0.74,
            hip_m: 0.98,
            ease_m: 0.04,
            hip_depth_m: 0.18,
            length_m: 0.55,
            flare_m: 0.24,
            waist_height_m: 1.06,
        }
    }
}

impl SkirtMeasurements {
    pub fn validate(&self) -> Result<(), String> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.waist_m, 0.4, 2.0)
            || !within(self.hip_m, 0.5, 2.2)
            || !within(self.ease_m, 0.0, 0.4)
            || !within(self.hip_depth_m, 0.08, 0.4)
            || !within(self.length_m, 0.2, 1.3)
            || !within(self.flare_m, 0.0, 2.0)
            || !within(self.waist_height_m, 0.4, 2.0)
        {
            return Err("Skirt measurements are out of range".into());
        }
        if self.waist_m > self.hip_m {
            return Err("The waist is wider than the hip".into());
        }
        if self.length_m < self.hip_depth_m + 0.05 {
            return Err("The skirt must reach below the hip".into());
        }
        if self.length_m > self.waist_height_m - 0.02 {
            return Err("The skirt would reach the floor".into());
        }
        Ok(())
    }
}

/// A skirt drafted from `m`: a front and a back of the same outline, from the waist out to
/// the hip and on down to a hem as much wider as the flare asks, sewn down both sides. The
/// waist is left open; its girth with the ease keeps the skirt up on the body.
pub fn skirt(m: &SkirtMeasurements, subdivisions: u32) -> Result<GarmentTemplate, String> {
    m.validate()?;
    let cx = 0.5;
    let waist = (m.waist_m + m.ease_m) / 4.;
    let hip = (m.hip_m + m.ease_m) / 4.;
    let hem = hip + m.flare_m / 4.;
    let points = [
        [cx - waist, 0.0],
        [cx + waist, 0.0],
        [cx + hip, m.hip_depth_m],
        [cx + hem, m.length_m],
        [cx - hem, m.length_m],
        [cx - hip, m.hip_depth_m],
    ];
    let edges = [
        "waist",
        "hip-right",
        "side-right",
        "hem",
        "side-left",
        "hip-left",
    ];
    let sewn = [1, 2, 4, 5];
    let shift = 0.01 - (cx - hem);
    let mut template = basic(GarmentKind::Tshirt, subdivisions);
    let polygon = template.panels[0].polygon.clone().map(|mut p| {
        p.points = points
            .iter()
            .map(|q| DVec2::new(q[0] + shift, q[1]))
            .collect();
        p.edges = edges.iter().map(|e| e.to_string()).collect();
        p
    });
    for (panel, z) in template.panels.iter_mut().zip([0.30, -0.22]) {
        panel.name = format!("skirt {}", panel.id);
        panel.polygon = polygon.clone();
        panel.pattern.width = 2. * hem + 0.02;
        panel.pattern.height = m.length_m;
        panel.placement.translation = DVec3::new(-(cx + shift), m.waist_height_m, z);
    }
    template.seams = sewn
        .iter()
        .map(|&i| {
            let mut seam = template.seams[0].clone();
            seam.id = edges[i].into();
            seam.a.edge = Some(edges[i].into());
            seam.b.edge = Some(edges[i].into());
            seam
        })
        .collect();
    template.name = format!(
        "Skirt for a {:.0} cm waist, {:.0} cm hip, {:.0} cm long",
        m.waist_m * 100.,
        m.hip_m * 100.,
        m.length_m * 100.
    );
    template.compile()?;
    Ok(template)
}

/// Measurements trousers are drafted from, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrousersMeasurements {
    /// Waist girth of the body they are for.
    pub waist_m: f64,
    /// Hip girth of the body they are for.
    pub hip_m: f64,
    /// Added to both girths: how loose the trousers are.
    pub ease_m: f64,
    /// From the waist down to the widest hip.
    pub hip_depth_m: f64,
    /// From the waist down to the crotch.
    pub rise_m: f64,
    /// From the waist to the hem, down the side.
    pub length_m: f64,
    /// Around each leg's opening.
    pub hem_m: f64,
    /// Height of the waist above the floor, to place the pieces at.
    pub waist_height_m: f64,
}

impl Default for TrousersMeasurements {
    /// Ankle-length trousers for the 1.75 m mannequin.
    fn default() -> Self {
        Self {
            waist_m: 0.74,
            hip_m: 0.98,
            ease_m: 0.06,
            hip_depth_m: 0.18,
            rise_m: 0.27,
            length_m: 0.96,
            hem_m: 0.44,
            waist_height_m: 1.06,
        }
    }
}

impl TrousersMeasurements {
    pub fn validate(&self) -> Result<(), String> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.waist_m, 0.4, 2.0)
            || !within(self.hip_m, 0.5, 2.2)
            || !within(self.ease_m, 0.0, 0.4)
            || !within(self.hip_depth_m, 0.08, 0.4)
            || !within(self.rise_m, 0.15, 0.5)
            || !within(self.length_m, 0.3, 1.4)
            || !within(self.hem_m, 0.2, 1.2)
            || !within(self.waist_height_m, 0.4, 2.0)
        {
            return Err("Trouser measurements are out of range".into());
        }
        if self.waist_m > self.hip_m {
            return Err("The waist is wider than the hip".into());
        }
        if self.rise_m < self.hip_depth_m + 0.04 {
            return Err("The crotch must be below the hip".into());
        }
        if self.length_m < self.rise_m + 0.15 {
            return Err("The legs are too short for this rise".into());
        }
        if self.length_m > self.waist_height_m - 0.02 {
            return Err("The trousers would reach the floor".into());
        }
        Ok(())
    }
}

/// Trousers drafted from `m`: a front and a back piece for each leg. Each piece runs from the
/// waist out to the hip and down the side to the hem; on the inside it comes down the centre
/// to the hip, slants out to the crotch point (further out at the back) and goes down the
/// inside leg to the hem. The sides and inside legs sew each leg's front to its back, the centre and crotch
/// edges sew the two fronts together and the two backs together. The waist is left open.
pub fn trousers(m: &TrousersMeasurements, subdivisions: u32) -> Result<GarmentTemplate, String> {
    m.validate()?;
    let waist = (m.waist_m + m.ease_m) / 4.;
    let hip = (m.hip_m + m.ease_m) / 4.;
    let inset = (hip - waist) / 2.;
    let half_hem = m.hem_m / 4.;
    let front_reach = (m.hip_m + m.ease_m) / 16.;
    let back_reach = (m.hip_m + m.ease_m) / 10.;
    // The leg's middle, the same on the front and the back so their sides match.
    let leg = (hip + front_reach) / 2.;
    // A right-leg piece seen from outside, the side seam on the left: waist, centre, crotch,
    // inside leg, hem, lower and upper side.
    let edges = [
        "waist",
        "centre",
        "crotch",
        "inseam",
        "hem",
        "outseam-lower",
        "outseam-upper",
    ];
    let right = |reach: f64| -> Vec<DVec2> {
        vec![
            DVec2::new(inset, 0.),
            DVec2::new(hip - inset, 0.),
            DVec2::new(hip, m.hip_depth_m),
            DVec2::new(hip + reach, m.rise_m),
            DVec2::new(leg + half_hem, m.length_m),
            DVec2::new(leg - half_hem, m.length_m),
            DVec2::new(0., m.hip_depth_m),
        ]
    };
    if leg - half_hem < 0.01 {
        return Err("The leg opening is wider than the leg".into());
    }
    // The left leg is the mirror image, its points in reverse so the outline turns the same
    // way; edge k of the mirror runs backwards along edge `MIRRORED[k]` of the right leg.
    const MIRRORED: [usize; 7] = [0, 6, 5, 4, 3, 2, 1];
    let left = |points: &[DVec2]| -> Vec<DVec2> {
        let width = points.iter().map(|p| p.x).fold(0., f64::max);
        [1, 0, 6, 5, 4, 3, 2]
            .iter()
            .map(|&k| DVec2::new(width - points[k].x, points[k].y))
            .collect()
    };
    let mut template = basic(GarmentKind::Tshirt, subdivisions);
    let base = template.panels[0].clone();
    let base_polygon = base
        .polygon
        .clone()
        .ok_or("The base piece has no outline")?;
    let gap = 0.012;
    let mut panels = vec![];
    for (id, reach, z, mirrored) in [
        ("front-right", front_reach, 0.30, false),
        ("front-left", front_reach, 0.30, true),
        ("back-right", back_reach, -0.22, false),
        ("back-left", back_reach, -0.22, true),
    ] {
        let outline = right(reach);
        let width = outline.iter().map(|p| p.x).fold(0., f64::max);
        let (points, names): (Vec<DVec2>, Vec<String>) = if mirrored {
            (
                left(&outline),
                MIRRORED.iter().map(|&k| edges[k].to_string()).collect(),
            )
        } else {
            (outline, edges.iter().map(|e| e.to_string()).collect())
        };
        // The crotch point sits just off the body's middle.
        let crotch = if mirrored {
            width - (hip + reach)
        } else {
            hip + reach
        };
        let x = if mirrored {
            gap - crotch
        } else {
            -gap - crotch
        };
        let mut panel = base.clone();
        panel.id = id.into();
        panel.name = format!("trousers {id}");
        let mut polygon = base_polygon.clone();
        polygon.points = points;
        polygon.edges = names;
        panel.polygon = Some(polygon);
        panel.pattern.width = width + 0.001;
        panel.pattern.height = m.length_m;
        panel.placement.translation = DVec3::new(x, m.waist_height_m, z);
        panels.push(panel);
    }
    let seam_template = template.seams[0].clone();
    let seam = |id: &str, a: &str, b: &str, edge: &str, reverse: bool, ease: bool| {
        let mut s = seam_template.clone();
        s.id = id.into();
        s.a.panel = a.into();
        s.a.edge = Some(edge.into());
        s.b.panel = b.into();
        s.b.edge = Some(edge.into());
        s.reverse_b = reverse;
        s.allow_ease = ease;
        s
    };
    let mut seams = vec![];
    for side in ["right", "left"] {
        let (front, back) = (format!("front-{side}"), format!("back-{side}"));
        for edge in ["outseam-upper", "outseam-lower"] {
            seams.push(seam(
                &format!("{edge}-{side}"),
                &front,
                &back,
                edge,
                false,
                false,
            ));
        }
        seams.push(seam(
            &format!("inseam-{side}"),
            &front,
            &back,
            "inseam",
            false,
            true,
        ));
    }
    for part in ["front", "back"] {
        let (right, left) = (format!("{part}-right"), format!("{part}-left"));
        for edge in ["centre", "crotch"] {
            seams.push(seam(
                &format!("{edge}-{part}"),
                &right,
                &left,
                edge,
                true,
                false,
            ));
        }
    }
    template.panels = panels;
    template.seams = seams;
    template.name = format!(
        "Trousers for a {:.0} cm waist, {:.0} cm hip, {:.0} cm long",
        m.waist_m * 100.,
        m.hip_m * 100.,
        m.length_m * 100.
    );
    template.compile()?;
    Ok(template)
}

/// Measurements a sleeveless dress is drafted from, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DressMeasurements {
    pub bust_m: f64,
    pub hip_m: f64,
    /// Added to both girths.
    pub ease_m: f64,
    /// From the shoulder line to the hem.
    pub length_m: f64,
    /// Across the shoulders, seam to seam.
    pub shoulder_m: f64,
    /// From the shoulder line down to the hip line.
    pub hip_depth_m: f64,
    /// Added to the hem's girth beyond what the hip needs: how much the skirt flares.
    pub flare_m: f64,
    /// Height of the shoulder line above the floor, to place the pieces at.
    pub top_m: f64,
}
impl Default for DressMeasurements {
    /// A knee-length A-line dress for the 1.75 m mannequin.
    fn default() -> Self {
        Self {
            bust_m: 0.88,
            hip_m: 1.02,
            ease_m: 0.08,
            length_m: 0.95,
            shoulder_m: 0.38,
            hip_depth_m: 0.56,
            flare_m: 0.30,
            top_m: 1.50,
        }
    }
}
impl DressMeasurements {
    pub fn validate(&self) -> Result<(), String> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.bust_m, 0.5, 2.0)
            || !within(self.hip_m, 0.5, 2.2)
            || !within(self.ease_m, 0.0, 0.4)
            || !within(self.length_m, 0.5, 1.6)
            || !within(self.shoulder_m, 0.25, 0.7)
            || !within(self.hip_depth_m, 0.3, 1.0)
            || !within(self.flare_m, 0.0, 2.0)
            || !within(self.top_m, 0.5, 2.5)
        {
            return Err("Dress measurements are out of range".into());
        }
        if self.hip_depth_m + 0.05 > self.length_m {
            return Err("The dress must reach below the hip line".into());
        }
        Ok(())
    }
}

/// The outline shared by the dress and the shirt's bodice: neck, shoulders, armholes down to
/// the underarm, sides straight to the hem. Ten points in the order of
/// [`GarmentKind::Dress`]'s segments, starting at x = 0.01.
fn bodice_outline(chest: f64, shoulder: f64, hem: f64, armhole: f64, length: f64) -> Vec<DVec2> {
    let cx = 0.5;
    let neck = (0.08 * shoulder / 0.22).min(shoulder - 0.04);
    let (neck_depth, drop) = (0.065, 0.04);
    let points = [
        [cx - neck, 0.0],
        [cx - neck / 2., neck_depth],
        [cx + neck / 2., neck_depth],
        [cx + neck, 0.0],
        [cx + shoulder, drop],
        [cx + chest, drop + armhole],
        [cx + hem, length],
        [cx - hem, length],
        [cx - chest, drop + armhole],
        [cx - shoulder, drop],
    ];
    let left = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    points
        .iter()
        .map(|p| DVec2::new(p[0] + 0.01 - left, p[1]))
        .collect()
}

/// A front and a back cut from `outline`, sewn at the shoulders and sides like
/// [`GarmentKind::Dress`], centred over the avatar with the shoulder line at `top_m`.
fn bodice(outline: Vec<DVec2>, length: f64, top_m: f64, subdivisions: u32) -> GarmentTemplate {
    let mut template = basic(GarmentKind::Dress, subdivisions);
    let right = outline
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let centre = (outline[3].x + outline[0].x) / 2.;
    for panel in &mut template.panels {
        if let Some(polygon) = panel.polygon.as_mut() {
            polygon.points = outline.clone();
        }
        panel.pattern.width = right + 0.01;
        panel.pattern.height = length;
        panel.placement.translation.x = -centre;
        panel.placement.translation.y = top_m;
    }
    template
}

/// A sleeveless A-line dress drafted from `m`: a front and a back of the same outline, sewn
/// at the shoulders and down the sides. The side runs straight from the underarm to the hem,
/// wide enough to pass the hip, plus the flare.
pub fn dress(m: &DressMeasurements, subdivisions: u32) -> Result<GarmentTemplate, String> {
    m.validate()?;
    let chest = (m.bust_m + m.ease_m) / 4.;
    let hip = (m.hip_m + m.ease_m) / 4.;
    let shoulder = m.shoulder_m / 2.;
    if shoulder >= chest + 0.05 {
        return Err("The shoulders are much wider than the chest".into());
    }
    let armhole = ((m.bust_m + m.ease_m) * 0.22).max(0.16);
    let underarm = 0.04 + armhole;
    if m.hip_depth_m <= underarm + 0.05 {
        return Err("The hip line is too close to the underarm".into());
    }
    // The straight side must clear the hip where it crosses the hip line.
    let through_hip =
        chest + (hip - chest).max(0.) * (m.length_m - underarm) / (m.hip_depth_m - underarm);
    let hem = through_hip.max(chest) + m.flare_m / 4.;
    let outline = bodice_outline(chest, shoulder, hem, armhole, m.length_m);
    let mut template = bodice(outline, m.length_m, m.top_m, subdivisions);
    for panel in &mut template.panels {
        panel.name = format!("dress {}", panel.id);
    }
    template.name = format!(
        "Dress for a {:.0} cm bust, {:.0} cm hip, {:.0} cm long",
        m.bust_m * 100.,
        m.hip_m * 100.,
        m.length_m * 100.
    );
    template.compile()?;
    Ok(template)
}

/// Measurements a shirt with set-in sleeves is drafted from, metres, and where the avatar's
/// arms are so the sleeves start around them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShirtMeasurements {
    pub bust_m: f64,
    pub hip_m: f64,
    /// Added to both girths.
    pub ease_m: f64,
    /// From the shoulder line to the hem.
    pub length_m: f64,
    /// Across the shoulders, seam to seam.
    pub shoulder_m: f64,
    /// Girth of the upper arm; the sleeve gets the ease on top.
    pub upper_arm_m: f64,
    /// From the top of the sleeve cap to the hem of the sleeve.
    pub sleeve_m: f64,
    /// Around the sleeve's hem.
    pub cuff_m: f64,
    /// Height of the shoulder line above the floor, to place the pieces at.
    pub top_m: f64,
    /// Shoulder and elbow joints of the arm on the +x side and of the arm on the −x side,
    /// metres: each sleeve starts wrapped around its arm.
    pub arms: [[DVec3; 2]; 2],
}
impl Default for ShirtMeasurements {
    /// A short-sleeved blouse for the 1.75 m mannequin, whose joints these are.
    fn default() -> Self {
        let shoulder = DVec3::new(0.176, 1.409, 0.015);
        let elbow = DVec3::new(0.329, 1.225, 0.014);
        let mirror = |p: DVec3| DVec3::new(-p.x, p.y, p.z);
        Self {
            bust_m: 0.88,
            hip_m: 1.02,
            ease_m: 0.10,
            length_m: 0.66,
            shoulder_m: 0.40,
            upper_arm_m: 0.29,
            sleeve_m: 0.25,
            cuff_m: 0.32,
            top_m: 1.50,
            arms: [[shoulder, elbow], [mirror(shoulder), mirror(elbow)]],
        }
    }
}
impl ShirtMeasurements {
    pub fn validate(&self) -> Result<(), String> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.bust_m, 0.5, 2.0)
            || !within(self.hip_m, 0.5, 2.2)
            || !within(self.ease_m, 0.0, 0.4)
            || !within(self.length_m, 0.3, 1.2)
            || !within(self.shoulder_m, 0.25, 0.7)
            || !within(self.upper_arm_m, 0.15, 0.6)
            || !within(self.sleeve_m, 0.12, 0.8)
            || !within(self.cuff_m, 0.15, 0.6)
            || !within(self.top_m, 0.5, 2.5)
        {
            return Err("Shirt measurements are out of range".into());
        }
        for [shoulder, elbow] in self.arms {
            if !shoulder.is_finite() || !elbow.is_finite() || shoulder.distance(elbow) < 0.1 {
                return Err("The arm joints are missing or too close together".into());
            }
        }
        Ok(())
    }
}

/// How far around its arm a sleeve is wrapped when it starts: nearly a full turn, the two
/// underarm edges left a little apart for their seam to close.
const SLEEVE_WRAP_DEGREES: f64 = 165.;
/// Distance the sleeve keeps from the arm's axis at the least, metres.
const SLEEVE_RADIUS_M: f64 = 0.075;
/// Distance down the arm from the shoulder joint where a sleeve may start, metres, nearest
/// first; and how much its tube may be widened to clear the body. With the body known, the
/// first pair that keeps every point of the sleeve [`SLEEVE_CLEARANCE_M`] from it is taken.
const SLEEVE_STARTS_M: [f64; 5] = [0.02, 0.05, 0.08, 0.11, 0.14];
const SLEEVE_GROWTH: [f64; 3] = [1.0, 1.15, 1.3];
const SLEEVE_CLEARANCE_M: f64 = 0.015;

/// A shirt with set-in sleeves drafted from `m`: a front and a back like the dress's bodice,
/// cut to the hip, and two sleeves whose caps are eased into the armholes. The armholes and
/// caps are straight lines, a simplified block rather than a tailor's curve. Each sleeve
/// starts wrapped around its arm, and each half of its cap is sewn to the armhole of the
/// piece it faces.
pub fn shirt(m: &ShirtMeasurements, subdivisions: u32) -> Result<GarmentTemplate, String> {
    shirt_around(m, subdivisions, None)
}

/// [`shirt`], with each sleeve started clear of `body` (the avatar's vertices, as posed) when
/// it is known. Without it the sleeves start where they clear the 1.75 m mannequin.
/// Where a sleeve `width` wide and `length` long starts around an arm (`arm`: its shoulder and
/// elbow joints; `plus_x`: the arm on the +x side): wrapped into a tube whose axis runs down
/// the arm just below the shoulder, the cap's middle up and away from the body, the
/// underarm towards it, as close to the shoulder and as narrow as `body` allows (searched in
/// steps when a body is given). The sleeve's outline lies in x 0..`width`, y 0..`length`,
/// cap at the top.
pub fn sleeve_on_arm(
    arm: [DVec3; 2],
    plus_x: bool,
    width: f64,
    length: f64,
    body: Option<&[DVec3]>,
) -> Result<(Placement, super::template::PanelWrap), String> {
    use super::template::PanelWrap;
    let radius = (width / 2. / SLEEVE_WRAP_DEGREES.to_radians()).max(SLEEVE_RADIUS_M);
    let wrapped = |radius: f64| PanelWrap {
        axis_x_m: width / 2.,
        radius_m: radius,
        max_angle_degrees: (width / 2. / radius).to_degrees().min(SLEEVE_WRAP_DEGREES),
        half_width_m: None,
    };
    let [joint, elbow] = arm;
    let down = (elbow - joint).normalize();
    // The cap's top faces up and away from the body, the underarm towards it.
    let outward = DVec3::new(if plus_x { 1. } else { -1. }, 0., 0.);
    let up = (outward - down * outward.dot(down)).normalize_or(DVec3::Y);
    let up = if up.y < 0. { -up } else { up };
    let x_axis = (-down).cross(up);
    let rotation = glam::DQuat::from_mat3(&glam::DMat3::from_cols(x_axis, -down, up));
    let (rx, ry, rz) = rotation.to_euler(glam::EulerRot::XYZ);
    let placement = Placement {
        translation: DVec3::ZERO,
        rotation_degrees: DVec3::new(rx, ry, rz) * (180. / std::f64::consts::PI),
    };
    // The wrap's axis, at the cap's level, lies on the arm below the joint: as close to the
    // shoulder as the body allows, the tube as narrow as it allows.
    let place = |start: f64, growth: f64| {
        let wrap = wrapped(radius * growth);
        let on_axis = placement.transform(DVec3::new(width / 2., 0., -wrap.radius_m));
        let placement = Placement {
            translation: joint + down * start - on_axis,
            ..placement.clone()
        };
        (placement, wrap)
    };
    let clearance = |(placement, wrap): &(Placement, PanelWrap)| -> f64 {
        let Some(body) = body else {
            return f64::INFINITY;
        };
        let mut nearest = f64::INFINITY;
        for i in 0..=8 {
            for j in 0..=8 {
                let (u, v) = (width * f64::from(i) / 8., length * f64::from(j) / 8.);
                let p = placement.transform(wrap.bend(DVec3::new(u, -v, 0.)));
                nearest = body
                    .iter()
                    .map(|b| b.distance_squared(p))
                    .fold(nearest, f64::min);
            }
        }
        nearest.sqrt()
    };
    let candidates = SLEEVE_STARTS_M
        .iter()
        .flat_map(|s| SLEEVE_GROWTH.iter().map(move |g| (*s, *g)));
    if body.is_some() {
        candidates
            .map(|(s, g)| place(s, g))
            .find(|c| clearance(c) >= SLEEVE_CLEARANCE_M)
            .ok_or_else(|| "The sleeves cannot start clear of this avatar's arms".to_owned())
    } else {
        Ok(place(SLEEVE_STARTS_M[0], SLEEVE_GROWTH[2]))
    }
}

pub fn shirt_around(
    m: &ShirtMeasurements,
    subdivisions: u32,
    body: Option<&[DVec3]>,
) -> Result<GarmentTemplate, String> {
    m.validate()?;
    let chest = (m.bust_m + m.ease_m) / 4.;
    let hip = ((m.hip_m + m.ease_m) / 4.).max(chest);
    let shoulder = m.shoulder_m / 2.;
    if shoulder >= chest + 0.05 {
        return Err("The shoulders are much wider than the chest".into());
    }
    let width = m.upper_arm_m + 0.06;
    // The armhole is deep enough for a cap to rise over the sleeve's half width.
    let reach = (chest - shoulder).abs();
    let armhole = ((m.bust_m + m.ease_m) * 0.2)
        .max(0.15)
        .max(((width / 2. + 0.03).powi(2) - reach * reach).max(0.).sqrt());
    let armhole_length = (reach * reach + armhole * armhole).sqrt();
    // The cap is a centimetre longer than the armhole, eased in.
    let cap_edge = armhole_length + 0.01;
    let cap = (cap_edge * cap_edge - width * width / 4.).sqrt();
    if m.sleeve_m < cap + 0.03 {
        return Err(format!(
            "The sleeve must be at least {:.0} cm long for this armhole",
            (cap + 0.03) * 100.
        ));
    }
    let cuff = m.cuff_m.min(width);
    let outline = bodice_outline(chest, shoulder, hip, armhole, m.length_m);
    let mut template = bodice(outline, m.length_m, m.top_m, subdivisions);
    for panel in &mut template.panels {
        panel.name = format!("shirt {}", panel.id);
    }
    let sleeve = PolygonPattern {
        points: vec![
            DVec2::new(width / 2., 0.),
            DVec2::new(width, cap),
            DVec2::new(width / 2. + cuff / 2., m.sleeve_m),
            DVec2::new(width / 2. - cuff / 2., m.sleeve_m),
            DVec2::new(0., cap),
        ],
        edges: [
            "cap-right",
            "underarm-right",
            "hem",
            "underarm-left",
            "cap-left",
        ]
        .map(String::from)
        .to_vec(),
        ..template.panels[0]
            .polygon
            .clone()
            .ok_or("no bodice outline")?
    };
    let seam = |id: String, a: (&str, &str), b: (&str, &str), reverse: bool, ease: bool| {
        let side = |(panel, edge): (&str, &str)| BoundaryRef {
            reversed: false,
            panel: panel.into(),
            side: Side::Top,
            edge: Some(edge.into()),
            range: [0., 1.],
        };
        SeamSpec {
            id,
            a: side(a),
            b: side(b),
            a_tail: vec![],
            b_tail: vec![],
            reverse_b: reverse,
            notches: vec![],
            stiffness: super::seam::DEFAULT_SEAM_STIFFNESS,
            allow_ease: ease,
        }
    };
    for (k, side) in ["right", "left"].into_iter().enumerate() {
        let (placement, wrap) = sleeve_on_arm(m.arms[k], k == 0, width, m.sleeve_m, body)?;
        let id = format!("sleeve-{side}");
        // Which half of the cap faces the front decides which armhole it is sewn to.
        let world = |u: f64, v: f64| placement.transform(wrap.bend(DVec3::new(u, -v, 0.)));
        let right_half_in_front = world(width * 0.75, cap / 2.).z > world(width * 0.25, cap / 2.).z;
        let (front_half, back_half) = if right_half_in_front {
            ("cap-right", "cap-left")
        } else {
            ("cap-left", "cap-right")
        };
        let armhole_edge = format!("armhole-{side}");
        // The armhole runs shoulder → underarm on the right, underarm → shoulder on the left;
        // cap-right runs top → underarm, cap-left underarm → top.
        let reverse = |cap_half: &str| (side == "right") != (cap_half == "cap-right");
        let front = seam(
            format!("{id}-front"),
            ("front", armhole_edge.as_str()),
            (id.as_str(), front_half),
            reverse(front_half),
            true,
        );
        let back = seam(
            format!("{id}-back"),
            ("back", armhole_edge.as_str()),
            (id.as_str(), back_half),
            reverse(back_half),
            true,
        );
        // underarm-right runs underarm → hem, underarm-left hem → underarm.
        let under = seam(
            format!("{id}-underarm"),
            (id.as_str(), "underarm-right"),
            (id.as_str(), "underarm-left"),
            true,
            false,
        );
        template.seams.extend([front, back, under]);
        template.panel_wraps.insert(id.clone(), wrap);
        template.panels.push(Panel {
            id: id.clone(),
            name: format!("shirt sleeve {side}"),
            pattern: Pattern {
                width,
                height: m.sleeve_m,
                nx: 1,
                ny: 1,
            },
            polygon: Some(sleeve.clone()),
            fabric: template.panels[0].fabric.clone(),
            grain_degrees: 90.,
            placement,
            layout_2d: None,
            layout_rotation_degrees: 0.,
            pins: vec![],
            pin_points: vec![],
        });
    }
    template.name = format!(
        "Shirt with set-in sleeves for a {:.0} cm bust, {:.0} cm ease",
        m.bust_m * 100.,
        m.ease_m * 100.
    );
    template.compile()?;
    Ok(template)
}

#[cfg(test)]
mod drafted_tests {
    use super::*;

    fn length(t: &GarmentTemplate, panel: &str, edge: &str) -> f64 {
        let p = t.panels.iter().find(|p| p.id == panel).unwrap();
        crate::authoring::edge_length(p.polygon.as_ref().unwrap(), edge).unwrap()
    }

    #[test]
    fn a_dress_clears_the_hip_and_flares() {
        let m = DressMeasurements::default();
        let t = dress(&m, 1).unwrap();
        assert_eq!(t.panels.len(), 2);
        let points = &t.panels[0].polygon.as_ref().unwrap().points;
        // Half the front's hem plus half the back's is the hem's half girth.
        let hem = points[6].x - points[7].x;
        assert!(hem * 2. >= m.hip_m + m.ease_m + m.flare_m - 1e-9, "{hem}");
        assert!((points[6].y - m.length_m).abs() < 1e-12);
        assert!(dress(&DressMeasurements { length_m: 0.5, ..m }, 1).is_err());
    }

    #[test]
    fn a_shirt_eases_each_cap_half_into_the_armhole_it_faces() {
        let m = ShirtMeasurements::default();
        let t = shirt(&m, 1).unwrap();
        assert_eq!(t.panels.len(), 4);
        assert_eq!(t.seams.len(), 4 + 6);
        let cap = length(&t, "sleeve-right", "cap-right");
        let armhole = length(&t, "front", "armhole-right");
        assert!((cap - armhole - 0.01).abs() < 1e-9, "{cap} {armhole}");
        assert!(t.panel_wraps.contains_key("sleeve-left"));
        for side in ["right", "left"] {
            // The cap half sewn to the front lies in front of the one sewn to the back.
            let half = |seam: &str| {
                let s = t.seams.iter().find(|s| s.id == seam).unwrap();
                s.b.edge.clone().unwrap()
            };
            let (front, back) = (
                half(&format!("sleeve-{side}-front")),
                half(&format!("sleeve-{side}-back")),
            );
            assert_ne!(front, back);
            let panel = t
                .panels
                .iter()
                .find(|p| p.id == format!("sleeve-{side}"))
                .unwrap();
            let wrap = &t.panel_wraps[&panel.id];
            let polygon = panel.polygon.as_ref().unwrap();
            let middle = |edge: &str| {
                let i = polygon.edges.iter().position(|e| e == edge).unwrap();
                let q = polygon.points[i].lerp(polygon.points[(i + 1) % 5], 0.5);
                panel
                    .placement
                    .transform(wrap.bend(DVec3::new(q.x, -q.y, 0.)))
            };
            assert!(middle(&front).z > middle(&back).z, "{side}");
            // The sleeve hangs along the arm: its hem is lower and further out than its cap.
            let (cap_top, hem) = (middle("cap-right"), middle("hem"));
            assert!(hem.y < cap_top.y);
            assert!(hem.x.abs() > cap_top.x.abs());
        }
        assert!(
            shirt(
                &ShirtMeasurements {
                    sleeve_m: 0.12,
                    ..m
                },
                1
            )
            .is_err()
        );
    }
}
