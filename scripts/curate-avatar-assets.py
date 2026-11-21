"""Copy the avatar looks FashionCraft ships from MakeHuman's CC0 asset packs into assets/avatars.

Usage: python3 scripts/curate-avatar-assets.py <unzipped packs folder>

The folder holds the unzipped packs from https://static.makehumancommunity.org/assets/assetpacks/:
makehuman_system_assets_cc0/, skins01_cc0/ and skins02_cc0/. Every asset copied here is listed
as CC0 in its pack's manifest (packs/*.json); its record is kept in assets/avatars/provenance.json.
Only images and fitting data are copied: skins become JPEG, hair, eyebrow and eyelash images
keep their transparency at 1024 pixels, eyes are 512 pixels. MakeHuman's .mhmat material files
are not used. Needs Pillow.
"""
import json
import shutil
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "assets/avatars"

SKINS = [
    "young_caucasian_female",
    "young_caucasian_male",
    "young_african_female",
    "young_african_male",
    "young_asian_female",
    "young_asian_male",
    "middleage_caucasian_female",
    "middleage_caucasian_male",
    "middleage_african_male",
    "middleage_asian_female",
]
# (kind, name): fitted parts, each an .mhclo with its mesh and image.
PARTS = [
    ("eyebrows", "eyebrow001"),
    ("eyebrows", "eyebrow010"),
    ("eyelashes", "eyelashes01"),
    ("eyelashes", "eyelashes02"),
    ("hair", "short02"),
    ("hair", "short04"),
    ("hair", "ponytail01"),
    ("hair", "bob01"),
    ("hair", "bob02"),
    ("hair", "long01"),
    ("hair", "braid01"),
    ("hair", "afro01"),
]
EYE_COLOURS = ["brown", "brownlight", "blue", "green", "grey"]


def save_image(source: Path, target: Path, side: int, keep_alpha: bool) -> None:
    image = Image.open(source)
    image = image.convert("RGBA" if keep_alpha else "RGB")
    if image.size[0] > side:
        image = image.resize((side, side), Image.LANCZOS)
    target.parent.mkdir(parents=True, exist_ok=True)
    if keep_alpha:
        image.save(target, optimize=True)
    else:
        image.save(target, quality=90, optimize=True)


def main(packs: Path) -> None:
    system = packs / "makehuman_system_assets_cc0"
    manifest = json.loads((system / "packs/makehuman_system_assets.json").read_text())
    records = {}
    if OUT.exists():
        for sub in ("skins", "parts", "eyes"):
            shutil.rmtree(OUT / sub, ignore_errors=True)
    for name in SKINS:
        folder = system / "skins" / name
        diffuse = next(p for p in folder.glob("*.png") if "normal" not in p.name)
        save_image(diffuse, OUT / "skins" / f"{name}.jpg", 2048, keep_alpha=False)
        records[f"skins/{name}"] = manifest[name]
    for kind, name in PARTS:
        folder = system / kind / name
        target = OUT / "parts" / kind / name
        target.mkdir(parents=True, exist_ok=True)
        shutil.copy(folder / f"{name}.mhclo", target / "fit.mhclo")
        shutil.copy(folder / f"{name}.obj", target / "mesh.obj")
        image = next(
            p for p in sorted(folder.glob("*.png")) if "normal" not in p.name and "_ao" not in p.name
        )
        save_image(image, target / "image.png", 1024, keep_alpha=True)
        records[f"parts/{kind}/{name}"] = manifest[name]
    eyes = system / "eyes"
    target = OUT / "parts/eyes/low-poly"
    target.mkdir(parents=True, exist_ok=True)
    shutil.copy(eyes / "low-poly/low-poly.mhclo", target / "fit.mhclo")
    shutil.copy(eyes / "low-poly/low-poly.obj", target / "mesh.obj")
    records["parts/eyes/low-poly"] = manifest["low-poly"]
    for colour in EYE_COLOURS:
        save_image(eyes / "materials" / f"{colour}_eye.png", OUT / "eyes" / f"{colour}.png", 512, keep_alpha=False)
        records[f"eyes/{colour}"] = manifest[colour]
    licences = {r["license"] for r in records.values()}
    if licences != {"CC0"}:
        raise SystemExit(f"not all CC0: {licences}")
    provenance = {
        "license": "CC0-1.0",
        "source": "https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html",
        "pack": "makehuman_system_assets_cc0.zip",
        "downloaded": "2026-11-20",
        "copied_by": "scripts/curate-avatar-assets.py",
        "assets": records,
    }
    (OUT / "provenance.json").write_text(json.dumps(provenance, indent=1, sort_keys=True) + "\n")
    size = sum(p.stat().st_size for p in OUT.rglob("*") if p.is_file())
    print(f"{len(records)} assets, {size / 1e6:.1f} MB in {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    main(Path(sys.argv[1]))
