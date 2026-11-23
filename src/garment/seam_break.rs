//! Seams that break when pulled too hard.
//!
//! A seam is a penalty spring per stitch sample: its energy is ½ k w |gap|², with k the seam
//! stiffness (N/m²) and w the seam length the sample stands for (m). The force it carries is
//! therefore k w |gap| newtons, or k |gap| newtons per metre of seam: the pull the two pieces
//! put on each other there. A seam with a breaking strength (N/m) loses a stitch, that is the
//! stretch of seam one of its intervals sews, when that pull stays above the strength for
//! [`BREAK_HOLD_S`] of simulated time. The break is recorded as a tear of the seam (the same
//! tears a user makes by hand), so undo, save and reload need nothing new.
//!
//! A stitch can only break once it has been sewn: its gap must first have come within
//! [`SEWN_GAP_M`]. While the pieces are still being pulled together (the start of a
//! simulation, or a seam just drawn between pieces far apart) the large pull is how the seam
//! closes, not a load on stitches that exist; it never breaks them.
use super::{CompiledGarment, seam::CompiledSeam};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// How long a stitch's pull must stay above its seam's strength before it breaks, seconds of
/// simulated time: a jolt shorter than this does not tear the seam.
pub const BREAK_HOLD_S: f64 = 0.05;
/// A stitch also needs this many consecutive overloaded observations, so one step never
/// breaks it whatever the time step.
pub const BREAK_HOLD_STEPS: u32 = 2;
/// A stitch counts as sewn once the gap between its two sides has been at most this, metres.
pub const SEWN_GAP_M: f64 = 0.002;
/// Breaking strengths a seam may have, N per metre of seam.
pub const STRENGTH_N_PER_M: std::ops::RangeInclusive<f64> = 0.1..=1e6;
/// The strength a seam gets when breaking is turned on for it, N/m (10 N/cm).
pub const DEFAULT_STRENGTH_N_PER_M: f64 = 1000.;

/// A stitch that broke: the stretch of `seam` it sewed, as shares of the seam's length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StitchBreak {
    pub seam: String,
    pub span: [f64; 2],
    /// The pull it carried when it broke, N per metre of seam.
    pub pull_n_per_m: f64,
}

/// The pull per metre of seam of each stitch (interval) of `seam` at positions `x`, with the
/// stretch it sews and its largest gap: the larger of its samples' pulls.
pub fn stitch_pulls(seam: &CompiledSeam, x: &[DVec3]) -> Vec<([f64; 2], f64, f64)> {
    let mut out: Vec<([f64; 2], f64, f64)> = vec![];
    for sample in &seam.samples {
        let gap = sample.point.gap(x).length();
        let pull = seam.stiffness * gap;
        match out.last_mut() {
            Some((span, p, g)) if *span == sample.span => {
                *p = p.max(pull);
                *g = g.max(gap);
            }
            _ => out.push((sample.span, pull, gap)),
        }
    }
    out
}

type Key = (String, [u64; 2]);

fn key(seam: &str, span: [f64; 2]) -> Key {
    (seam.to_owned(), span.map(f64::to_bits))
}

/// Watches the stitches of seams that have a breaking strength while a simulation runs, and
/// says which broke. Keyed by seam and stretch, so it carries over a recompiled garment whose
/// other stitches are unchanged (as after a break).
#[derive(Clone, Debug, Default)]
pub struct SeamBreaker {
    /// Stitches that have been sewn (closed) at least once.
    sewn: HashSet<Key>,
    /// How long, and for how many observations, each stitch has been overloaded.
    overloaded: HashMap<Key, (f64, u32)>,
    /// Stitches already reported broken.
    broken: HashSet<Key>,
}

impl SeamBreaker {
    /// Forget every stitch: the next simulation starts over (its seams must close again
    /// before they can break).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Stitches being watched that have been sewn.
    pub fn sewn_count(&self) -> usize {
        self.sewn.len()
    }

