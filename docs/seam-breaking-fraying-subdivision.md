# Seams that break, frayed edges and subdividing a selected zone

Three cloth behaviours, written from public descriptions of what such tools do and from
first principles; no numbers from other products.

## Seams that break by themselves

**Model.** A seam is a penalty spring per stitch sample: energy ½ k w |gap|², with k the
seam stiffness (N/m²) and w the seam length the sample stands for. The force it carries is
k w |gap|, so k |gap| newtons per metre of seam is the pull the two pieces put on each other
there. A seam may have a **breaking strength** in N per metre of seam
(`GarmentTemplate::seam_strengths`, by seam id; the inspector shows it per centimetre).
Per seam rather than per fabric, because what gives way is the thread and the stitch, not the
cloth.

**When a stitch breaks** (`garment::seam_break::SeamBreaker`):

- a *stitch* is one interval of the seam's sewing partition (the two Gauss samples of
  `CompiledSeam::compile_until` share it; `StitchSample::span` now records it as shares of the
  seam's length), and its pull is the larger of its samples';
- it must have been **sewn** first: its gap has come within 2 mm (`SEWN_GAP_M`) at least once.
  While pieces are still being drawn together the large pull is how the seam closes, not a load
  on stitches that exist, so assembling a garment never tears it;
- its pull must **stay** above the strength for 0.05 s of simulated time and at least two
  consecutive looks (`BREAK_HOLD_S`, `BREAK_HOLD_STEPS`); a moment below starts the count over,
  so a jolt does not tear it.

**Recording.** A break becomes `Command::BreakStitches`: the broken spans are merged into the
seam's existing `seam_tears` (touching spans merge; past 64 tears the narrowest gaps close up).
So undo, save and reload reuse the manual-tear machinery, and the document stores tears, never
vertex indices. Breaks while a simulation runs join one undo step ("Seam broke") as long as
nothing else was recorded since, so one undo mends everything a pull tore.

**Torn seams keep their contact exclusions.** The solver excludes from contact the vertex pairs
a stitch joins (`VbdSolver::new`). A stretch that tears while its two sides still touch would
lose that exclusion, and the rebuilt solver would refuse to start ("initial intersection" or
"touching features"); this also hit a manual tear made on a draped garment. `CompiledSeam`
therefore keeps the stitch points of its torn stretches (`torn`, not sewn) and
`CompiledSeam::contact_exclusions` gives both kinds; `VbdSolver::new` takes them with one added
line, outside `step`. The two sides of a torn stretch so never collide vertex to vertex with
each other (they still collide with everything else).

**In the app** (`atelier/seam_break_ui.rs`): after each state the worker returns, the UI thread
checks the stitches (cheap: one gap per sample). Breaks are applied as an edit that keeps the
state and keeps playing; the garment is recompiled (tears never remesh) and the worker
generation bumped so the solver is rebuilt from the same positions; the watch carries over
that generation, so other stitches keep their history. Any other new generation (edit, reset,
load) starts the watch over. The status line says which seam broke and at what pull. Setting
or changing a strength (`Command::SetSeamStrength`) is not physics of the compiled garment
(`same_physics` and `only_appearance_changed` ignore it), so it can be lowered while the
simulation runs.

No hook in `vbd.rs` or `gpu_vbd.rs`: detection reads positions only, the GPU path works the
same, and the observation rate is the rate at which the window receives states (each look
adds the simulated time since the last).

**Checked in the window** with `FASHIONCRAFT_DEMO=effects fashioncraft`
(`atelier/demo_effects.rs`): on the T-shirt the app opens with, it frays the hems and a cuff,
turns on *Breaks at* for the right side seam and sets it to three times what its sewn stitches
carry while the shirt hangs, checks the seam holds while it just hangs, then pulls the front
away from the back with the Grab tool and keeps pulling (`Step::DragHold`, new) until stitches
break (the seam's pull rose from about 80 to 260 N/m and it tore over most of its length,
recorded as one tear while the simulation went on), mends it with one Edit › Undo, then
box-selects the chest, subdivides it and simulates again. Its log is in the session folder's
`demo-effects/review.md`.

