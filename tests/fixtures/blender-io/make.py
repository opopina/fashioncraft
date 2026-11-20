import bpy, sys, math, json
out = sys.argv[sys.argv.index("--") + 1]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.mesh.primitive_uv_sphere_add(segments=12, ring_count=8, radius=0.3, location=(1, 2, 3), rotation=(math.radians(30), 0, math.radians(45)), scale=(1, 1, 2))
sphere = bpy.context.object
bpy.ops.object.empty_add(location=(0, 0, 1))
parent = bpy.context.object
parent.rotation_euler = (0, 0, math.radians(90))
bpy.ops.mesh.primitive_cube_add(size=0.2, location=(0.5, 0, 0))
cube = bpy.context.object
cube.parent = parent
bpy.context.view_layer.update()
# World-space corners of everything, turned Y-up (x, z, -y).
pts = []
for ob in (sphere, cube):
    for v in ob.data.vertices:
        w = ob.matrix_world @ v.co
        pts.append((w.x, w.z, -w.y))
lo = [min(p[i] for p in pts) for i in range(3)]
hi = [max(p[i] for p in pts) for i in range(3)]
tris = sum(len(p.vertices) - 2 for ob in (sphere, cube) for p in ob.data.polygons)
json.dump({"lo": lo, "hi": hi, "vertices": len(pts), "triangles": tris}, open(out + "/expected.json", "w"))
bpy.ops.wm.alembic_export(filepath=out + "/scene.abc")
bpy.ops.wm.usd_export(filepath=out + "/scene.usda")
bpy.ops.wm.usd_export(filepath=out + "/scene.usdz")
print("done")
