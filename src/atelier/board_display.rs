//! Pattern display overlays for the 2D board: piece names, outline segment lengths, the grid and
//! the avatar's front silhouette. Everything here is presentation only: it reads the template and
//! the collider surface and never writes a material coordinate, a placement or the history.
use glam::DVec2;
use marvelous_cloth::{
    authoring,
    avatar::SurfaceMesh,
    garment::{curves, polygon::PolygonPattern, template::Placement},
};
use serde::{Deserialize, Serialize};

/// What the pattern board draws besides the pieces. Stored in the preferences file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct PatternDisplay {
    /// Piece name above each piece.
    pub(super) names: bool,
    /// Rest length of every outline segment, beside the segment.
    pub(super) lengths: bool,
    /// The board grid and its scale readout.
    pub(super) grid: bool,
    /// The avatar seen from the front, behind the pieces.
    pub(super) silhouette: bool,
    /// Rulers along the top and left edges of the board.
    pub(super) rulers: bool,
    /// Guidelines (drawn and snapped to).
    pub(super) guides: bool,
    /// Unit the rulers and guide readouts use.
    pub(super) ruler_unit: super::board_guides::RulerUnit,
}

impl Default for PatternDisplay {
    fn default() -> Self {
        Self {
            names: true,
            lengths: false,
            grid: true,
            silhouette: true,
            rulers: false,
            guides: true,
            ruler_unit: Default::default(),
        }
    }
}

/// Label for one outline segment, in the piece's local material frame (metres).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SegmentLabel {
    pub(super) edge: String,
    /// Arclength midpoint of the segment.
    pub(super) at: DVec2,
    /// Unit direction pointing away from the piece at the midpoint.
    pub(super) outward: DVec2,
    /// Sampled rest length in metres, the value `authoring::edge_length` reports.
    pub(super) length_m: f64,
}

/// One label per outline segment. Lengths use the same sampling as the mesher, the seam
/// compiler and the inspector, so the board never disagrees with the length card. Holes and
/// internal lines are not labelled. A malformed outline yields no labels.
pub(super) fn segment_labels(polygon: &PolygonPattern) -> Vec<SegmentLabel> {
    // Orientation from the signed area, so a clockwise hand-edited outline still gets its
    // labels outside rather than inside.
    let area: f64 = (0..polygon.points.len())
        .map(|i| {
            let a = polygon.points[i];
            let b = polygon.points[(i + 1) % polygon.points.len()];
            a.perp_dot(b)
        })
        .sum();
    let side = if area >= 0. { 1. } else { -1. };
    let mut labels = Vec::with_capacity(polygon.edges.len());
    for (k, edge) in polygon.edges.iter().enumerate() {
        let Ok(points) = polygon.edge_points(k) else {
            return Vec::new();
        };
        let Ok(length_m) = authoring::edge_length(polygon, edge) else {
            return Vec::new();
        };
        let at = curves::arclength_point(&points, 0.5);
        let ahead = curves::arclength_point(&points, 0.55) - curves::arclength_point(&points, 0.45);
        let tangent = if ahead.length_squared() > 1e-18 {
            ahead
        } else {
            points[points.len() - 1] - points[0]
        };
        let outward = (DVec2::new(tangent.y, -tangent.x) * side).normalize_or_zero();
        labels.push(SegmentLabel {
            edge: edge.clone(),
            at,
            outward,
            length_m,
        });
    }
    labels
}

/// Whether a segment is long enough on screen to carry its label: the label must fit along
/// the segment with a little room to spare, otherwise labels of short neighbours pile up.
pub(super) fn label_fits(length_m: f64, pixels_per_metre: f32, label_width_px: f32) -> bool {
    let on_screen = length_m * f64::from(pixels_per_metre);
    on_screen.is_finite() && on_screen >= f64::from(label_width_px) + 8.
}

/// Millimetre readout used on the board, one decimal like the rest of the board.
pub(super) fn length_text(length_m: f64) -> String {
    format!("{:.1} mm", authoring::m_to_mm(length_m))
}

/// Whether a placement leaves the piece facing the front camera unrotated: then its material
/// frame and the front view of the avatar differ only by a translation and a flip of v.
pub(super) fn is_front_aligned(placement: &Placement) -> bool {
    placement
        .rotation_degrees
        .to_array()
        .iter()
        .all(|d| d.is_finite() && (d.rem_euclid(360.) < 0.5 || d.rem_euclid(360.) > 359.5))
}