**Limits.** The penalty gap is what measures the pull, so a weak strength on a stiff seam
breaks at a small gap and a strong one only after a visible gap (k × gap = strength). No
fatigue, no strain-rate dependence, no partial damage: a stitch holds fully or is gone.

## Frayed edges

`TrimKind::Fray { path_m, length_m, density_per_m }`: a trim (nonphysical construction
metadata, panel-local metres) along a path 1 mm inside a cut edge (Materials › Fray edge, on
the selected segment or both sides of the selected seam). Drawn by
`garment::trims::fray_triangles` as loose threads: each starts on the path and runs out across
the edge in the cloth's tangent plane (the material outward direction carried by the triangle's
deformation), drooping a little and curling off the face, with spacing, length (45–100 % of
`length_m`) and direction varying deterministically per thread (hash of the trim id). Each
thread is two short stretches of two crossed thin ribbons (8 triangles), in the piece's fabric
colour slightly lightened or darkened. Length 1–50 mm, 20–2000 threads per metre, at most 8000
threads per trim; length and density are editable in the piece's trim list.

**Render only.** Like every other trim, it changes neither the drape nor any export (a test
compares the OBJ byte for byte with and without fraying). Exporting would mean adding a second
kind of geometry (open ribbons without UVs) to each writer; not done.

## Subdividing a selected zone

3D Garment › Subdivide selected mesh, or *Subdivide* in the Select mesh window: finer
triangles only where cloth is selected, each time about half as long.

**Stored** as `MeshingSettings::refine: Vec<RefineZone>` on the piece's polygon: a zone is the
selected triangles' corners in **material coordinates** plus the longest edge allowed there
(`edge_m`, 55 % of the median longest edge of the selected triangles). No vertex indices, so it
survives remeshing at another resolution and the global particle-distance change (which edits
the other `MeshingSettings` fields and keeps `refine`).

**Meshing.** The piece is meshed as before (constrained Delaunay refinement); then
`meshing::refine_longest_edge` splits every triangle whose centre lies in a zone and whose
longest edge exceeds `edge_m`, by longest-edge propagation (Rivara): when the edge is not also
the longest of the triangle across it, that neighbour is split first. Every split halves an
edge shared by two triangles that both have it as their longest (or one on the outline), so the
mesh stays conforming, the smallest angle is at least half of the smallest the piece had, new
vertices go after the old ones, and triangles away from the zone keep their exact vertices.
This runs before boundary recovery, so midpoints on outlines, holes and internal lines join
those named edges. Vertex budget as for the rest of the piece.

**Drape.** The edit goes through the ordinary path: `state_keeping_drape` transfers the drape to
the new mesh (old points stay where they were, new ones on the cloth between them). *Remove
subdivision* clears the zones of the selected pieces (the mesh is then exactly as before).
Undo, save and reload: the zones are part of the document.

**Not done.** Only drafted (polygon) pieces; the zone follows the selected triangles, it is not
painted with a soft falloff; no coarsening below the piece's own size.

## Tests

- `tests/seam_break.rs`: a stretched seam breaks with a strength below its sustained pull and
  holds with one above its peak; breaks are recorded as tears covering each broken stitch, the
  seam then opens; one undo step, saved and reopened; never breaks before being sewn or for a
  jolt shorter than the hold time; strengths validated and removed with their seam.
- `tests/fray.rs`: thread count follows the density, threads cross the edge outward, no longer
  than asked, in the fabric colour, deterministic; saved, validated; OBJ export unchanged.
- `tests/mesh_subdivide.rs`: zone triangles within the asked edge and about half as long,
  angles at least half the original minimum, far triangles and other pieces unchanged, outline
  ends kept, subdividing again halves again, reload identical, removal restores the original
  mesh; the drape is kept and the subdivided garment simulates with seams closing; zones
  validated, vertex budget respected.
