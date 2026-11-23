# Grading: sizes, grade rules and points of measure

A garment is drawn once, in its **base size**. Grading adds the other sizes: each grade point
of each piece grows by a rule, the rest of the piece follows, and every size can be shown,
simulated, measured and exported. This is the base of the manufacturing work that comes after
the drape (markers, plotting, tech packs): a workshop cuts every size from the same pattern.

- **Window:** 2D Pattern › Grading › Sizes and grade rules… (command `grading.table`).
- **Size on show:** the size buttons in the 2D bar under the board, or 2D Pattern › Grading ›
  Next size / Previous size (`grading.next_size`, `grading.previous_size`).
- **Review:** *All sizes* in the 2D bar, or 2D Pattern › Grading › Review all sizes on the
  board (`grading.review`).
- **Grade point on / off** for the selected point (`grading.toggle_grade_point`).
- **Points of measure:** 2D Pattern › Grading › Points of measure (POM)…
  (`grading.points_of_measure`).
- **Exchange:** File › Export › DXF-AAMA / ASTM patterns… writes the `.rul` rule table beside
  the `.dxf`; File › Import › DXF-AAMA / ASTM patterns… reads a `.rul` found beside the file.
- **CLI:** `cloth-template dxf-export` writes the `.rul` too; `cloth-template dxf-inspect`
  reports it; `cloth-template grade-size garment.json out.json --size L` writes one size as an
  ungraded template; `cloth-template pom garment.json [--csv]` prints the measurement table.
- **Agents (MCP):** the `size_chart` tool reads the sizes and the measurement table; the
  commands above run through `command_run`.

The code is `src/garment/grading/` (`mod.rs` the model and its edits, `apply.rs` a size's
geometry, `pom.rs` the measurements, `rul.rs` the rule table, `dxf.rs` the bridge to the DXF
pieces) and `src/atelier/grading_ui.rs` (the windows, the 2D bar and the review).

## The model

The document's panels **are** the base size. `GarmentTemplate::grading` (absent in ungraded
files, which load and save exactly as before) holds:

- **Size groups.** Each has sizes from smallest to largest, a base size and the size on show.
  Sizes have stable ids (`s1`, `s2`…), so renaming one keeps its rules. A size can be hidden
  (kept, but left out of the selector, the review and the export); the base size and the size
  on show cannot be. A panel follows the first group unless it is assigned to another.
- **Grade rules, per panel.** A contour point is named by the boundary that starts at it, the
  same stable name seams, notches and allowances use. A rule gives the point's growth from the
  base size in each size, as X/Y in the panel's material frame (millimetres in the interface,
  X across and Y down the board, the frame of the point's own coordinates in the inspector), or
  an **equal step** between consecutive sizes. Editing one size of a stepped rule writes the
  step out size by size first, so the other sizes keep their growth.
- **Grade points and proportional points.** Every contour point is a grade point unless it is
  made proportional. A proportional point moves between the grade points before and after it,
  in proportion to the base contour's length between them. A panel without any grade point
  does not grow.
- **Curve rules.** A curved boundary's control points follow its ends (at one and two thirds),
  so the curve keeps its shape as its ends move. A curve rule adds a growth to both control
  points, scaled by 4/3, so the curve's middle moves by exactly the growth given.
- **Notches** keep their share of their boundary (proportional), or their base distance from
  the boundary's start (*fixed from the start*) or from its end (*fixed distance from the
  end*: the direction reversed). Each may add an extra distance per size, away from the end it
  keeps. A notch is found by its boundary and its place there in the base size: moving a notch
  in the base size drops its rule (it becomes proportional again). A notch pushed off its
  boundary makes that size unusable, and the window says so.
- **Inside the piece.** Holes, internal lines, graphics, trims, pins, tacks and steam follow
  the outline by mean value coordinates of its movement (Hormann and Floater, *Mean value
  coordinates for arbitrary planar polygons*, ACM TOG 25(4), 2006), which reproduce the
  outline's movement on the outline and any affine movement exactly inside.
- **Graphics** follow their piece and may add an offset and a size (percent of the base,
  proportions locked) per size.
- **Seams** refer to boundary names, so they follow every size. A seam sewn at equal length
  that grows unevenly in another size eases there instead of failing (the window's *Seams in
  each size* lists every difference over 0.5 mm).
- **Points of measure** (POM): a straight distance between two contour points of a piece, or
  a length along one or more boundaries (of one piece or several, the gaps between pieces not
  counted). Each has a name and a tolerance, and is read in every size of the group of its
  first piece, with its difference from the base.

Two sizes of a pattern are never stored as geometry: a size is computed from the base and the
rules when it is needed (`grading::apply`). Rule edits are ordinary undoable edits.

## The size on show