/// Front silhouette of the avatar in a piece's local material frame (metres, v down).
///
/// With a front-aligned reference placement `t`, the material point (u, v) sits at 3D
/// (u + t.x, t.y − v), so the avatar point (x, y) lands at (x − t.x, t.y − y): the silhouette
/// lines up with where that piece hangs on the body. Without a reference the head top sits on
/// v = 0 and the body is centred on u = 0. Depth is dropped; triangles keep the surface indices.
pub(super) fn silhouette_uv(surface: &SurfaceMesh, reference: Option<&Placement>) -> Vec<DVec2> {
    let (dx, top) = match reference {
        Some(placement) => (placement.translation.x, placement.translation.y),
        None => (
            0.,
            surface
                .vertices
                .iter()
                .map(|v| v.y)
                .fold(f64::NEG_INFINITY, f64::max),
        ),
    };
    surface
        .vertices
        .iter()
        .map(|v| DVec2::new(v.x - dx, top - v.y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use std::collections::BTreeMap;

    fn polygon(points: &[(f64, f64)], curves: &[(&str, [DVec2; 2])]) -> PolygonPattern {
        PolygonPattern {
            points: points.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
            edges: (0..points.len()).map(|i| format!("e{i}")).collect(),
            subdivisions: 1,
            meshing: None,
            curves: curves.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
            holes: vec![],
            lines: vec![],
            target_lengths: BTreeMap::new(),
            seam_allowances: BTreeMap::new(),
            notches: BTreeMap::new(),
        }
    }

    #[test]
    fn straight_segments_report_exact_lengths_outside_the_piece() {
        let rect = polygon(&[(0., 0.), (0.4, 0.), (0.4, 0.6), (0., 0.6)], &[]);
        let labels = segment_labels(&rect);
        let lengths: Vec<f64> = labels.iter().map(|l| l.length_m).collect();
        assert_eq!(lengths, vec![0.4, 0.6, 0.4, 0.6]);
        let centre = DVec2::new(0.2, 0.3);
        for label in &labels {
            assert!((label.outward.length() - 1.).abs() < 1e-12);
            assert!((label.at - centre).dot(label.outward) > 0., "{label:?}");
        }
        assert_eq!(labels[0].at, DVec2::new(0.2, 0.));
        assert_eq!(length_text(labels[1].length_m), "600.0 mm");
    }

    #[test]
    fn clockwise_outlines_still_label_outward() {
        let rect = polygon(&[(0., 0.), (0., 0.6), (0.4, 0.6), (0.4, 0.)], &[]);
        for label in segment_labels(&rect) {
            assert!(
                (label.at - DVec2::new(0.2, 0.3)).dot(label.outward) > 0.,
                "{label:?}"
            );
        }
    }

    #[test]
    fn curved_lengths_match_the_inspector_and_converge_to_the_true_arclength() {
        // A cubic hem bowing 0.075 m below the bottom edge.
        let c = [DVec2::new(0.3, 0.7), DVec2::new(0.1, 0.7)];
        let shape = polygon(&[(0., 0.), (0.4, 0.), (0.4, 0.6), (0., 0.6)], &[("e2", c)]);
        let labels = segment_labels(&shape);
        assert_eq!(
            labels[2].length_m,
            authoring::edge_length(&shape, "e2").unwrap()
        );
        // Reference arclength by dense uniform sampling of the same cubic.
        let p = [DVec2::new(0.4, 0.6), c[0], c[1], DVec2::new(0., 0.6)];
        let n = 200_000;
        let dense: f64 = (0..n)
            .map(|i| {
                let a = curves::cubic(p, i as f64 / n as f64);
                let b = curves::cubic(p, (i + 1) as f64 / n as f64);
                a.distance(b)
            })
            .sum();
        // Flattening at 0.25 mm chord error loses well under 0.1 mm of length here.
        assert!(
            (dense - labels[2].length_m).abs() < 1e-4,
            "{dense} vs {}",
            labels[2].length_m
        );
        assert!(labels[2].length_m > 0.4);
        // The midpoint label sits below the hem, away from the piece.
        assert!(labels[2].outward.y > 0.9);
    }

    #[test]
    fn labels_wait_for_room_along_their_segment() {
        // A 145.6 mm neckline at 184 px/m is 26.8 px long: a 48 px label does not fit.
        assert!(!label_fits(0.1456, 184., 48.));
        // Zoomed in to 600 px/m the same segment is 87 px long and gets its label.
        assert!(label_fits(0.1456, 600., 48.));
        assert!(!label_fits(f64::NAN, 600., 48.));
    }

    #[test]
    fn malformed_outlines_yield_no_labels() {
        let mut bad = polygon(&[(0., 0.), (0.4, 0.), (0.4, 0.6)], &[]);
        bad.edges.pop();
        assert!(segment_labels(&bad).is_empty());
    }

    #[test]
    fn front_alignment_accepts_full_turns_only() {
        let at = |r: [f64; 3]| Placement {
            translation: DVec3::ZERO,
            rotation_degrees: DVec3::from_array(r),
        };
        assert!(is_front_aligned(&at([0., 0., 0.])));
        assert!(is_front_aligned(&at([360., -360., 0.2])));
        assert!(!is_front_aligned(&at([0., 180., 0.])));
        assert!(!is_front_aligned(&at([0., 0., 90.])));
        assert!(!is_front_aligned(&at([f64::NAN, 0., 0.])));
    }

    #[test]
    fn silhouette_follows_the_reference_piece() {
        let surface = SurfaceMesh {
            vertices: vec![
                DVec3::new(-0.2, 1.7, 0.1),
                DVec3::new(0.2, 1.0, -0.1),
                DVec3::new(0., 0., 0.),
            ],
            triangles: vec![[0, 1, 2]],
        };
        let placement = Placement {
            translation: DVec3::new(-0.2, 1.4, 0.15),
            rotation_degrees: DVec3::ZERO,
        };
        let uv = silhouette_uv(&surface, Some(&placement));
        // A material point maps to 3D exactly where the avatar point it overlays lies.
        for (vertex, uv) in surface.vertices.iter().zip(&uv) {
            let local = DVec3::new(uv.x, -uv.y, 0.);
            let world = placement.transform(local);
            assert!((world.x - vertex.x).abs() < 1e-12 && (world.y - vertex.y).abs() < 1e-12);
        }
        let free = silhouette_uv(&surface, None);
        assert_eq!(free[0], DVec2::new(-0.2, 0.));
        assert!((free[2].y - 1.7).abs() < 1e-12);
    }
}
