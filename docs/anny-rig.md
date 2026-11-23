# Anny compact rig

MakeHuman avatars — the bundled mannequin and every body built by
`src/avatar_maker.rs` — now use Anny's compact 104-bone skeleton paired with
MakeHuman/MPFB2's hand-painted skin weights instead of the previous 19-bone
nearest-bone automatic rig. The primary goal is cleaner deformation at the
shoulder, axilla, elbow and wrist where sleeves drape and crease.

## Asset and conversion

`scripts/convert-anny-rig.py` (Python standard library only, run with
`python3 -I`) converts MakeHuman/MPFB2's CC0 `rig.default.json` and
`weights.default.json` from NAVER LABS Europe's `naver/anny` repository at
revision `8f35dc2df1c4062fd2fae86734f9705de50c0c2b` into
`assets/makehuman/anny-rig.bin` (125.7 KB, zlib-compressed binary, magic
`FCAR` v1). Provenance and licensing are recorded in
`assets/makehuman/anny-rig.provenance.json`,
`assets/makehuman/anny-rig.LICENSE.md` (CC0 1.0 Universal) and
`THIRD_PARTY_NOTICES.md`.

Pruning follows Anny's `"anny"` rig specification (Apache License 2.0,
reimplemented independently in the converter): 59 bones are removed from the
163-bone MakeHuman default rig — tongue (11), breasts (2), facial-expression
bones (46, which include all 24 zero-weight bones) — leaving 104 bones in
parent-first depth-first order. Every retained bone is reparented to its
nearest retained ancestor (for example, `eye.L` and `eye.R` reparent to
`head`), and skin weights from removed bones are summed onto their nearest
retained ancestor.

Before transferring weights, the converter verifies that the first 13,380
vertices of `assets/makehuman/base.obj` (transformed by `measurements.json`)
match `assets/makehuman/mannequin.obj` vertex-for-vertex (observed maximum
coordinate difference: `8.30e-11` m).

## Bone placement across body shapes

Each bone's head and tail in `anny-rig.bin` uses one of four placement rules:

- **Joint helper cubes (`Cube`, 195 of 208 endpoints across 110 landmarks):**
  placed directly at the body's own `joint-*` landmark from `measurements.json`
  or `MacroTargets::make`. On the stock mannequin and on generated bodies of
  any age, height, weight or proportions, these endpoints land at exact zero
  offset from the body's joint helpers.
- **Direct body vertex (`Vertex`, `root` head = vertex 4223):** taken directly
  from `surface.vertices[4223]` when the mesh is present, with a pelvis
  landmark-frame fallback in bones-only mode.
- **Body vertex midpoints (`MeanBody`, 6 split joints: `upperleg01/02`,
  `lowerarm01/02`, `foot.L/R` tails):** exact midpoint of the two body vertices
  `< 13380`, with a linear interpolation fallback between adjacent landmarks in
  bones-only mode.
- **Body + helper midpoints (`MeanBodyHelper`, 6 split joints: `upperarm01/02`,
  `lowerleg01/02`, `neck01/02`, `neck02/03`):** the six non-body MakeHuman
  helper vertices (`helper-tights` and `helper-hair`) are bound at conversion
  time to the local triangle frame `(u, v, h)` of a well-conditioned adjacent
  body triangle (`p = a + u(b - a) + v(c - a) + h * sqrt(|(b - a) x (c - a)|) * n`),
  giving exact reconstruction on the base mesh and scale-covariant tracking on
  morphed bodies without enlarging `macro-targets.bin`. In bones-only mode
  (empty surface, used by `AvatarPose::with_joint` and skeleton drawing), they
  fall back to landmark interpolation so joint edits still validate bone
  lengths.

## Skin weights and influence limit

After merging removed bones' weights onto their nearest retained ancestors,
every one of the 13,380 body vertices has strictly positive weight summing to
1.0. Across the body mesh, 1,846 vertices carry 5 to 9 non-zero bone
influences (concentrated around the torso, shoulders, axilla, wrists and
hips):

| Kept influences (`K`) | Max dropped weight | Vertices losing > 1 % weight | Max L1 weight error |
|---|---:|---:|---:|
| 4 (previous limit) | 0.2196 | 1,162 | 0.4392 |
| 6 | 0.0572 | 131 | 0.1144 |
| 8 (`INFLUENCES`) | 0.0010 | 0 | 0.0020 |

Because truncating to 4 influences drops up to 22 % weight around the
shoulders and torso, `INFLUENCES` in `src/avatar_rig.rs` is raised from 4 to 8.
The remaining 2 vertices with a 9th influence lose at most 0.001 weight before
renormalisation, and every vertex's weights are normalised in `f64` to sum to
1.0.

## Measured shoulder and axilla preservation

Comparing `Rig::new` (104-bone Anny rig) against `Rig::build_automatic`
(legacy 19-bone surface-smoothed rig) on the stock mannequin with the left arm
raised via `upper_arm.L` (`Z = +75°` and `Z = +90°`):

- **Arm cross-sectional area at 20 % from shoulder to elbow (beside the
  axilla, rest = 88.74 cm²):**
  - At `+75°`: legacy drops to 56.40 cm² (63.6 %), Anny retains 69.04 cm²
    (77.8 %, +22.4 % more area).
  - At `+90°`: legacy drops to 52.65 cm² (59.3 %), Anny retains 66.58 cm²
    (75.0 %, +26.5 % more area).
