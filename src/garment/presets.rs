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
            stiffness: 50_000.0,
            allow_ease: false,
        })
        .collect();
    GarmentTemplate {
        format: "marvelous-cloth-template".into(),
        mirror_links: vec![],
        pin_targets: vec![],
        frozen_panels: vec![],
        deactivated_panels: vec![],
        flipped_panels: Default::default(),
        archive: Vec::new(),
        seam_folds: Default::default(),
        seam_zippers: Default::default(),
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
