"""Pack MakeHuman's CC0 body targets into assets/makehuman/macro-targets.bin.

Usage: python3 scripts/pack-macro-targets.py <makehuman>/data/targets

Packed: the macro targets (`macrodetails`, named by their path below it), the bust size and
firmness targets (`breast/female-…`) and the measurement targets (`measure/…`).

The targets come from the MakeHuman community repository at the revision in
assets/makehuman/macro-targets.provenance.json. Only their data is used; no MakeHuman program
code. Standard library only; the output is deterministic.

Kept vertices: the body of base.obj (its first 13380 vertices, the mannequin's), then the
vertices of its joint helper groups and the other helper vertices that the avatar parts in
assets/avatars/parts (eyes, hair…) are fitted to, in index order. Little-endian, deflated as a whole with a
zlib wrapper:
  b"FCMH", u32 version = 1
  u32 K; K x u32 base-mesh index of each kept vertex; K x 3 x f32 base position (decimetres)
  u32 J; J x (u16 name length, UTF-8 name, u8 n, n x u16 kept index): joint helper groups
  u32 T; T x (u16 name length, UTF-8 name, u32 n,
              n x u16 gaps between successive kept indices (the first from 0),
              for x, y and z: n low bytes then n high bytes of the zigzagged differences
              between successive offsets, in units of 0.001 dm, the source files' own precision)
"""
import os
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "assets/makehuman/base.obj"
OUTPUT = ROOT / "assets/makehuman/macro-targets.bin"
BODY = 13380


def fit_references(path: Path) -> set:
    """The base-mesh vertices a MakeHuman .mhclo fitting refers to."""
    refs, inside = set(), False
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if not parts or parts[0].startswith("#"):
            continue
        try:
            values = [float(v) for v in parts]
        except ValueError:
            # A keyword: header lines may come among the vertices; other lists end them.
            if parts[0] == "verts":
                inside = True
            elif parts[0] in ("delete_verts", "weights", "vertex_groups"):
                inside = False
            elif parts[0] in ("x_scale", "y_scale", "z_scale"):
                refs |= {int(parts[1]), int(parts[2])}
            continue
        if inside:
            refs |= {int(v) for v in values[: 3 if len(values) >= 9 else 1]}
    return refs


def target_files(targets: Path):
    """(name, path) of every packed target, sorted by name."""
    out = []
    for folder, prefix in [("macrodetails", ""), ("breast", "breast/"), ("measure", "measure/")]:
        root = targets / folder
        for d, _, fs in os.walk(root):
            for f in fs:
                if not f.endswith(".target"):
                    continue
                path = Path(d) / f
                name = path.relative_to(root).with_suffix("").as_posix()
                # The bust's local targets (breast-dist-…) are not part of the body settings.
                if prefix == "breast/" and not name.startswith("female-"):
                    continue
                out.append((prefix + name, path))
    return sorted(out)


def main(targets: Path) -> None:
    vertices, groups, group = [], {}, None
    for line in BASE.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "v":
            vertices.append(tuple(float(v) for v in parts[1:4]))
        elif parts[0] == "g":
            group = parts[1]
        elif parts[0] == "f" and group and group.startswith("joint-"):
            for token in parts[1:]:
                groups.setdefault(group, set()).add(int(token.split("/")[0]) - 1)
    fitted = set()
    for fit in sorted((ROOT / "assets/avatars/parts").glob("*/*/fit.mhclo")):
        fitted |= {i for i in fit_references(fit) if i >= BODY}
    kept = list(range(BODY)) + sorted({i for g in groups.values() for i in g} | fitted)
    compact = {v: k for k, v in enumerate(kept)}
    blob = bytearray(b"FCMH" + struct.pack("<I", 1))
    blob += struct.pack(f"<I{len(kept)}I", len(kept), *kept)
    for i in kept:
        blob += struct.pack("<3f", *vertices[i])
    blob += struct.pack("<I", len(groups))
    for name in sorted(groups):
        index = sorted(compact[i] for i in groups[name])
        encoded = name.encode()
        blob += struct.pack("<H", len(encoded)) + encoded
        blob += struct.pack(f"<B{len(index)}H", len(index), *index)
    files = target_files(targets)
    blob += struct.pack("<I", len(files))
    offsets = 0
    for name, path in files:
        rows = []
        for line in path.read_text(encoding="utf-8").splitlines():
            parts = line.split()
            if not parts or parts[0].startswith("#") or int(parts[0]) not in compact:
                continue
            rows.append((compact[int(parts[0])], *(round(float(v) * 1000) for v in parts[1:4])))
        rows.sort()
        offsets += len(rows)
        encoded = name.encode()
        blob += struct.pack("<H", len(encoded)) + encoded + struct.pack("<I", len(rows))
        previous = 0
        for row in rows:
            gap = row[0] - previous
            if not 0 < gap < 65536 and not (gap == 0 and previous == 0):
                raise SystemExit(f"{name}: vertex {row[0]} out of order")
            blob += struct.pack("<H", gap)
            previous = row[0]
        for axis in range(3):
            last, low, high = 0, bytearray(), bytearray()
            for row in rows:
                difference = row[1 + axis] - last
                last = row[1 + axis]
                zigzag = (difference << 1) ^ (difference >> 63)
                if zigzag >= 65536:
                    raise SystemExit(f"{name}: offset step too large")
                low.append(zigzag & 0xFF)
                high.append(zigzag >> 8)
            blob += low + high
    packed = zlib.compress(bytes(blob), 9)
    OUTPUT.write_bytes(packed)
    print(
        f"{len(files)} targets, {offsets} offsets, {len(kept)} vertices, {len(groups)} joints: "
        f"{OUTPUT.relative_to(ROOT)} {len(packed) / 1e6:.2f} MB"
    )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    main(Path(sys.argv[1]))
