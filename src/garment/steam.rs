//! Steam: a brush that shrinks or stretches the cloth where it is painted, as a steam iron
//! does to wool. Each stamp of the brush scales the rest shape of the cloth under it; the
//! simulation then pulls the cloth to its new size. Stamps of one stroke overlap, so where
//! several reach a point the strongest one counts, not their sum.
use super::CompiledGarment;
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// One dab of the steam brush on a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteamStamp {
    pub panel: String,
    /// Centre, in the piece's material coordinates, metres.
    pub center: DVec2,
    pub radius_m: f64,
    /// Share of the radius at full strength, 0 to 1; beyond it the effect eases off to the rim.
    pub hardness: f64,
    /// Size the cloth takes under the stamp, percent: below 100 shrinks, above stretches.
    pub percent: f64,
}

/// Sizes the steam can give the cloth, percent.
pub const PERCENT_RANGE: std::ops::RangeInclusive<f64> = 50.0..=150.0;
/// Brush radii, metres.
pub const RADIUS_RANGE_M: std::ops::RangeInclusive<f64> = 0.005..=0.3;
/// Most stamps a document keeps.
pub const MAX_STAMPS: usize = 20_000;

impl SteamStamp {
    pub fn validate(&self) -> Result<(), String> {
        if !self.center.is_finite()
            || !RADIUS_RANGE_M.contains(&self.radius_m)
            || !(0.0..=1.0).contains(&self.hardness)
            || !PERCENT_RANGE.contains(&self.percent)
        {
            return Err(
                "Steam needs a radius of 5 to 300 mm, a hardness of 0 to 100 % and a size of 50 to 150 %"
                    .into(),
            );
        }
        Ok(())
    }

    /// How much of this stamp reaches material point `uv`: all of it within the hard core,
    /// easing off smoothly to nothing at the rim.
    pub fn weight(&self, uv: DVec2) -> f64 {
        let d = uv.distance(self.center) / self.radius_m;
        if d >= 1. {
            0.
        } else if d <= self.hardness {
            1.
        } else {
            let t = (d - self.hardness) / (1. - self.hardness);
            1. - t * t * (3. - 2. * t)
        }
    }
}

/// Scale of the cloth at material point `uv` of `panel`: 1 where no steam reaches, otherwise
/// the strongest stamp's effect there.
pub fn scale_at(stamps: &[SteamStamp], panel: &str, uv: DVec2) -> f64 {
    let mut strongest = 0_f64;
    for stamp in stamps.iter().filter(|s| s.panel == panel) {
        let effect = stamp.weight(uv) * (stamp.percent / 100. - 1.);
        if effect.abs() > strongest.abs() {
            strongest = effect;
        }
    }
    1. + strongest
}

/// Stamps along a stroke through `points` (material metres), a quarter radius apart.
pub fn stroke(
    panel: &str,
    points: &[DVec2],
    radius_m: f64,
    hardness: f64,
    percent: f64,
) -> Vec<SteamStamp> {
    let stamp = |center: DVec2| SteamStamp {
        panel: panel.into(),
        center,
        radius_m,
        hardness,
        percent,
    };
    let Some(&first) = points.first() else {
        return vec![];
    };
    let step = (radius_m * 0.25).max(1e-4);
    let mut out = vec![stamp(first)];
    let mut last = first;
    for &p in &points[1..] {
        let mut gap = p.distance(last);
        while gap >= step {
            last += (p - last) / gap * step;
            out.push(stamp(last));
            gap = p.distance(last);
        }
    }
    out
}

/// Scale the rest shape of every triangle of `out` that steam reaches, by the scale at its
/// centre: lengths by `s`, areas (and the vertices' rest areas) by `s²`.
pub(crate) fn apply(out: &mut CompiledGarment, stamps: &[SteamStamp]) {
    for range in out.panels.clone() {
        if !stamps.iter().any(|s| s.panel == range.id) {
            continue;
        }
        for k in range.triangles.clone() {
            let t = &out.mesh.triangles[k];
            let centre = t.vertices.iter().map(|&v| out.mesh.uv[v]).sum::<DVec2>() / 3.;
            let s = scale_at(stamps, &range.id, centre);
            if (s - 1.).abs() < 1e-12 {
                continue;
            }
            let vertices = t.vertices;
            let t = &mut out.mesh.triangles[k];
            let before = t.area;
            t.rest = t.rest.map(|p| p * s);
            t.inv_dm *= 1. / s;
            t.area *= s * s;
            let added = (t.area - before) / 3.;
            for v in vertices {
                out.mesh.rest_area[v] += added;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(percent: f64, hardness: f64) -> SteamStamp {
        SteamStamp {
            panel: "p".into(),
            center: DVec2::ZERO,
            radius_m: 0.1,
            hardness,
            percent,
        }
    }

    #[test]
    fn a_stamp_is_full_in_its_core_and_fades_to_its_rim() {
        let s = stamp(80., 0.5);
        assert_eq!(s.weight(DVec2::new(0.04, 0.)), 1.);
        let mid = s.weight(DVec2::new(0.075, 0.));
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
        assert_eq!(s.weight(DVec2::new(0.1, 0.)), 0.);
        assert!((scale_at(std::slice::from_ref(&s), "p", DVec2::ZERO) - 0.8).abs() < 1e-12);
        assert_eq!(scale_at(&[s], "q", DVec2::ZERO), 1.);
    }

    #[test]
    fn overlapping_stamps_take_the_strongest_not_the_sum() {
        let stamps = [stamp(90., 1.), stamp(80., 1.), stamp(105., 1.)];
        assert!((scale_at(&stamps, "p", DVec2::ZERO) - 0.8).abs() < 1e-12);
    }

    #[test]
    fn a_stroke_lays_stamps_a_quarter_radius_apart() {
        let points = [DVec2::ZERO, DVec2::new(0.1, 0.)];
        let stamps = stroke("p", &points, 0.02, 0.5, 85.);
        // 10 cm at 5 mm steps: twenty steps, give or take rounding at the end.
        assert!((20..=21).contains(&stamps.len()), "{}", stamps.len());
        assert!(
            stamps
                .windows(2)
                .all(|w| (w[0].center.distance(w[1].center) - 0.005).abs() < 1e-9)
        );
        assert!(stamps.iter().all(|s| s.validate().is_ok()));
        assert!(stamp(40., 0.5).validate().is_err());
    }
}
