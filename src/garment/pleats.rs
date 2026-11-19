//! Pleats along a segment: straight creases laid across a piece at even steps along one of its
//! segments (a waist, a yoke seam), each with the fold angle that turns the fabric under for
//! its kind of pleat, and optionally tacks that close each pleat at the segment.
//!
//! A pleat of depth `d` hides `2·d` of fabric: two creases `d` apart fold it into a Z, the
//! outer crease turning the fabric back and the inner one turning it forward again. Laid
//! flat, the outer crease at `s` lands on the point `s + 2·d`, which is where its tack goes.
use serde::{Deserialize, Serialize};

/// How the pleats fold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PleatKind {
    /// Every pleat folds the same way.
    #[default]
    Knife,
    /// Two knife pleats folding away from each other, meeting behind.
    Box,
    /// A box pleat turned inside out: the two folds meet in front.
    InvertedBox,
}

/// Fold angle of an outer crease (the fabric turns away from its front), degrees, as a
/// line fold's angle (180 is flat).
pub const OUTER_FOLD_DEGREES: f64 = 330.;
/// Fold angle of an inner crease (the fabric turns back toward its front), degrees.
pub const INNER_FOLD_DEGREES: f64 = 30.;
/// Crease stiffness, times the fabric's bending.
pub const PLEAT_FOLD_STRENGTH: f64 = 10.;
/// Most pleats one command lays.
pub const MAX_PLEATS: usize = 60;
/// Pleat depths accepted, metres.
pub const DEPTH_RANGE_M: std::ops::RangeInclusive<f64> = 0.003..=0.3;
/// Closest two creases, or a crease and a segment end, may lie, metres.
const MIN_GAP_M: f64 = 0.002;

/// One crease: where it leaves the segment (arclength from its start, metres) and its fold
/// angle (degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crease {
    pub at_m: f64,
    pub angle: f64,
}

/// The creases of `count` pleats of `kind`, each hiding `2·depth_m` of fabric, centred at
/// even steps along a segment `length_m` long, in order along it; and for each pleat the
/// pair of arclengths a tack joins to close it at the segment.
pub fn layout(
    length_m: f64,
    count: usize,
    depth_m: f64,
    kind: PleatKind,
) -> Result<(Vec<Crease>, Vec<[f64; 2]>), String> {
    if !(1..=MAX_PLEATS).contains(&count) {
        return Err(format!("Lay 1 to {MAX_PLEATS} pleats"));
    }
    if !depth_m.is_finite() || !DEPTH_RANGE_M.contains(&depth_m) {
        return Err("The pleat depth must be between 3 and 300 mm".into());
    }
    if !length_m.is_finite() || length_m <= 0. {
        return Err("The segment has no length".into());
    }
    let d = depth_m;
    let (outer, inner) = match kind {
        PleatKind::InvertedBox => (INNER_FOLD_DEGREES, OUTER_FOLD_DEGREES),
        _ => (OUTER_FOLD_DEGREES, INNER_FOLD_DEGREES),
    };
    let mut creases = vec![];
    let mut tacks = vec![];
    for k in 0..count {
        let c = length_m * (k as f64 + 0.5) / count as f64;
        match kind {
            PleatKind::Knife => {
                creases.push(Crease {
                    at_m: c - 0.5 * d,
                    angle: outer,
                });
                creases.push(Crease {
                    at_m: c + 0.5 * d,
                    angle: inner,
                });
                tacks.push([c - 0.5 * d, c + 1.5 * d]);
            }
            PleatKind::Box | PleatKind::InvertedBox => {
                // The left half folds toward the right, the right half mirrors it.
                creases.push(Crease {
                    at_m: c - 1.5 * d,
                    angle: outer,
                });
                creases.push(Crease {
                    at_m: c - 0.5 * d,
                    angle: inner,
                });
                creases.push(Crease {
                    at_m: c + 0.5 * d,
                    angle: inner,
                });
                creases.push(Crease {
                    at_m: c + 1.5 * d,
                    angle: outer,
                });
                tacks.push([c - 1.5 * d, c + 1.5 * d]);
            }
        }
    }
    let ends = tacks.iter().flatten().copied();
    let lowest = creases
        .iter()
        .map(|c| c.at_m)
        .chain(ends.clone())
        .fold(f64::INFINITY, f64::min);
    let highest = creases
        .iter()
        .map(|c| c.at_m)
        .chain(ends)
        .fold(f64::NEG_INFINITY, f64::max);
    if lowest < MIN_GAP_M || highest > length_m - MIN_GAP_M {
        return Err("The pleats do not fit on this segment; lay fewer or shallower ones".into());
    }
    if creases
        .windows(2)
        .any(|w| w[1].at_m - w[0].at_m < MIN_GAP_M)
    {
        return Err("The pleats overlap; lay fewer or shallower ones".into());
    }
    Ok((creases, tacks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knife_pleats_fold_in_a_z_and_tack_the_outer_crease_to_where_it_lands() {
        let (creases, tacks) = layout(0.6, 3, 0.02, PleatKind::Knife).unwrap();
        assert_eq!(creases.len(), 6);
        assert!((creases[0].at_m - 0.09).abs() < 1e-12);
        assert!((creases[1].at_m - 0.11).abs() < 1e-12);
        assert_eq!(creases[0].angle, OUTER_FOLD_DEGREES);
        assert_eq!(creases[1].angle, INNER_FOLD_DEGREES);
        // The outer crease lands 2·depth further on.
        assert!((tacks[0][1] - tacks[0][0] - 0.04).abs() < 1e-12);
        assert!(creases.windows(2).all(|w| w[0].at_m < w[1].at_m));
    }

    #[test]
    fn box_pleats_mirror_and_inverted_ones_fold_the_other_way() {
        let (boxed, tacks) = layout(0.4, 2, 0.015, PleatKind::Box).unwrap();
        assert_eq!(boxed.len(), 8);
        let angles: Vec<f64> = boxed[..4].iter().map(|c| c.angle).collect();
        assert_eq!(
            angles,
            [
                OUTER_FOLD_DEGREES,
                INNER_FOLD_DEGREES,
                INNER_FOLD_DEGREES,
                OUTER_FOLD_DEGREES
            ]
        );
        assert!((tacks[0][1] - tacks[0][0] - 0.045).abs() < 1e-12);
        let (inverted, _) = layout(0.4, 2, 0.015, PleatKind::InvertedBox).unwrap();
        for (b, i) in boxed.iter().zip(&inverted) {
            assert_eq!(b.at_m, i.at_m);
            assert_eq!(b.angle + i.angle, 360.);
        }
    }

    #[test]
    fn pleats_that_do_not_fit_are_refused() {
        assert!(layout(0.1, 10, 0.02, PleatKind::Knife).is_err());
        assert!(layout(0.5, 0, 0.02, PleatKind::Knife).is_err());
        assert!(layout(0.5, 2, 0.001, PleatKind::Knife).is_err());
        assert!(layout(0.05, 1, 0.02, PleatKind::Box).is_err());
    }
}