    /// Look at the stitches at positions `x`, `dt` seconds of simulated time after the last
    /// look, and return those that broke now. `strengths` are the seams' breaking strengths
    /// by id (N/m); seams without one are not watched.
    pub fn observe(
        &mut self,
        strengths: &BTreeMap<String, f64>,
        garment: &CompiledGarment,
        x: &[DVec3],
        dt: f64,
    ) -> Vec<StitchBreak> {
        let mut out = vec![];
        if strengths.is_empty() || !dt.is_finite() || dt <= 0. || x.len() != garment.mesh.uv.len() {
            return out;
        }
        for seam in &garment.seams {
            let Some(&strength) = strengths.get(&seam.id) else {
                continue;
            };
            for (span, pull, gap) in stitch_pulls(seam, x) {
                let k = key(&seam.id, span);
                if self.broken.contains(&k) {
                    continue;
                }
                if gap <= SEWN_GAP_M {
                    self.sewn.insert(k.clone());
                }
                if !self.sewn.contains(&k) || !pull.is_finite() || pull <= strength {
                    self.overloaded.remove(&k);
                    continue;
                }
                let (time, steps) = self.overloaded.entry(k.clone()).or_insert((0., 0));
                *time += dt;
                *steps += 1;
                if *time >= BREAK_HOLD_S - 1e-12 && *steps >= BREAK_HOLD_STEPS {
                    self.overloaded.remove(&k);
                    self.broken.insert(k);
                    out.push(StitchBreak {
                        seam: seam.id.clone(),
                        span,
                        pull_n_per_m: pull,
                    });
                }
            }
        }
        out
    }
}

/// Most tears a seam may have (as `GarmentTemplate` validation allows).
pub const MAX_TEARS: usize = 64;

/// `tears` and `added` together, sorted, with overlapping and touching stretches merged; when
/// they are more than [`MAX_TEARS`], the narrowest gaps between them close up (the seam is
/// torn there too) until they fit.
pub fn merge_tears(tears: &[[f64; 2]], added: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut all: Vec<[f64; 2]> = tears
        .iter()
        .chain(added)
        .filter(|[lo, hi]| lo.is_finite() && hi.is_finite() && hi > lo)
        .map(|[lo, hi]| [lo.clamp(0., 1.), hi.clamp(0., 1.)])
        .collect();
    all.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut merged: Vec<[f64; 2]> = vec![];
    for [lo, hi] in all {
        match merged.last_mut() {
            Some(last) if lo <= last[1] + 1e-12 => last[1] = last[1].max(hi),
            _ => merged.push([lo, hi]),
        }
    }
    while merged.len() > MAX_TEARS {
        let i = (0..merged.len() - 1)
            .min_by(|a, b| {
                (merged[*a + 1][0] - merged[*a][1]).total_cmp(&(merged[*b + 1][0] - merged[*b][1]))
            })
            .unwrap_or(0);
        merged[i][1] = merged[i + 1][1];
        merged.remove(i + 1);
    }
    merged
}

/// The template's tears with `breaks` added, by seam id; seams with no break keep theirs.
pub fn tears_after(
    tears: &BTreeMap<String, Vec<[f64; 2]>>,
    breaks: &[StitchBreak],
) -> BTreeMap<String, Vec<[f64; 2]>> {
    let mut out = tears.clone();
    let mut by_seam: BTreeMap<&str, Vec<[f64; 2]>> = BTreeMap::new();
    for b in breaks {
        by_seam.entry(&b.seam).or_default().push(b.span);
    }
    for (seam, spans) in by_seam {
        let before = out.get(seam).cloned().unwrap_or_default();
        out.insert(seam.to_owned(), merge_tears(&before, &spans));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tears_merge_when_they_touch_and_stay_within_the_limit() {
        assert_eq!(
            merge_tears(&[[0.2, 0.3]], &[[0.3, 0.4], [0.6, 0.7]]),
            vec![[0.2, 0.4], [0.6, 0.7]]
        );
        let many: Vec<[f64; 2]> = (0..100)
            .map(|i| [i as f64 / 100., i as f64 / 100. + 0.004])
            .collect();
        let merged = merge_tears(&[], &many);
        assert_eq!(merged.len(), MAX_TEARS);
        assert!(merged.windows(2).all(|w| w[0][1] < w[1][0]));
        assert_eq!(merged[0][0], 0.);
    }
}