The board always edits the base size: dragging a point changes the pattern, and every size
follows. The size on show is what **compiles**: `GarmentTemplate::compile` grades the template
into the sizes on show first, so the 3D view, the simulation, the 3D exports and every edit's
validation see that size. Picking a size is a view, not an edit: it does not enter the undo
history, and undo keeps the size on show (`apply::keep_shown_sizes`). A rule change in a size
that is not on show is draft data: it does not stop or reset the simulation.

Material coordinates start at zero, so a size that grows past the base's left or top edge is
moved over in the compiled garment, and its placement moved back by the same amount: the cloth
stays where it was in 3D. Captured 3D shapes (frozen, solidified or deactivated pieces) belong
to the base size's mesh and are left out of the other sizes.

## The interface

- **Grading window.** With no sizes, it asks for the sizes (smallest first) and the base. Then:
  the size group (rename, new group, remove), the sizes (rename, shown, in 3D, insert after,
  remove; add a larger or a smaller size), the pieces in the group, and the **grade table** of
  the selected piece: one row per contour point (P1…Pn as the board labels them) and per
  curve, one X and one Y column per shown size. A grade checkbox makes a point a grade point or
  a proportional one (whose growth shows read-only). A field takes sums such as `10+2` or
  `(4+2)/2`; Enter or leaving the field applies it, Esc discards it. *＝* makes the row grow by
  an equal step (the growth of the size above the base). Arrows move the selected point in the
  size on show by a chosen step. Notches and graphics of the piece have their own tables.
- **2D bar.** The shown sizes as buttons (a list when there are more than eight), coloured from
  blue (small) to orange (large), and *All sizes*.
