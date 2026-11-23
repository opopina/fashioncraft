"""Convert Anny's compact 104-bone rig and MakeHuman skin weights into assets/makehuman/anny-rig.bin.

Usage: python3 -I scripts/convert-anny-rig.py <folder-with-rig-and-weights-json>

Reads `rig.default.json` and `weights.default.json` from NAVER LABS Europe's Anny
(MakeHuman/MPFB2 CC0 rig data, revision pinned in assets/makehuman/anny-rig.provenance.json),
together with this repository's `assets/makehuman/base.obj`, `mannequin.obj` and
`measurements.json`.

Pruning follows Anny's compact `"anny"` rig (Apache License 2.0): removes tongue, breast,
facial-expression and zero-weight bones (59 of 163), reparents every retained bone to its
nearest retained ancestor, and merges removed bones' skin weights onto their nearest retained
ancestor, leaving 104 bones.

Verifies the body vertex order of `base.obj` against `mannequin.obj` before transferring
weights to the 13,380 body vertices, keeps up to 8 strongest influences per vertex
(renormalised to sum to 1), and packs everything deterministically with zlib level 9.

Standard library only.

Binary layout (little-endian, zlib-compressed as a whole):
  b"FCAR", u32 version = 1, u32 bone_count, u32 vertex_count, u8 max_influences
  bone_count x Bone:
    u16 name_len, UTF-8 name, i16 parent (-1 for root), Endpoint head, Endpoint tail
  where Endpoint is:
    u8 tag = 0 (Cube):
      u16 joint_len, UTF-8 joint_name
    u8 tag = 1 (Vertex):
      u32 v_idx,
      u16 lm_origin_len, UTF-8 lm_origin,
      u16 lm_a_len, UTF-8 lm_a,
      u16 lm_b_len, UTF-8 lm_b,
      u16 lm_up_len, UTF-8 lm_up,
      3 x f32 (ox, oy, oz)
    u8 tag = 2 (MeanBody):
      u32 v0, u32 v1,
      u16 lm_a_len, UTF-8 lm_a,
      u16 lm_b_len, UTF-8 lm_b,
      f32 t
    u8 tag = 3 (MeanBodyHelper):
      u32 v_body, 3 x u32 tri, 3 x f32 (u, v, h),
      u16 lm_a_len, UTF-8 lm_a,
      u16 lm_b_len, UTF-8 lm_b,
      f32 t
  vertex_count x VertexWeights:
    u8 count, count x (u8 bone_index, f32 weight)
"""

import json
import math
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_OBJ = ROOT / "assets/makehuman/base.obj"
MANNEQUIN_OBJ = ROOT / "assets/makehuman/mannequin.obj"
MEASUREMENTS_JSON = ROOT / "assets/makehuman/measurements.json"
OUTPUT = ROOT / "assets/makehuman/anny-rig.bin"

BODY_VERTICES = 13380
MAX_INFLUENCES = 8

# Bone groups removed by Anny's compact "anny" rig (notongue, nobreasts, nofacialexpression, pruned).
TONGUE_BONES = {
    "tongue00",
    "tongue01",
    "tongue02",
    "tongue03",
    "tongue04",
    "tongue05.L",
    "tongue05.R",
    "tongue06.L",
    "tongue06.R",
    "tongue07.L",
    "tongue07.R",
}
BREAST_BONES = {"breast.L", "breast.R"}
FACIAL_EXPRESSION_BONES = {
    "jaw",
    "special04",
    "oris02",
    "oris01",
    "oris06.L",
    "oris07.L",
    "oris06.R",
    "oris07.R",
    "levator02.L",
    "levator03.L",
    "levator04.L",
    "levator05.L",
    "levator02.R",
    "levator03.R",
    "levator04.R",
    "levator05.R",
    "special01",
    "oris04.L",
    "oris03.L",
    "oris04.R",
    "oris03.R",
    "oris06",
    "oris05",
    "special03",
    "levator06.L",
    "levator06.R",
    "special06.L",
    "special05.L",
    "orbicularis03.L",
    "orbicularis04.L",
    "special06.R",
    "special05.R",
    "orbicularis03.R",
    "orbicularis04.R",
    "temporalis01.L",
    "oculi02.L",
    "oculi01.L",
    "temporalis01.R",
    "oculi02.R",
    "oculi01.R",
    "temporalis02.L",
    "risorius02.L",
    "risorius03.L",
    "temporalis02.R",
    "risorius02.R",
    "risorius03.R",
}
ZERO_WEIGHT_BONES = {
    "oris02",
    "oris06.L",
    "oris06.R",
    "levator02.L",
    "levator03.L",
    "levator04.L",
    "levator02.R",
    "levator03.R",
    "levator04.R",
    "special01",
    "oris04.L",
    "oris04.R",
    "oris06",
    "special03",
    "special06.L",
    "special06.R",
    "temporalis01.L",
    "oculi02.L",
    "temporalis01.R",
    "oculi02.R",
    "temporalis02.L",
    "risorius02.L",
    "temporalis02.R",
    "risorius02.R",
}
REMOVED_BONES = TONGUE_BONES | BREAST_BONES | FACIAL_EXPRESSION_BONES | ZERO_WEIGHT_BONES

