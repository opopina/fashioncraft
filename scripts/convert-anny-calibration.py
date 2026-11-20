"""Convert Anny's shape calibration (boys.pth, girls.pth) to assets/makehuman/anny-calibration.json.

Usage: python3 scripts/convert-anny-calibration.py <anny>/src/anny/data/shape_calibration

The two files are PyTorch state dicts from NAVER LABS Europe's Anny (Apache License 2.0; see
assets/makehuman/anny-calibration.provenance.json). They are read here without PyTorch: a
restricted unpickler that only rebuilds tensors from their raw little-endian storage.
Standard library only.
"""
import io
import json
import pickle
import struct
import sys
import zipfile
from collections import OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "assets/makehuman/anny-calibration.json"
STORAGES = {"DoubleStorage": ("d", 8), "FloatStorage": ("f", 4)}


def load(path: Path) -> dict:
    archive = zipfile.ZipFile(path)
    top = archive.namelist()[0].split("/")[0]

    def rebuild(storage, offset, size, *_):
        if len(size) != 1:
            raise ValueError("only vectors are expected")
        return storage[offset : offset + size[0]]

    class Reader(pickle.Unpickler):
        def find_class(self, module, name):
            if (module, name) == ("torch._utils", "_rebuild_tensor_v2"):
                return rebuild
            if module == "torch" and name in STORAGES:
                return name
            if (module, name) == ("collections", "OrderedDict"):
                return OrderedDict
            raise pickle.UnpicklingError(f"refused {module}.{name}")

        def persistent_load(self, pid):
            _, kind, key, _location, _count = pid
            code, size = STORAGES[kind]
            raw = archive.read(f"{top}/data/{key}")
            return list(struct.unpack("<" + code * (len(raw) // size), raw))

    state = Reader(io.BytesIO(archive.read(f"{top}/data.pkl"))).load()
    return {name: {k: list(v) for k, v in part.items()} for name, part in state.items()}


def main(folder: Path) -> None:
    out = {sex: load(folder / f"{sex}.pth") for sex in ("boys", "girls")}
    OUTPUT.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    print(f"{OUTPUT.relative_to(ROOT)}: {', '.join(out['boys'])}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    main(Path(sys.argv[1]))
