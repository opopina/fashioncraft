# Fabric kit: how the fit works

Design note for `src/fabric_kit.rs` (fit, virtual tests, file format) and `src/atelier/fabric_kit_ui.rs` (Materials › Measure fabric…). The workshop procedure is in [fabric-kit.md](fabric-kit.md).

## What is fitted, from what

| `FabricProperties` field | From | How |
|---|---|---|
| `density` | weight of a known area | direct: g/m² ÷ 1000 |
| `thickness` | caliper | direct: mm ÷ 1000 |
| `warp`, `weft` | warp and weft strips under weights | search on the virtual strip |
| `shear` | bias strip, warp and weft known | search on the virtual bias strip |
| `bend_warp`, `bend_weft` | warp and weft cantilever overhangs | coupled search on the virtual cantilevers |
| `compression_ratio`, `compression_transition` | not measured | library defaults (0.15, 0.005); the kit has no in-plane compression test, and a fabric buckles long before it compresses |
| `internal_damping` | not measured | 0, as the library's presets |
| `collision_distance` | not a fabric measurement | none (the scene's) |

Friction comes from the slide angle (μ = tan θ). The simulation has one friction coefficient per scene (`VbdSettings::friction`), so it is kept with the measurements and the window offers to put it in the scene; it is not a fabric field.

## The virtual tests

Each test is a FashionCraft mesh solved by the CPU reference solver (`solver::ReferenceSolver`: implicit Euler, Newton, PCG, Armijo) on exactly the membrane and hinge energies of `energy.rs` that VBD minimizes in the editor. Only static equilibrium is wanted, so masses are scaled down by 10⁴ and gravity up by 10⁴: every weight is unchanged, inertia becomes a tiny proximal term and each implicit step at dt = 0.1 s is a static Newton solve. A test has settled when a step starts with its residual force below the tolerance (10⁻⁶ of the load per clamp vertex for strips, 10⁻⁶ of a vertex's weight for cantilevers). Equilibrium does not depend on the time integrator, so this is the fabric the editor's VBD converges to.

The tests use `model::Pattern`'s regular grid (alternating diagonals) so that clamps, marks and the bench edge fall on rows.

**Strip.** The measured width and free length, eight cells across, square cells. Grain 90° (warp strip), 0° (weft) or 45° (bias) puts the strip's length along the fibre being tested. The top row is pinned; the bottom row is pinned lower by a drop δ, like a rigid clamp. For each load, a secant search on δ finds the equilibrium whose bottom clamp carries the load: the clamp's force is the cloth's pull on its row (−Σ∂E/∂y from the membrane and hinge elements there) less that row's own weight. The strip's own weight is in the solve, as on the real strip. Loads are solved smallest first, each from the last equilibrium, and each search evaluation starts every load's drop from the ratio to the constitutive law's drop that the previous evaluation found. The gauge is the distance between the material points of the two marks on the strip's centre line (linear in the element, so exact for the mesh).

**Cantilever.** A strip with 32 cells along the overhang and 3 across, square cells (rigidity is per width and the model has no Poisson coupling; three cells across bend within 2 % of a wide strip, two do not). `Pattern` wants 10 mm of width, so the grid is built larger and shrunk. Its first two rows lie pinned on the bench; the third row is the bench edge. Newton starts from the exact heavy elastica (below) for the rigidity the last solve showed, a few iterations from the discrete equilibrium. The result is the chord angle from the bench edge to the tip.

**Known difference from the workshop.** The virtual bottom clamp cannot drift sideways; a hanging one can. Warp and weft strips do not try to; a bias strip of a balanced fabric does not either, an unbalanced one a little. The virtual strip has no Poisson contraction because the membrane model has none.

## The searches

`search` finds the root of a residual that falls with ln x at a slope near −1: it steps by that slope until the root is bracketed, then runs Illinois regula falsi in ln x, to a relative tolerance of 10⁻³. At a bound that cannot bracket the root it stops there and the fit says so (`Note::AtLimit`).

1. **Warp, weft**: residual = weighted mean of ln(simulated extension / measured extension) over that strip's readings, each weighted by its measured extension (a ruler step matters less on a longer reading). Start: the constitutive law's modulus for each reading, geometric mean.
2. **Shear**: the same residual on the bias strip, warp and weft fixed. Start: the small-strain bias compliance 1/k₄₅ = (1/k_warp + 1/k_weft + 1/G)/4. If the bias strip is stiffer than warp and weft allow at any shear, the search stops at its limit.
3. **Bending**: residual = ln λ(θ) − ln λ\*, where θ is the virtual cantilever's chord angle at the measured overhang, λ(θ) the elastica's λ for that angle and λ\* the ramp's. The warp and weft rigidities share hinges, so the two searches alternate until neither moves by more than 0.2 %. With one direction measured both are equal; with none, the default stays.
4. **Check**: every test runs once more with the fitted fabric; for each cantilever a search on the overhang finds where the virtual strip touches the ramp. The table in the window lists each reading beside the simulated one.

Notes tell the user what was copied or estimated, which readings are too coarse for the ruler (one resolution step > 15 % of the largest extension) and when one stiffness per direction cannot follow the measured curve (worst extension off by > 15 %).

A fit with all tests is about 60 settles: about 30 s in a release build on a busy laptop (one core; the window shows the stage and the seconds and can cancel), a few seconds at the tests' coarser resolution.

## Analytic references

**Constitutive law.** For an axis of modulus k, the nominal traction is dW/dλ of `energy::axial`'s smoothed Green-strain law (`fabric_kit::axial_traction`, the same as `fabrics.rs`). Test: the virtual warp and weft strips give the law's strain at 40 and 200 N/m within 0.5 % (the clamps and the strip's weight make the rest).

**Heavy elastica.** A strip clamped horizontally and free at the tip, bending under its own weight q per length with rigidity G: φ'' = −λ(1 − s)cos φ, φ(0) = 0, φ'(1) = 0, λ = qL³/G, solved by shooting with RK4. The chord reaches 41.5° at **λ\* = 8.0042**. The textile standard's G = w·g·(O/2)³ assumes λ = 8, so the 41.5° ramp makes its formula exact to 0.05 %; Peirce chose the angle for that. In the small-deflection limit the chord angle is λ/8 (tip drop qL⁴/8G), checked to 0.2 %.

**The hinge model's rigidity on a mesh.** `bend_warp` and `bend_weft` are coefficients of the flat-rest hinge energy ½·3|e|²/(A₁+A₂)·B·|n₁−n₂|²; the continuum rigidity they produce depends on the mesh. A smooth cylinder interpolated on equilateral triangles gives G = 1.5·bend_warp + 0.5·bend_weft along the warp (2B for an isotropic B); on the test grid's rows it gives 3B. The equilibrium relaxes below the interpolated cylinder. Measured on the virtual strip from a short overhang in the linear regime:

| Cells along the overhang (3 across) | G / B (isotropic B) |
|---:|---:|
| 16 | 2.21 |
| 24 | 2.28 |
| 32 | 2.31 |
| 48 | 2.34 |

Width matters little (a wide strip is about 2 % stiffer than one 3 cells across), so this is mostly not an edge effect: the factor grows slowly with resolution along the bend. **This is why the fit searches through the solver instead of using a formula**: whatever the factor, the fitted coefficients make the virtual cantilever touch the ramp at the measured overhang. The test `virtual_cantilever_matches_the_analytic_bending_length` takes the rigidity a short virtual cantilever shows in the linear regime, puts it in the exact elastica and finds the overhang at which the long virtual cantilever, far past linear theory, touches the 41.5° ramp within 1 % (0.3–0.7 % observed).

The consequence for garments: the fit is made at 32 cells along the overhang (about 2 mm cells for a 60 mm overhang). Garment meshes are coarser (10–30 mm triangles) and of the mesher's even, non-grid kind; with the same coefficients they show a somewhat different rigidity, within the spread of the table above for moderately coarser meshes. A per-resolution calibration of the hinge model is future work on the solver side, not something a fabric file can fix.

## Validation

`tests/fabric_kit.rs`:

- `virtual_strips_follow_the_constitutive_law`: above.
- `virtual_cantilever_matches_the_analytic_bending_length`: above, warp and weft of an anisotropic fabric; also checks λ\* against the standard's 8 and the bending length c = O/2 against (G/q)^⅓.
- `a_synthetic_fabrics_readings_give_back_its_parameters`: a fabric of 2400 / 1500 N/m, shear 70 N/m, bending 15 / 9 µN·m, 120 g/m² gives readings in the virtual tests (three warp, three weft and two bias loads, two overhangs, a slide angle); the fit of those readings returns **every parameter within 1 %** (stated tolerance), no notes, and reproduces every strip reading within 0.02 mm and every overhang within 0.2 mm.
- `readings_as_a_ruler_gives_them_still_give_the_fabric`: the same readings rounded to 0.5 mm (strips) and 1 mm (overhangs) still give every parameter within 3 %. At the window's resolution the rounding moved them by at most 1.2 % (warp), and the exact readings gave them back within 0.12 %.
- `missing_tests_are_filled_in_and_said`, `coarse_readings_and_impossible_ones_are_reported`, `measurements_are_checked_before_fitting`, `a_measured_fabric_file_keeps_its_measurements`.

The tests use coarser virtual meshes than the window (6 and 16 cells) so they run quickly in a debug build; the window uses 8 and 32.

## File format

A measured fabric is a JSON file with the extension `.fcfabric`, saved in the library's `Fabrics` folder:

```json
{
  "format": "fashioncraft.fabric",
  "version": 1,
  "name": "Cotton poplin",
  "properties": { "warp": 2400.0, "weft": 1500.0, "shear": 70.0, "...": "FabricProperties" },
  "measurements": { "gsm": 120.0, "thickness_mm": 0.25, "warp": [{ "load_g": 500.0, "length_mm": 101.0 }], "...": "Measurements" },
  "friction": 0.45,
  "checks": [{ "test": { "strip": { "direction": "warp", "load_g": 500.0 } }, "measured_mm": 101.0, "simulated_mm": 101.02 }],
  "notes": ["shear_estimated"]
}
```

`properties` is the existing `FabricProperties` (the template's fabric), so applying it is the ordinary `Command::Fabric` edit with undo; nothing in the project format changes.

## Sources

Public descriptions only: ASTM D1388 (cantilever, 41.5°, bending length = overhang/2, G = W·c³); F. T. Peirce, *The handle of cloth as a measurable quantity*, J. Textile Institute 21 (1930), for the cantilever's theory; the FAST system (CSIRO; Minazio, *FAST – Fabric Assurance by Simple Testing*, 1995) for extension at 5, 20 and 100 gf/cm and bias extension; ISO 13934-1 for 50 mm strips; the incline method for the static friction coefficient; the public product pages of desktop garment tools' fabric kits for the workflow only. No measurement, parameter or file of another product is used.
