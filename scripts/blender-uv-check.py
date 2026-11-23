"""Import the files `cargo run --example uv_layout_export -- DIR` wrote into Blender and check
that each one carries the edited UV layout.

    Blender -b --factory-startup --python scripts/blender-uv-check.py -- DIR

For every format it lists the UV maps Blender made and compares the layout map with
`DIR/expected.json` (each piece's layout coordinates, v up): every texture coordinate Blender
read must be one of the piece's expected points and every expected point must be read, within
1e-5. Exits with status 1 when a format fails.
"""
import json
import os
import sys

import bpy
from mathutils import Vector
from mathutils.kdtree import KDTree

TOLERANCE = 1e-5


def tree(points):
    t = KDTree(len(points))
    for k, (u, v) in enumerate(points):
        t.insert(Vector((u, v, 0.0)), k)
    t.balance()
    return t


def compare(read, expected):
    """Largest distance from a read coordinate to the expected ones and back."""
    if not read or not expected:
        return float("inf")
    worst = 0.0
    a, b = tree(expected), tree(read)
    for u, v in read:
        worst = max(worst, a.find(Vector((u, v, 0.0)))[2])
    for u, v in expected:
        worst = max(worst, b.find(Vector((u, v, 0.0)))[2])
    return worst


def meshes():
    return [o for o in bpy.context.scene.objects if o.type == "MESH"]


def uv_points(obj, name):
    layer = obj.data.uv_layers.get(name)
    if layer is None:
        return None
    return [(d.uv[0], d.uv[1]) for d in layer.data]


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def check(label, importer, path, layout_names, expected, per_piece):
    clear()
    importer(path)
    objs = meshes()
    names = sorted({uv.name for o in objs for uv in o.data.uv_layers})
    layer = next((n for n in layout_names if n in names), None)
    if layer is None:
        print(f"{label}: FAIL no layout UV map among {names}")
        return False
    worst = 0.0
    if per_piece:
        for o in objs:
            piece = o.data.name if o.data.name in expected else o.name.split(".")[0]
            if piece not in expected:
                print(f"{label}: FAIL object {o.name} is no piece of {sorted(expected)}")
                return False
            worst = max(worst, compare(uv_points(o, layer), expected[piece]))
    else:
        read = [p for o in objs for p in (uv_points(o, layer) or [])]
        every = [p for points in expected.values() for p in points]
        worst = compare(read, every)
    ok = worst <= TOLERANCE
    print(
        f"{label}: {'PASS' if ok else 'FAIL'} {len(objs)} meshes, UV maps {names}, "
        f"layout map {layer!r}, largest difference {worst:.2e}"
    )
    return ok


def main():
    directory = sys.argv[sys.argv.index("--") + 1]
    with open(os.path.join(directory, "expected.json")) as f:
        expected = {k: [tuple(p) for p in v] for k, v in json.load(f).items()}
    path = lambda name: os.path.join(directory, name)
    results = [
        check(
            "OBJ unified UV",
            lambda p: bpy.ops.wm.obj_import(filepath=p),
            path("garment.obj"),
            ["UVMap"],
            expected,
            True,
        ),
        check(
            "OBJ baked atlas",
            lambda p: bpy.ops.wm.obj_import(filepath=p),
            path("baked.obj"),
            ["UVMap"],
            expected,
            True,
        ),
        check(
            "glTF TEXCOORD_1",
            lambda p: bpy.ops.import_scene.gltf(filepath=p),
            path("garment.glb"),
            ["UVMap.001", "UVMap_1", "TEXCOORD_1"],
            expected,
            False,
        ),
        check(
            "FBX UVLayout",
            lambda p: bpy.ops.import_scene.fbx(filepath=p),
            path("garment.fbx"),
            ["UVLayout"],
            expected,
            True,
        ),
        check(
            "USD uvLayout",
            lambda p: bpy.ops.wm.usd_import(filepath=p),
            path("garment.usda"),
            ["uvLayout"],
            expected,
            True,
        ),
        check(
            "Alembic UVLayout",
            lambda p: bpy.ops.wm.alembic_import(filepath=p, as_background_job=False),
            path("garment.abc"),
            ["UVLayout"],
            expected,
            False,
        ),
    ]
    # The baked atlas's material uses the image written beside the OBJ.
    clear()
    bpy.ops.wm.obj_import(filepath=path("baked.obj"))
    images = sorted(
        os.path.basename(n.image.filepath)
        for o in meshes()
        for m in o.data.materials
        if m and m.use_nodes
        for n in m.node_tree.nodes
        if n.type == "TEX_IMAGE" and n.image
    )
    print(f"OBJ baked atlas images: {images}")
    sys.exit(0 if all(results) else 1)


main()
