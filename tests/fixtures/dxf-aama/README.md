# DXF-AAMA / DXF-ASTM samples

None of these files comes from Marvelous Designer or CLO.

- `handmade-aama.dxf`: written by hand (2026-11-21) from the public descriptions listed in
  `docs/dxf-aama-astm.md`, not by FashionCraft's writer. An AAMA file in millimetres, with no
  version line. It holds:
  - a front piece whose sew and cut lines include an arc (bulged vertex);
  - a slit and a V notch as points, and a slit notch drawn as a line;
  - two grade rule numbers (`# 1`, `# 2`) on layer 2;
  - a grain line, an internal line, a drill hole and an annotation;
  - a pocket whose cut line is four separate LINEs (no sew line);
  - a mirror line, a SPLINE and a line on a layer named `CORTE`, which the reader must list as
    not imported.
- `handmade-astm.dxf`: written by hand the same way (laid out with a small script that only
  prints group pairs). An ASTM file in inches, with the version line
  `ASTM/D13Proposal 1 Version: D 6673-04`. It holds:
  - a back yoke with a 3/8 in allowance, inserted turned 90°;
  - a T notch (layer 80), a check notch (82) and a slit (4);
  - a quality validation curve (84);
  - a cuff with an internal cutout (11).
- `sample-tshirt-aama.dxf`, `sample-tshirt-astm.dxf`: the sample garment of
  `tests/dxf_aama.rs`, as FashionCraft writes it. It is a T-shirt front and back with a scooped
  neckline, shaped sides, allowances and slit/V notches, plus a bias-cut pocket. The test fails
  if they fall out of date; run `DXF_AAMA_BLESS=1 cargo test --test dxf_aama` to write them again.