- **Shoulder/axilla surface area within 12 cm of `joint-l-shoulder` (rest =
  756.67 cm²):**
  - At `+75°`: legacy shrinks to 692.99 cm² (91.6 %), Anny retains 760.02 cm²
    (100.4 %).
  - At `+90°`: legacy shrinks to 667.09 cm² (88.2 %), Anny retains 753.58 cm²
    (99.6 %).

These properties are asserted automatically in `tests/anny_rig.rs`.

## Legacy compatibility

All 19 bone names of the previous skeleton remain valid in poses, presets,
ready-made animations, user pose files, `AddedBone` parents, `PaintedWeights`,
two-bone inverse kinematics and BVH retargeting:

| Legacy bone | Anny target(s) | Distribution rule |
|---|---|---|
| `pelvis` | `root` | Direct |
| `spine` | `spine03`, `spine02` | Equal slerp along chain (1/2 each) |
| `chest` | `spine01` | Direct |
| `neck` | `neck01`, `neck02`, `neck03` | Equal slerp along chain (1/3 each) |
| `head` | `head` | Direct (same name) |
| `clavicle.L` / `.R` | `clavicle.L` / `.R` | Direct (same name) |
| `upper_arm.L` / `.R` | `upperarm01.L/R`, `upperarm02.L/R` | Swing + half twist on `01`, half twist on `02` |
| `forearm.L` / `.R` | `lowerarm01.L/R`, `lowerarm02.L/R` | Swing + half twist on `01`, half twist on `02` |
| `hand.L` / `.R` | `wrist.L` / `.R` | Direct |
| `thigh.L` / `.R` | `upperleg01.L/R`, `upperleg02.L/R` | Swing + half twist on `01`, half twist on `02` |
| `shin.L` / `.R` | `lowerleg01.L/R`, `lowerleg02.L/R` | Swing + half twist on `01`, half twist on `02` |
| `foot.L` / `.R` | `foot.L` / `.R` | Direct (same name) |

For split long bones (`upper_arm`, `forearm`, `thigh`, `shin`), a legacy
rotation `q` is factored as `q = q_swing * q_twist` about the full limb axis
(`b1.tail - b0.head`). Segment `01` receives `q_swing * half(q_twist)` and
segment `02` receives `half(q_twist)`. The net distal orientation satisfies
`q0 * q1 = q` exactly without introducing a mid-bone kink during bending, while
spreading pronation/supination smoothly along the forearm and upper arm.

- **`Rig::reach` and `Rig::limb_end`:** use `Rig::bone_span` so two-bone IK
  spans `upperarm01..upperarm02 -> lowerarm01..lowerarm02` (and legs) whether
  called with legacy names (`LIMBS`) or Anny bone names, materialising any
  pre-existing legacy keys on that limb onto the Anny bones so they are never
  double-applied.
- **`src/bvh_import.rs`:** matches standard BVH joint names onto the rig's
  main bones and aligns each full anatomical span (including `upperleg01..02`,
  `upperarm01..02`, `clavicle..shoulder01`, `wrist..finger3-3`).
- **`src/avatar_shape.rs`:** classifies both Anny and legacy bone names by body
  region so measurement-driven reshaping moves arms, legs and torso cleanly.
- **`src/atelier/pose_ui.rs`:** lists the active skeleton's built-in bones
  (104 for MakeHuman avatars, 19 for non-MakeHuman avatars) plus added bones.
- **Added bones and painted weights:** added bones blend smoothly over Anny's
  hand-painted weights where they are nearest on the surface, leaving distant
  vertices' Anny weights untouched. Legacy bone names in `AddedBone::parent`
  and `PaintedWeights` resolve to their primary Anny bone.
- **Older saved projects:** projects saved before this change carry only the
  24 legacy landmarks in `AvatarPose.joints`; `Rig::build` detects this and
  keeps the 19-bone automatic rig so existing documents reopen with matching
  geometry.

## Non-MakeHuman avatars

`Rig::build` selects the 104-bone Anny rig only when `is_makehuman_body(surface)`
holds (exact 13,380-vertex MakeHuman body topology) and all 112 Anny joint
landmarks are present (or in bones-only mode when all 112 landmarks are
present). Imported OBJ, FBX, glTF and USD avatars, as well as avatars whose
landmarks were estimated from geometry alone, continue to use
`Rig::build_automatic` (19 bones and 24-pass surface-smoothed nearest-bone
weights).

## Limits

- **Linear blend skinning only:** `Rig::skin` remains linear blend skinning
  (LBS), not dual-quaternion skinning (DQS). Hand-painted weights and split
  twist bones substantially reduce volume loss at the shoulder, axilla, elbow
  and wrist, but extreme rotations beyond ~120° still exhibit mild LBS
  shrinkage.
- **No corrective blendshapes or pose-space deformation:** skin deformation is
  purely skeletal.
- **Six helper-referenced midpoints:** the 6 non-body MakeHuman helper vertices
  referenced by `rig.default.json` (`helper-tights` and `helper-hair`) are
  reconstructed from local body triangles rather than packed into
  `macro-targets.bin`. On extreme morphs they track the underlying body
  surface closely (within a few millimetres) rather than reproducing MakeHuman's
  exact clothing/hair helper targets.
- **Excluded bone groups:** facial-expression, tongue and breast bones are
  omitted as in Anny's `"anny"` rig.
- **BVH retargeting scope:** BVH import drives the primary body chain by bone
  direction without axial twist or finger-bone retargeting.
- **Pose parameterization:** `Pose` still stores XYZ Euler angles in degrees
  about world axes at rest (`Rig::transforms`), matching existing FashionCraft
  poses and animations rather than Anny's `local-ref` / `local-bone` bone
  frames.