# Landmark pairs used as bones-only fallbacks for MEAN endpoints when no surface mesh is supplied.
MEAN_FALLBACK_LANDMARKS = {
    (10712, 11192): ("joint-l-upper-leg", "joint-l-knee"),
    (4056, 4574): ("joint-r-upper-leg", "joint-r-knee"),
    (11243, 16708): ("joint-l-knee", "joint-l-ankle"),
    (4625, 15404): ("joint-r-knee", "joint-r-ankle"),
    (11535, 12923): ("joint-l-ankle", "joint-l-foot-2"),
    (4917, 6326): ("joint-r-ankle", "joint-r-foot-2"),
    (8057, 17023): ("joint-l-shoulder", "joint-l-elbow"),
    (1365, 15711): ("joint-r-shoulder", "joint-r-elbow"),
    (8390, 9206): ("joint-l-elbow", "joint-l-hand"),
    (1718, 2538): ("joint-r-elbow", "joint-r-hand"),
    (659, 18889): ("joint-neck", "joint-head"),
    (226, 17824): ("joint-neck", "joint-head"),
}


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def mul(a, s):
    return (a[0] * s, a[1] * s, a[2] * s)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def norm(a):
    return math.sqrt(dot(a, a))


def unit(a):
    l = norm(a)
    if l < 1e-18:
        raise ValueError("zero-length vector")
    return mul(a, 1.0 / l)


def parse_base_obj():
    verts = []
    groups = {}
    body_tris = []
    group = None
    for line in BASE_OBJ.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "v":
            verts.append((float(parts[1]), float(parts[2]), float(parts[3])))
        elif parts[0] == "g":
            group = parts[1]
        elif parts[0] == "f" and group:
            face = [int(tok.split("/")[0]) - 1 for tok in parts[1:]]
            groups.setdefault(group, set()).update(face)
            if group == "body":
                for k in range(1, len(face) - 1):
                    body_tris.append((face[0], face[k], face[k + 1]))
    return verts, groups, body_tris


def verify_mannequin_vertex_order(base_verts):
    """Check that mannequin.obj matches base.obj[0..13380] vertex-for-vertex."""
    meas = json.loads(MEASUREMENTS_JSON.read_text(encoding="utf-8"))
    scale = meas["scale"]
    offset = meas["offset_source"]
    mannequin_verts = []
    for line in MANNEQUIN_OBJ.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if parts and parts[0] == "v":
            mannequin_verts.append((float(parts[1]), float(parts[2]), float(parts[3])))
    if len(mannequin_verts) != BODY_VERTICES:
        raise SystemExit(
            f"mannequin.obj has {len(mannequin_verts)} vertices, expected {BODY_VERTICES}"
        )
    max_err = 0.0
    for i in range(BODY_VERTICES):
        expected = tuple((base_verts[i][k] - offset[k]) * scale for k in range(3))
        err = norm(sub(expected, mannequin_verts[i]))
        if err > max_err:
            max_err = err
    if max_err > 1e-8:
        raise SystemExit(
            f"Vertex order mismatch between base.obj and mannequin.obj: max error {max_err} m"
        )
    return max_err, set(meas["landmarks"].keys())


