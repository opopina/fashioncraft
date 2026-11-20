# Contributors

FashionCraft grew out of ClothCraft. This file records where the code comes from and who wrote it,
so that the credit stays with the people who did the work.

## ClothCraft (the foundation)

[ClothCraft](https://github.com/erythrofr/ClothCraft) was created by
[erythrofr](https://github.com/erythrofr) and published under the MIT license. FashionCraft starts
from its commit `16761c5` (8 November 2026): the physics engine (VBD with OGC contact and the
double-precision reference), the garment document model, meshing and sewing, the research record in
`docs/`, and the first desktop editor. The full history of that work is in the original repository.

People recorded as contributors to the original repository:

- [erythrofr](https://github.com/erythrofr): creator and main author
- [mikristian](https://github.com/mikristian)
- [NicTanghe](https://github.com/NicTanghe)
- [FireflyTheReal](https://github.com/FireflyTheReal) (Julien Foss, also recorded as julienfoss):
  authoring and camera tools adapted from his fork; see [the integration notes](docs/firefly-integration.md)

ClothCraft development continues at [NicTanghe/ClothCraft](https://github.com/NicTanghe/ClothCraft).
FashionCraft does not include code from that repository yet; anything taken from it will be listed
here with its source.

## FashionCraft

- Ricardo Rivadeneira ([opopina](https://github.com/opopina)): the additions since `16761c5`,
  including the expanded studio menus and panels, export to OBJ + MTL, glTF / GLB, FBX, Alembic and USD,
  FBX and BVH import, the avatar skeleton and animation, body measurements and tracing, automatic
  sewing, friction and the physical floor, PDF printing and the Spanish interface.

## Third-party material

The bundled MakeHuman assets are CC0, and the workspace look adapts the colour tokens and shape rules
of ArtCraft's apps (Apache-2.0) with their fonts Inter, Archivo and Geist Mono (SIL OFL 1.1); see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
Dependencies keep their own licenses.
