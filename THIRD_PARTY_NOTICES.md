# Third-party material

## MakeHuman assets

`assets/makehuman/base.obj` comes from the MakeHuman community repository at revision `a8bc2d54ff0ac92e78ff71431b1023eda42bf482`. The base mesh is CC0, as distinguished from program code in the [upstream notice](https://github.com/makehumancommunity/makehuman/blob/a8bc2d54ff0ac92e78ff71431b1023eda42bf482/LICENSE.md).

The asset notice is retained at [LICENSE.ASSETS.md](assets/makehuman/LICENSE.ASSETS.md), with [provenance](assets/makehuman/provenance.json). `scripts/prepare-avatar.py` extracts the body, removes helpers, triangulates and normalizes to 1.75 m; `mannequin.obj` and `measurements.json` are derived outputs. No MakeHuman program code is incorporated.

`assets/makehuman/macro-targets.bin` packs MakeHuman's CC0 macro targets (`data/targets/macrodetails`, 348 files, at the same revision; see [its provenance](assets/makehuman/macro-targets.provenance.json)) with the joint helpers of `base.obj`. `scripts/pack-macro-targets.py` makes it from those files; `src/avatar_maker.rs` blends them into new bodies. Only the data is used: the blend is this project's own, and no MakeHuman or MPFB program code is incorporated.

## Dependencies and references

Rust dependencies are resolved by `Cargo.lock` and retain their respective upstream licenses. They are not vendored here. Consult each dependency's manifest/notices when preparing a binary distribution.

Research and public manual references are recorded in `docs/research.md`, `docs/research-patterns.md`, `docs/ogc.md` and `docs/manual-study.md`. Citations do not imply that upstream implementations, assets or trademarks are included or licensed by this project.

## ClothCraft source

FashionCraft is built on ClothCraft by erythrofr and the ClothCraft contributors. Both are
distributed under the MIT license; see [LICENSE](LICENSE), which keeps the original copyright
notice, and [CONTRIBUTORS.md](CONTRIBUTORS.md).
The MakeHuman CC0 notice applies to the bundled assets, not the program source.

## ArtCraft look

The workspace theme in `src/atelier/workspace.rs` follows the look of ArtCraft's own apps: its
colour tokens, hairline rules, square panels and 3 px corners on controls are adapted from
[storytold/artcraft](https://github.com/storytold/artcraft) (`artcraft-native/src/theme.rs`) and
[storytold/craft-launcher](https://github.com/storytold/craft-launcher)
(`crates/ui-egui/src/theme.rs`), both licensed under Apache-2.0. Only values and rules are
adapted; no code or images from those repositories are included.

## Fonts

`assets/fonts/` holds the three typefaces ArtCraft's apps use, taken as distributed in
storytold/craft-launcher (`assets/fonts/`), each under the SIL Open Font License 1.1 with its
licence beside it: Inter (`OFL-Inter.txt`, The Inter Project Authors), Archivo
(`OFL-Archivo.txt`, The Archivo Project Authors) and Geist Mono (`OFL-GeistMono.txt`, The Geist
Project Authors). They are embedded in the studio binary unchanged.

## App icon

The FashionCraft icon's drawing (`assets/branding/source/vicuna-chatgpt.webp`) was generated with
ChatGPT (OpenAI) by the project's author; the icon made from it is distributed with the
repository under its MIT License. See [assets/branding/README.md](assets/branding/README.md).

## Optional Box3D research harness

`experiments/box3d` contains ClothCraft's comparison harness. It requires a
separate checkout of Box3D and does not vendor that project's source. Box3D
retains its own MIT license; keep its notice when distributing a combined
build. The pinned source and experimental limitations are recorded in
[the solver laboratory](docs/solver-lab.md).