- **Review.** With *All sizes*, every shown size is drawn over the base (the base thick, in the
  application's colour), with grade lines joining each point through its sizes. Otherwise the
  size on show is drawn dashed over the base when it is not the base. With the grading window
  open, the selected piece shows its grade points as squares and its proportional points as
  rings.
- **Points of measure window.** The table (value in each size, the difference from the base in
  brackets, the tolerance), names and tolerances editable, *Save the table as CSV…*. New
  measurements come from the selection: a distance between two points of the selected piece,
  or the length of the selected boundary, or of boundaries collected one by one.

Interface text is in English with Spanish translations (`grading_ui::spanish`, consulted first
by `i18n`).

## Exchange: DXF-AAMA / ASTM with a `.RUL` rule table

### What is written

The DXF holds the pieces in the base size, as before, with:

- `Sample Size:` the base size's name and `Grade Rule Table:` the table's name (the file's
  stem in capitals) in the Style System Text, and `Size:` the base size in each piece.
- A grade rule number, a TEXT `# n`, on every point that carries a rule, at the point:
  - every grade point of the sew line, and the corresponding corners of the cut line;
  - every other point of a written line (sew line, cut line, cutouts and their sew lines,
    internal lines) whose growth differs by more than 0.005 mm from what a reader gives a
    point without a rule: the proportional blend of the tagged points before and after it
    along the line, or no growth on a line with none. That covers proportional points next to
    curve rules, curve points, cutouts and internal lines (which follow the outline);
  - every notch with a notch rule, on layer 4 at the notch.
- A grade reference line on layer 5: horizontal, through the middle of the piece, along +X.
  Rules are given along it.

The `.rul` beside the DXF:

```
ASTM/D13 Proposal 1 Version: D 6673-04     (ASTM files only)
AUTHOR: FashionCraft;FashionCraft;0.2.0
CREATION DATE: 23-11-2026
CREATION TIME: 10:00
UNITS: METRIC
GRADE RULE TABLE: TSHIRT
NUMBER OF SIZES: 4
SIZE LIST: S M L XL
SAMPLE SIZE: M
RULE: DELTA 1
-10.00, -15.00 0.00, 0.00 10.00, 15.00 20.00, 30.00
...
END
```

Growths are measured from the sample size (whose growth is `0, 0`), one `x, y` pair per size
in size-list order, four pairs to a line; millimetres with 2 decimals (metric) or inches with 4
(english), +Y up as in the DXF. Equal growths share one rule.

### What is read

- The header keys in any case and spacing; values split by commas, spaces, tabs or line
  breaks; `RULE: DELTA <integer>` (only DELTA rules); `END` or the end of the file.
- A rule whose sample growth is not zero is measured from the sample instead (noted).
- `NUMBER OF SIZES` that disagrees with the size list is noted; the list is used.
- A `# n` (or `#n`, or `# n, m`: the first number) on layers 1–5, 13 or 80–83 is a grade
  rule number; on layer 4 or 80–83 it grades a notch.
- The first line on layer 5 is the grade reference line: rules are turned from its direction
  into the drawing's. Without one, rules follow the drawing's X axis.

On import, a number on an outline corner (within 0.05 mm) becomes that point's rule; corners
without one become proportional points (unless the outline has none, then nothing grows). When
only the cut line's corners carry numbers, the nearest sew-line corners take them. Numbers on
curve points (within 0.15 mm, the importer refits curves within 0.1 mm) become the curve's rule:
the middle's growth that puts the size's curve nearest its graded points, by Gauss-Newton on
the distance. Numbers on notches become a notch rule (fixed from the start, with the distance
the growth gives in each size). Numbers on cutouts and internal lines are not needed: those
follow the outline. The sizes come from the table; an existing group with the same sizes and
base is reused, else a group is added.

### Choices where the public description is silent or ambiguous

- **Layer of the rule numbers.** The description says both that the number sits "on the layer
  of the point element" and that it is found "on layer 5 or layer 4 for notches". FashionCraft
  writes layer 5 (4 for notches) and reads any of 1–5, 13 and 80–83.
- **Interpolation of points without a rule.** Described as proportional to the neighbouring
  grade points, without saying how. FashionCraft assumes linear by length along the line, and
  writes a rule for every point where that would be off by more than 0.005 mm, so a reader that
  interpolates otherwise still lands within the table's precision on corners.
- **The cut line.** Not said whether a reader grades the cut line from the sew line's rules or
  from its own. FashionCraft numbers both.
- **Grade reference line.** Optional in the description; FashionCraft writes a horizontal one so
  a reader that turns pieces keeps the rules' axis.
- **Version line.** Written only for ASTM, as in the DXF.
- **Size names** in the size list are separated by spaces, so spaces in a name become `_` and
  other characters 7-bit ASCII.
- **Hidden sizes** are left out of the table.
- **One group per file:** pieces of two size groups are refused; export one group at a time.

### Round trip

`tests/grading.rs` writes a graded T-shirt (front and back, S M L XL, base M, seam allowances,
a curved side with a curve rule in XL, a proportional neck point, slit and V notches, one fixed
from the end) as DXF-ASTM with its `.rul`, reads both back into a garment and compares every
size. Measured on 2026-11-23:

- every corner within **0.009 mm** in every size;
- outlines within **0.14 mm** in the base size (the importer's curve refit, as without
  grading) and **0.34 mm** in XL, whose side bows 3 mm by its curve rule: the refitted curve is
  parameterised differently, and one middle growth cannot follow that exactly;
- notches within **0.20 mm**.

`tests/fixtures/grading/handmade.rul` is a table written by hand from the description, apart
from the writer.

## Sources

- S. Bakker, *ePattern ASTM Standard*, DH Patterns and Fit, 2011–12, the "Grading" section:
  grade reference lines, grade points and their `# <ID>` texts, the rule table's syntax and an
  example table, graded nests.
  <https://dorthehansen.com/wp-content/uploads/2014/10/ePattern-ASTM-Standard.pdf>
- Grafis help, *Export formats and their characteristics*: the base size in the DXF, the other
  sizes as a rule table in a separate `.RUL` file.
  <https://help.grafis.de/en-gb/Content/ExportIImport/Exportformate_und_deren_Besonderheiten.htm>
- K. Hormann, M. S. Floater, *Mean value coordinates for arbitrary planar polygons*, ACM
  Transactions on Graphics 25(4), 2006.

ASTM D6673 itself is a paid standard and was not consulted. No other application's files or
code were read.

## Limits

- Other sizes are edited through the grade table and the arrows; dragging on the board edits
  the base size.
- One avatar for every size; the 3D view simulates the size on show, one at a time.
- In a size other than the base, things placed by clicking the 3D cloth (pins, tacks, graphics
  on the 3D garment) are recorded at the clicked material point as if it were the base size.
- Graded nests (one block per size) are neither read nor written; alternate grade reference
  lines are not read.
- A notch's rule follows its place in the base size: moving the notch drops the rule.
- Curve rules move both control points alike; a curve that changes shape unevenly between sizes
  needs a corner (a point) in between.

## Tests

`tests/grading.rs`: steps and per-size rules, proportional points, curve rules, notch modes,
seams, graphics and the inside following each size, the size on show compiling, sizes added,
inserted, renamed and hidden, points of measure in every size, persistence (old files load
unchanged), undo keeping the size on show, sums in fields, the rule table written and read
(also by hand), DXF with `.rul` out and back, and two groups refused in one export.
`src/garment/grading/tests.rs`: the interpolation reproduces affine movements inside a concave
outline and keeps the outline's own, and the notch modes. `src/atelier/dxf_aama_ui.rs`: a
graded export writes its `.rul`, and the import review finds it beside the DXF (any case of
`.rul`), brings the pieces in graded, or ungraded when its box is unticked.

In the window, `FASHIONCRAFT_DEMO=grading fashioncraft` drives the real interface in Spanish
on the T-shirt FashionCraft opens with: it opens the grading window from the menu, creates
S M L XL with base M, picks the hem's corner and types `10+2` into L's X field (12 mm), turns
it into an equal step, shows XL from the 2D bar (the simulated cloth is wider) and drapes it,
lays every size over the base, undoes and redoes the rules (XL stays on show), opens the
points of measure (chest 20 mm a size) and exports the DXF-ASTM with its `.rul`. On
2026-11-23 it ran its 65 steps with none failing.
