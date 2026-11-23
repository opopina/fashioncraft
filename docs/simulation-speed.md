# Simulation speed and settling — 22 November 2026

This pass makes a garment sew and come to rest sooner by default, removes work from every
step, and lets each fabric set its own friction. Physics laws, contact radii, movement bounds
and iteration budgets are unchanged; where a change alters trajectories it says so.

## Changes

- **Split steps by default.** `VbdSettings::max_substeps` now defaults to 16
  (`DEFAULT_MAX_SUBSTEPS`). A step whose motion its contacts cannot allow at once is split
  into shorter whole steps, each with its own contact detection. `VbdSettings::part` gives
  each part a share of the time **and of the iterations** (at least `MIN_PART_ITERATIONS` =
  4), so a split step costs about the same sweeps as an unsplit one plus one detection per
  part. Settings saved without the field (every project saved before) take the new default;
  a saved `"max_substeps": 1` still never splits. The CPU and GPU solvers share the rule.
- **Where a step splits.** A part may move 0.3 of the query radius of four contact radii
  (`SPLIT_SHARE`, was 0.4): 3.6 mm at the 3 mm default, about 0.13 m/s at 1/60 s. A part's
  query reaches its contact radius plus its motion and its warm start is clipped to 0.45 of
  that, so at 0.4 a part's ball (3.5 mm) fell short of its motion (4.8 mm). A clipped vertex
  ran ahead of its neighbours alone, they pulled it back, and the sheet needed many
  iterations to move together: with the shared iterations a falling square fell less than
  half as far as it should. At 0.3 the ball (3 mm) nearly covers the motion. 16 parts then
  follow cloth moving at about 3.4 m/s (a 60 cm drop), four parts about 0.8 m/s.
- **"Full-speed falls"** (Simulation › Quality) is on by default. Off, a step splits into at
  most four parts (`CAPPED_MAX_SUBSTEPS`): enough for sewing and draping, and falling cloth is
  held to about 0.8 m/s with no extra work while it falls. The Fast/Normal/Accurate presets set
  the step and iterations only; a split step shares whatever iterations the preset gives.
- **Friction terms.** `friction_terms` evaluated and differentiated every retained contact
  pair (about 8,000 per step on the T-shirt) when only a few hundred press. It now finds the
  pressing ones with one plain energy evaluation each, on the calling thread, and builds only
  those in parallel. Results are bitwise identical.
- **CPU derivatives.** The second-order forward AD type `Jet` computed every subtraction as an
  addition of a value multiplied by −1 (a full product of Hessians) and both triangles of
  every symmetric Hessian. Subtraction and negation are now direct and Hessians are
  assembled from their upper triangle. Every CPU local solve, the GPU path's f64 precision
  and seam refinements and the friction terms use it. Values are mathematically the same;
  Hessians are now exactly symmetric, so CPU trajectories differ in the last bits.
- **GPU contact search.** Each edge query reports only pairs with later edges, so an avatar
  edge, which comes after every cloth edge, never finds a pair: those queries (most of the
  edges) are no longer dispatched. The search is checked against the complete f64 detector
  by `tests/gpu_contact_filter.rs`.
- **Subgroup reductions on Apple GPUs.** Metal reports a 4–64 subgroup range for every GPU it
  drives, so the 32-lane shuffle reductions were never used on a Mac. `GpuVbd::new` now
  measures the subgroup size inside a 32-invocation kernel and uses the shuffle reductions
  when it is 32 (every Apple GPU). Results are bitwise identical to the shared-memory path.
- **Per-fabric friction.** `Material::friction` / `FabricProperties::friction` (optional,
  0–5) override the scene's coefficient. A contact side takes the largest coefficient of its
  cloth vertices; two pieces of cloth rub with the mean of their two sides, cloth on the
  avatar or the floor with its own (`friction::contact_friction`). Fabrics without a value use
  the scene's, so saved projects are unchanged. The Fabric tab has **Own friction**, like
  **Own collision distance**. The GPU receives the combined μλ per term, so both solvers
  agree.
- `cloth-fit` gained `--friction` and `--max-substeps`; `bench/summarize.py` reports sewing
  and settling times (simulated and real), `bench/tree.py` summarises macOS `sample`
  profiles, and `examples/seam_gap_probe.rs` lists the contacts on the widest seam probes of a
  saved state.

## Seams that stay open about 3 mm

The widest seam gap of a draped T-shirt levels off near 3 mm, close to the contact radius.
`seam_gap_probe` shows it is not contact: the widest probe is the underarm corner where the
sleeve's underarm seam meets the side seam, and neither of its two vertices has an active
contact. The gap is the seam spring's equilibrium against the cloth's tension at that corner.
Measured on the T-shirt (dt 1/60, friction 0.3, at most 4 parts at the earlier split share of 0.4):

| Seam stiffness | Widest gap at 5 s | Sewn (< 3.5 mm) | RMS speed at 5 s |
|---|---:|---:|---:|
| 50,000 N/m² (default) | 3.11 mm | 1.61 s | 0.16 cm/s |
| 100,000 | 1.68 mm | 1.16 s | 0.26 cm/s |
| 200,000 | 0.89 mm | 1.06 s | 0.53 cm/s |
| 500,000 | 0.38 mm | 0.92 s | 1.12 cm/s |