def bind_helper_to_body_triangle(p, base_verts, body_tris):
    """Represent a non-body helper vertex in the local frame of a nearby body triangle."""
    best = None
    for ia, ib, ic in body_tris:
        a, b, c = base_verts[ia], base_verts[ib], base_verts[ic]
        e1, e2 = sub(b, a), sub(c, a)
        n = cross(e1, e2)
        area2 = norm(n)
        if area2 < 1e-12:
            continue
        n_hat = mul(n, 1.0 / area2)
        d = sub(p, a)
        g11, g12, g22 = dot(e1, e1), dot(e1, e2), dot(e2, e2)
        det = g11 * g22 - g12 * g12
        if det < 1e-18:
            continue
        r1, r2 = dot(d, e1), dot(d, e2)
        u = (r1 * g22 - r2 * g12) / det
        v = (g11 * r2 - g12 * r1) / det
        # Keep barycentric coordinates well-conditioned so extreme morphs do not extrapolate.
        if -0.5 <= u <= 1.5 and -0.5 <= v <= 1.5 and u + v <= 1.5:
            proj = add(a, add(mul(e1, u), mul(e2, v)))
            dist = norm(sub(p, proj))
            scale_len = math.sqrt(area2)
            h = dot(d, n_hat) / scale_len
            if best is None or dist < best[0]:
                best = (dist, (ia, ib, ic), u, v, h)
    if best is None:
        raise SystemExit(f"Could not bind helper vertex at {p} to any body triangle")
    _, tri, u, v, h = best
    # Sanity check exact reconstruction on base.obj.
    a, b, c = base_verts[tri[0]], base_verts[tri[1]], base_verts[tri[2]]
    e1, e2 = sub(b, a), sub(c, a)
    n = cross(e1, e2)
    area2 = norm(n)
    rec = add(add(a, add(mul(e1, u), mul(e2, v))), mul(n, h * math.sqrt(area2) / area2))
    if norm(sub(rec, p)) > 1e-10:
        raise SystemExit(f"Triangle frame reconstruction failed: err={norm(sub(rec, p))}")
    return tri, (u, v, h)


def pack_str(blob: bytearray, text: str) -> None:
    encoded = text.encode("utf-8")
    blob += struct.pack("<H", len(encoded)) + encoded


def pack_endpoint(blob: bytearray, spec: dict, base_verts, groups, body_tris, landmark_names):
    def cube_center(name: str):
        idxs = groups[name]
        n = len(idxs)
        return tuple(sum(base_verts[i][k] for i in idxs) / n for k in range(3))

    strategy = spec["strategy"]
    if strategy == "CUBE":
        name = spec["cube_name"]
        if name not in landmark_names:
            raise SystemExit(f"Missing landmark {name} in measurements.json")
        blob += struct.pack("<B", 0)
        pack_str(blob, name)
    elif strategy == "VERTEX":
        vi = int(spec["vertex_index"])
        if vi >= BODY_VERTICES:
            raise SystemExit(f"VERTEX endpoint {vi} is outside body vertices")
        # Bones-only fallback: express p in the pelvis landmark frame.
        lm_origin = "joint-pelvis"
        lm_a = "joint-l-upper-leg"
        lm_b = "joint-r-upper-leg"
        lm_up = "joint-spine-4"
        o = cube_center(lm_origin)
        pa, pb, pu = cube_center(lm_a), cube_center(lm_b), cube_center(lm_up)
        width = norm(sub(pa, pb))
        x_axis = unit(sub(pa, pb))
        y_raw = sub(pu, o)
        y_axis = unit(sub(y_raw, mul(x_axis, dot(y_raw, x_axis))))
        z_axis = cross(x_axis, y_axis)
        rel = sub(base_verts[vi], o)
        ox = dot(rel, x_axis) / width
        oy = dot(rel, y_axis) / width
        oz = dot(rel, z_axis) / width
        blob += struct.pack("<BI", 1, vi)
        for nm in (lm_origin, lm_a, lm_b, lm_up):
            pack_str(blob, nm)
        blob += struct.pack("<3f", ox, oy, oz)
    elif strategy == "MEAN":
        idxs = [int(i) for i in spec["vertex_indices"]]
        if len(idxs) != 2:
            raise SystemExit(f"Expected 2 vertices in MEAN endpoint, got {idxs}")
        key = tuple(idxs)
        if key not in MEAN_FALLBACK_LANDMARKS:
            raise SystemExit(f"No fallback landmark pair configured for MEAN {key}")
        lm_a, lm_b = MEAN_FALLBACK_LANDMARKS[key]
        pa, pb = cube_center(lm_a), cube_center(lm_b)
        mid = mul(add(base_verts[idxs[0]], base_verts[idxs[1]]), 0.5)
        ab = sub(pb, pa)
        t = dot(sub(mid, pa), ab) / dot(ab, ab)
        t = max(0.05, min(0.95, t))
        if idxs[0] < BODY_VERTICES and idxs[1] < BODY_VERTICES:
            blob += struct.pack("<BII", 2, idxs[0], idxs[1])
            pack_str(blob, lm_a)
            pack_str(blob, lm_b)
            blob += struct.pack("<f", t)
        else:
            body_i = [i for i in idxs if i < BODY_VERTICES]
            helper_i = [i for i in idxs if i >= BODY_VERTICES]
            if len(body_i) != 1 or len(helper_i) != 1:
                raise SystemExit(f"Unexpected MEAN mix: {idxs}")
            tri, (u, v, h) = bind_helper_to_body_triangle(
                base_verts[helper_i[0]], base_verts, body_tris
            )
            blob += struct.pack("<BI3I3f", 3, body_i[0], tri[0], tri[1], tri[2], u, v, h)
            pack_str(blob, lm_a)
            pack_str(blob, lm_b)
            blob += struct.pack("<f", t)
    else:
        raise SystemExit(f"Unsupported endpoint strategy: {strategy}")


