# FashionCraft

Open-source 3D garment design and cloth simulation, written in Rust.

Draw patterns, sew them together, place them around an avatar, simulate the drape and export the result to other 3D tools.

FashionCraft is built on [ClothCraft](https://github.com/erythrofr/ClothCraft), created by erythrofr with the ClothCraft contributors under the MIT license. Its physics engine, document model and first editor come from there; see [CONTRIBUTORS.md](CONTRIBUTORS.md) for who did what.

**FashionCraft is early alpha, not a production garment tool.** Expect unfinished interactions, residual oscillation, slow simulation on dense meshes and cases that still fail. Help is welcome with the interface, physics, testing, documentation and file formats. You do not need to be a programmer to contribute a useful reproduction, workflow sketch or testable hypothesis.

![FashionCraft workspace](docs/screenshots/workspace.png)

## Try it

Install Rust stable and run from the repository root:

```sh
cargo run --release --locked -j 2 --bin cloth-fit -- --steps 0 --view
```

The editor opens a T-shirt and the bundled MakeHuman mannequin. Windows is the main development platform; macOS and Linux need equivalent acceptance testing. The desktop requires a graphics adapter supported by wgpu. CPU command-line tools build without graphics dependencies.

To reopen a saved garment:

```sh
cargo run --release --locked -j 2 --bin cloth-fit -- --project path/to/garment.json --steps 0 --view
```

Use **Space** to start or pause, **Grab** to pull the cloth and **Ctrl+S** to save. Grab exposes radius and strength above the 3D view; **3D gear → Clean view** hides editing annotations. Right-click a canvas for the radial tool menu. **Q** opens nearby shape, mirror and boolean actions. The 2D board and 3D viewport share panel and boundary selections.

## What works today

- **Patterns:** rectangles, ellipses, arc sectors, Bézier contours, point editing, holes and internal sewing paths; independent and paired linked mirrors; two-piece booleans.
- **Sewing:** whole boundaries, partial ranges and ordered multi-edge chains, with direction, explicit easing and editable relationships.
- **Arrangement:** translation and rotation in 3D; a separate 2D board layout; undo/redo and semantic copy/paste.
- **Simulation:** CPU and hybrid GPU VBD with OGC contact, smoothed Coulomb friction, a physical floor, gravity and quality presets, mouse grabbing and passive motion diagnostics.
- **Documents:** Open, Save, Save As, portable avatar assets and local recovery. Keep a saved JSON and its adjacent `.assets` folder together.
- **Starting assets:** six bundled pattern templates, native template JSON import, and static OBJ/glTF/GLB avatars through File → Import avatar. Avatar replacement preserves authored patterns but resets the drape and undo history; [supported geometry and limits](docs/avatar-import.md).
- **Avatars:** Avatar → Avatar library… offers twelve ready-made bodies (women, men and children of several builds, ages and ancestries) with skin, eyes, eyebrows, eyelashes, hair and neutral underwear; Avatar → Create avatar (MakeHuman)… makes a new one from gender, age, build and twenty measurements, with its look. Bodies and looks are MakeHuman's CC0 assets; the settings follow NAVER LABS Europe's Anny.
- **Avatar inspection:** Properties → Simulation reports the actual collision mesh and can check the current arrangement through the CPU solver's initialization gate. [Coverage and limits](docs/avatar-diagnostics.md): topology warnings are not an intersection or global containment certificate.
- **DXF outlines:** File → Import DXF patterns previews units, layers and excluded data before adding selected closed outlines as editable pieces. Existing unchanged panels retain their drape. This is a [generic planar subset](docs/pattern-import.md), not industrial DXF-AAMA/ASTM.
- **SVG outlines:** File → Import SVG patterns adds selected closed outlines from a [physically scaled SVG subset](docs/svg-import.md). Curves are sampled into polygon edges; unsupported artwork and nested or overlapping selected outlines are rejected rather than interpreted as sewing or holes.
- **Net material report:** the CPU `cloth-template metrics` command reports
  rest-mesh area and cloth mass per panel and fabric. It is not cutting or
  purchasing consumption; see [units and limits](docs/production-metrics.md).

Select two boundaries in Sew, then confirm the nearby proposal; unequal lengths offer an explicit easing action. In Edit, box/lasso can select contour points across patterns for one group move; Delete removes selected pieces when no point, seam or text field owns the action. See [editing and inspection controls](docs/editor-basics.md).

Added in FashionCraft on top of ClothCraft:

- **Export:** OBJ with a material library, glTF 2.0 / GLB (optionally with the avatar and a recorded take), FBX 7.4, Alembic and USD / USDZ; quad retopology along the grain and welded seams for export.
- **Avatar:** FBX import of static meshes, an automatic skeleton with skin weights, keyed poses and ready-made motions, and BVH motion retargeted onto the avatar.
- **Body tools:** measurements along the body or the draped garment, and patterns traced directly on the avatar.
- **Sewing:** automatic sewing of unsewn segments that pair up around the body.
- **Production:** a 1:1 print sheet as PDF.
- **Interface:** English and Spanish.

These are implemented foundations, not completed production workflows. See the [workspace guide](docs/workspace-ui.md), [document and sewing guide](docs/documents-and-sewing.md) and [authoring reference](docs/authoring.md) for controls and limitations.

## Physics: strengths and open problems

The interactive engine uses **Vertex Block Descent (VBD) with Offset Geometric Contact (OGC)**. GPU execution is hybrid: candidate search and local solves run on the GPU, while precise contact geometry and movement bounds remain on the CPU. A double-precision Newton reference is also available.

Contact protections are designed to preserve separation from an initially separated arrangement, within the documented geometric assumptions and local seam exclusions. They do not repair arbitrary initial intersections or guarantee every possible garment. See the [contact contract](docs/ogc.md).

The main open problems are residual motion under tension, convergence, cost at fine mesh resolutions, friction, seam treatment and measured material calibration. Some test scenes run in real time; that does not establish a universal performance guarantee. **Rendering FPS and simulation speed are different:** a smooth viewport can show cloth progressing slower than wall time. Comparisons must report simulated seconds per wall second and full-step timings, alongside collision and deformation checks.

The [headless solver lab](docs/solver-lab.md) contains experimental XPBD/OGC, finite-stiffness AVBD adaptations and a separate native Box3D particle lattice. Their failures are recorded; they are not alternative editor modes. Small contact research kernels are also not garment-ready solvers. No alternative has been selected to replace the current engine.

The long-term goal is interchangeable physics behind a stable document and authoring model. There is no public solver plugin ABI yet. A useful solver contribution must preserve collision coverage and fluid pull/release behavior, not just reduce a jitter number.

## Files

| Data | Input | Output |
|---|---|---|
| Editable garment | Project / template JSON (also files saved by ClothCraft) | Project / template JSON |
| Avatar | OBJ, static glTF / GLB, static FBX meshes; BVH motion | glTF / GLB, FBX |
| Simulated garment mesh | — | OBJ + MTL, glTF / GLB, FBX, Alembic, USD / USDZ |
| Print | — | 1:1 PDF sheet |
| 2D patterns | Selected closed outlines from generic planar ASCII DXF and physically scaled SVG subsets | Annotated SVG preview and millimetre-scale sewing outlines with a 100 mm ruler; no allowances or tiling |
| Research data | — | Metrics, fiber frames and geometric grain guides |

DXF-AAMA/ASTM, Alembic and USD import and grading rules remain planned. CLO and Marvelous Designer project files are not supported. Save preserves the editable document; Export writes separate interchange artifacts. See the [input/output contracts](docs/interoperability.md).

## Help build it

**Contributing with an AI agent? Give it [COORDINATION.md](COORDINATION.md).**
This repository hosts the Rust rewrite task catalog and a shared assignment
board for contributors. The [EXO feature audit](docs/exo-feature-audit.md)
and [atomic backlog](coordination/TASKS.md) identify gaps, existing PRs and
acceptance checks. Agents claim scoped work and leave public handoffs; the
coordinator detects overlapping assignments. It does not launch AI sessions
or merge changes automatically.

Choose a track from the [roadmap](ROADMAP.md):

- **Make editing understandable:** sewing feedback, selection, arrangement, measurement and accessibility. An unfamiliar person should be able to make a simple garment without reading implementation details.
- **Improve the solver:** profile full steps, investigate contact continuity and convergence, or build an alternative behind the same benchmark contract. Retain failed experiments and disclose tradeoffs.
- **Connect other tools:** implement a small, tested import or export slice with explicit units and supported entities.
- **Test real workflows:** share an original garment you may redistribute, exact steps, settings and expected behavior. A small failing example is valuable.

Start with [CONTRIBUTING.md](CONTRIBUTING.md). The [contributor kit](docs/contributor-kit.md) includes task briefs, an issue checklist and prompts you can give a coding assistant. Agents should read [AI_START_HERE.md](AI_START_HERE.md) and [AGENTS.md](AGENTS.md) before editing. Contributions need human review and the same evidence whether written by a person, an AI assistant or both.

## Development

```sh
cargo test --locked --no-default-features -j 2 -- --test-threads=2
cargo clippy --locked --no-default-features --all-targets -j 2 -- -D warnings
cargo build --locked -j 2
```

On Windows, `scripts/check-cpu.ps1` runs the CPU checks. GPU tests and native window checks are opt-in; coordinate GPU use with the machine's owner. The [headless physics lab](docs/directional-contact.md#headless-laboratory) and [settling zoo](docs/settling-zoo.md) provide reproducible fixtures without opening the editor.

| Area | Start here |
|---|---|
| Documents and editing | `src/document.rs`, `src/fitting.rs`, `src/authoring.rs` |
| Patterns, meshing and seams | `src/garment/` |
| Desktop workspace | `src/atelier/` |
| Material energy and reference solver | `src/energy.rs`, `src/solver.rs` |
| Interactive physics and contact | `src/vbd.rs`, `src/ogc/`, `src/gpu_vbd.rs` |
| Research adapters | `src/solver_lab/`, `examples/solver_compare.rs` |

The [architecture](docs/architecture.md), [handoff](docs/handoff.md) and [solver challenge](docs/solver-challenge.md) explain the contracts and evidence.

## License and credits

Original source and documentation are [MIT-licensed](LICENSE). The bundled MakeHuman assets retain their [CC0 notice and provenance](assets/makehuman/LICENSE.ASSETS.md) (the avatar looks: [their provenance](assets/avatars/provenance.json)); dependencies retain their own licenses. The project icon is documented in the [artwork record](assets/branding/README.md).

FashionCraft is built on ClothCraft by erythrofr and the ClothCraft contributors; their copyright notice is kept in [LICENSE](LICENSE) and [CONTRIBUTORS.md](CONTRIBUTORS.md) lists the people involved. Some authoring and camera tools were adapted from FireflyTheReal / julienfoss's fork; the [integration notes](docs/firefly-integration.md) record provenance and fixes.

FashionCraft is not affiliated with CLO or Marvelous Designer; those names are trademarks of their owners. Public manuals inform workflow research; proprietary code, icons and garment examples are not included. The [manual study](docs/manual-study.md) records the references consulted.