Stiffer seams close and sew sooner, but two sewn vertices then pull each other back and forth
between their local solves and the garment rests later. The default is unchanged and now
lives in `garment::seam::DEFAULT_SEAM_STIFFNESS`. Closing the corner without the slower rest
needs the sewn vertices solved together (the experimental seam-block refinement) — not done
here. Excluding contact between vertices sewn to a common partner was tried and changed
nothing, as the probe predicts.

## Tried and not kept

- Four line-search step lengths per pass over a vertex's terms (same result, bitwise): the
  GPU sweep got slower, so the sweep is bound by arithmetic, not by its barriers.
- Coloring vertices by decreasing degree: one color fewer on average and less GPU time per
  sweep, but the garment came to rest later.
- Two line-search attempts instead of eight: less GPU time, but the T-shirt never came to rest.
- More narrow-phase lanes than two: bitwise identical, no measurable gain.
- `SPLIT_SHARE` 0.2 (no clipping at all): sewn and at rest soonest in simulated time and the
  lowest stretch, but cloth moving faster than about 6 cm/s splits: at rest after 3.9 s real.
- A query radius of the part's motion / 0.45 (so the ball covers the motion without more
  parts): at rest sooner in simulated time but not in real time (2.75 s against 2.68 s), and
  the dense private garment came within 0.02 mm of contact.

## Evidence

Apple M1 Max, macOS, `cloth-fit --threads 10 --gpu-sim --steps 300` on the T-shirt preset
(2,454 vertices) saved as a project with the editor's settings (1/60 s, 16 iterations,
friction 0.3, no `max_substeps`), so each build runs its own default. *Before* is `public/main`
(aacc758); *start* is the measured starting point of this pass (friction fix and shared
iterations, run with `--max-substeps 4`). Three interleaved runs each on a quiet machine; the
physics is deterministic, so only the wall times vary (within 0.4 ms per step).
`bench/summarize.py`: sewn = widest seam under 3.5 mm; at rest = RMS speed under 1 cm/s from
then on with the seams sewn.

| T-shirt, GPU | Before | Start | **Now** |
|---|---:|---:|---:|
| Mean step | 17.6 ms | 16.1 ms | 17.2 ms |
| Sewn (simulated / real) | 2.90 s / 3.01 s | 1.61 s / 1.82 s | **1.04 s / 1.44 s** |
| At rest (simulated / real) | not within 5 s | 2.73 s / 2.97 s | **2.00 s / 2.64 s** |
| RMS speed at 5 s | 1.19 cm/s | 0.16 cm/s | 0.14 cm/s |
| Step once at rest (last 120 steps) | 18.2 ms | 13.4 ms | 13.4 ms |
| Widest seam gap at 5 s | 2.86 mm | 3.11 mm | 3.05 mm |
| Stretch p95 / max (solver) | 4.8 % / 24.4 % | 4.9 % / 27.0 % | 4.9 % / 26.0 % |

The independent OBJ evaluator (`drapecraft/bench/evaluate.ts`) finds no vertex inside the body
before or after, stretch p95 3.7 % / 3.9 % and max 17.4 % / 18.0 %. Its "crossings away from
borders" (3 before, 6 now) are all seam edges: a boundary edge of one panel through a boundary
triangle of the other at the side and shoulder seams, just past its 1 cm border threshold on
2 cm edges. The initial arrangement has none.

Other measurements, same machine:

| | Before | Now |
|---|---:|---:|
| T-shirt on the CPU, unsplit step (`"max_substeps": 1`) | 147–158 ms | **91–94 ms** |
| of which local solves | 101–110 ms | 55–58 ms |
| A private dense garment (3,513 vertices), GPU, mean step | 21.4 ms (0.78× real time) | **17.5 ms (0.95×)** |
| Its RMS speed at 5 s | 1.15 cm/s | 0.17 cm/s |
| Its closest contact at 5 s | 0.15 mm | 2.33 mm |

Its sleeve seams stay several centimetres open in both builds, as they are in its saved
state: their ends are pressed against the body. That is the arrangement, not the solver.

Once the T-shirt rests, a step costs the same as at the start of this pass: the GPU sweeps
take about 7.5 ms (10 colors × 16 sweeps) and the contact search about 4 ms, mostly the GPU
filter's round trip. Subgroup reductions and the skipped avatar-edge queries gained under 5 %
there; the sweeps are bound by the arithmetic of each vertex's terms.

## Tests

`tests/substeps.rs` covers the iteration share of `part`, the new default (a free square falls
nearly as far as a damped free fall in half a second, and less with "Full-speed falls" off) and
settings saved with and without the field. `tests/friction.rs` checks a fabric's own μ on the
inclined plane both ways (slippery fabric on a grippy scene, grippy fabric on a frictionless
one) and the pair rule; `tests/gpu_friction.rs` repeats the plane on the GPU with per-vertex
forces checked against the CPU. `tests/gpu_motion.rs` pins one unsplit step where it checks a
single detection's movement bound. On the GPU only the first part of a split step follows a
body that moved (the later parts start from contacts found after it): the arm-lift recording
now carries the cloth 3.0 cm on the GPU and 3.4 cm on the CPU, closer than before (5.1 / 3.1).

## Not done

- Hand-written element gradients and Hessians: the faster `Jet` gave most of the CPU gain
  without changing any element's formula.
- Closing the underarm seam without a slower rest (coupled seam solves).
- Reusing contact candidates across the parts of a split step, which would take most of the
  per-part contact search off a falling garment.
