# Blender interchange fixtures

`scene.abc` and `scene.usda` were written by Blender 5.2 LTS (headless, factory settings) with
`make.py`, from two primitives made in the script itself: a 12 × 8 UV sphere moved, turned and
stretched, and a cube under a turned and raised empty. `expected.json` holds the corners of both
in world space, turned Y-up as `(x, z, −y)`, and their vertex and triangle counts, as Blender
reported them. No other content is included.

Regenerate with:

```sh
Blender -b --factory-startup --python tests/fixtures/blender-io/make.py -- tests/fixtures/blender-io
```
