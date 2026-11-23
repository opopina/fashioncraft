# UV editor and UV layout

**Materials › UV › UV editor** (also **Window › UV editor**) shows every piece's *island*: its
flat pattern placed in the texture square that exports and the baked atlas use. The user moves,
turns and scales islands, and packs them into the square without overlap. Written from the
public descriptions of UV editors in garment tools and observed behaviour as a user; no
proprietary file or number was read.

## What the layout is, and what it is not

- **Export metadata, never the look in the editor.** The 3D view keeps drawing fabric
  textures in material space (`render.rs` uses the mesh's material coordinates, the piece's
  grain, its texture position and the fabric's repeat). Moving an island changes only what the
  exported texture coordinates and the baked atlas show. `uv_layout` is cleared in
  `authoring::same_physics`, counted by `authoring::only_appearance_changed` (so an edit neither
  resets nor pauses the simulation) and left out of the fitting project's mesh signature (the
  saved drape survives a layout edit).
- **Per piece, anchored in material space.** `GarmentTemplate::uv_layout` maps a piece id to a
  `UvPlacement { center, rotation_degrees, scale }`: the centre of the piece's material bounds
  lands on `center`, the piece turned by `rotation_degrees` (clockwise as drawn, v down, like a
  piece's board rotation) and scaled by `scale` texture units per metre. No vertex index is
  stored, so remeshing, or an edit that moves the piece's material origin (`normalize`), keeps
  the island where it was. Deleting a piece drops its island (`prune_simulation_settings`).
- **Texture space** runs u right, v down, (0, 0) at the image's top-left (glTF and the atlas
  read it so). OBJ, FBX, USD and Alembic count v from the bottom, so they write `1 − v`.

## Automatic layout

With no placement at all the whole garment is packed with the default options (1 % margin, no
quarter turns, one texel density). When some pieces are placed and others are not (a piece added
or pasted after editing), the missing ones go into the free space at the placed pieces' median
texel density; what does not fit goes in a column beside the square and the editor counts it as
"outside the square". **Automatic layout** forgets every placement.

## Packing

`uv_layout::pack` packs islands by their bounds: each island's bounding rectangle (after its
turn), grown by the margin, goes into a square inset by half the margin with MaxRects (best
short side fit), so islands end up at least a margin apart and a margin from the edges. The
common scale is the largest one that fits, found by bisection, for four orders (longest side,
area, height, width) and the best one kept. Options: margin (0–20 %), allow quarter turns, keep
the islands' relative sizes. Packing is by bounds, not by outline: L-shaped or round pieces
leave space unused (nesting by outline is not done).

## Editing

Click selects (Shift adds), drag moves, a corner handle scales about the selection's centre, the
round handle turns (Shift snaps to 15°), a drag on empty space selects with a box, wheel zooms,
middle or right drag pans, Escape cancels a drag. The inspector row edits the selected island's
centre, turn and scale numerically; ⟲/⟳ turn the selection a quarter. Overlapping islands are
outlined red, islands leaving the square orange. Every change (a drag on release, a field, a
button, Pack) is one `Command::SetUvLayout` and so one undo step; the first edit of an automatic
layout writes every piece's placement, so later edits never repack the others.

Registry commands: `materials.uv_editor` (opens the window), `materials.pack_uv` and
`materials.automatic_uv_layout` (template history).

## Exports

| Output | Fabric coordinates | UV layout |
|---|---|---|
| OBJ | `vt` by default | `vt` with **Unified UV** (v from the bottom) |
| Baked atlas (`bake.rs`) | — | islands painted where the layout puts them; `Atlas::uv` is the layout |
| glTF / GLB | `TEXCOORD_0` (the material's texture) | `TEXCOORD_1` |
| FBX | `UVMap` | `UVLayout`, its own layer |
| USD | `primvars:st` | `primvars:uvLayout` |
| Alembic | `uv` | `.arbGeomParams/UVLayout` |

## Checks

- `tests/uv_layout.rs`: packing (7 pieces, curved ones included, margins 0–3 %, with and without
  quarter turns, relative sizes) leaves no overlap by outline and keeps the margin by bounds;
  pieces added later go into free space; an edit is one undo step, keeps physics and the fabric
  coordinates, refuses bad placements, loses the island of a deleted piece; the project file
  round-trips the layout and an automatic layout is not written; every export and the atlas
  carry the layout's coordinates.
- `cargo run --example uv_layout_export -- DIR` then
  `Blender -b --factory-startup --python scripts/blender-uv-check.py -- DIR` imports every
  format into Blender and compares the layout map with the expected coordinates.

## Not done

- Packing by outline (nesting), packing only the selection, pinning islands while packing.
- Mirroring islands, per-island texel-density targets in pixels, UDIM tiles.
- Showing the fabric texture or the baked atlas under the islands in the editor.