def main(folder: Path) -> None:
    rig_raw = json.loads((folder / "rig.default.json").read_text(encoding="utf-8"))
    if "bones" in rig_raw:
        rig_raw = rig_raw["bones"]
    weights_raw = json.loads((folder / "weights.default.json").read_text(encoding="utf-8"))[
        "weights"
    ]

    base_verts, groups, body_tris = parse_base_obj()
    max_order_err, landmark_names = verify_mannequin_vertex_order(base_verts)

    # Depth-first traversal from root (parents before children), matching Anny's load_rig.
    roots = [name for name, data in rig_raw.items() if not data.get("parent")]
    if len(roots) != 1:
        raise SystemExit(f"Expected 1 root bone, found {roots}")
    order = []
    parents = []

    def dfs(name: str, parent_idx: int) -> None:
        idx = len(order)
        order.append(name)
        parents.append(parent_idx)
        for child, data in rig_raw.items():
            if child not in order and data.get("parent") == name:
                dfs(child, idx)

    dfs(roots[0], -1)
    if len(order) != len(rig_raw):
        raise SystemExit(f"Rig traversal visited {len(order)} of {len(rig_raw)} bones")

    retained_old = [i for i, name in enumerate(order) if name not in REMOVED_BONES]
    retained_set = set(retained_old)
    old_to_new = {old: new for new, old in enumerate(retained_old)}

    def nearest_retained_ancestor(old_idx: int) -> int:
        cur = parents[old_idx]
        while cur >= 0:
            if cur in retained_set:
                return old_to_new[cur]
            cur = parents[cur]
        return -1

    def retained_target(old_idx: int) -> int:
        cur = old_idx
        while cur >= 0:
            if cur in retained_set:
                return old_to_new[cur]
            cur = parents[cur]
        return 0

    retained_names = [order[i] for i in retained_old]
    retained_parents = [nearest_retained_ancestor(i) for i in retained_old]
    if sum(1 for p in retained_parents if p < 0) != 1:
        raise SystemExit("Pruned rig must have exactly one root bone")

    # Aggregate weights from all 163 bones onto the 104 retained bones for body vertices 0..13380.
    vert_weights = [{} for _ in range(BODY_VERTICES)]
    for old_idx, name in enumerate(order):
        tgt = retained_target(old_idx)
        for vi, w in weights_raw.get(name, []):
            if vi < BODY_VERTICES and w > 0.0:
                vert_weights[vi][tgt] = vert_weights[vi].get(tgt, 0.0) + float(w)

    # Truncate to MAX_INFLUENCES strongest weights and normalise every vertex to sum to 1.
    max_dropped = 0.0
    total_pairs = 0
    packed_weights = []
    for vi, table in enumerate(vert_weights):
        if not table:
            raise SystemExit(f"Body vertex {vi} has zero total skin weight")
        total = sum(table.values())
        items = sorted(((b, w / total) for b, w in table.items()), key=lambda x: (-x[1], x[0]))
        if len(items) > MAX_INFLUENCES:
            dropped = sum(w for _, w in items[MAX_INFLUENCES:])
            if dropped > max_dropped:
                max_dropped = dropped
            items = items[:MAX_INFLUENCES]
        kept_sum = sum(w for _, w in items)
        items = [(b, w / kept_sum) for b, w in items]
        total_pairs += len(items)
        packed_weights.append(items)

    blob = bytearray(b"FCAR")
    blob += struct.pack("<IIIB", 1, len(retained_names), BODY_VERTICES, MAX_INFLUENCES)
    for name, parent in zip(retained_names, retained_parents):
        pack_str(blob, name)
        blob += struct.pack("<h", parent)
        pack_endpoint(blob, rig_raw[name]["head"], base_verts, groups, body_tris, landmark_names)
        pack_endpoint(blob, rig_raw[name]["tail"], base_verts, groups, body_tris, landmark_names)

    for items in packed_weights:
        blob += struct.pack("<B", len(items))
        for b, w in items:
            blob += struct.pack("<Bf", b, w)

    compressed = zlib.compress(bytes(blob), 9)
    OUTPUT.write_bytes(compressed)
    print(
        f"{len(retained_names)} bones, {BODY_VERTICES} vertices, {total_pairs} weight pairs "
        f"(max_influences={MAX_INFLUENCES}, max_dropped={max_dropped:.6f}, "
        f"vertex_order_err={max_order_err:.2e} m): "
        f"{OUTPUT.relative_to(ROOT)} {len(compressed) / 1024:.1f} KB"
    )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    main(Path(sys.argv[1]))
