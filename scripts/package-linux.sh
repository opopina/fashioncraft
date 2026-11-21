#!/bin/sh
# Builds FashionCraft's Linux release files for this machine's architecture:
#   dist/fashioncraft-<version>-linux-<arch>.tar.gz    bin/fashioncraft and share/ (assets, menu
#                                                       entry, icons), to unpack into a prefix
#   dist/fashioncraft-<version>-linux-<arch>.AppImage  when APPIMAGETOOL names appimagetool
# Names follow the ArtCraft launcher's `<app>-<version>-<os>-<arch>.<ext>`. The program finds its
# assets in ../share/fashioncraft/assets beside bin/.
#
# Needs Rust and Python 3 with Pillow.
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app_id=io.github.opopina.fashioncraft
arch=$(uname -m)
case $arch in arm64) arch=aarch64 ;; esac
dist=dist
root="$dist/linux/fashioncraft-$version"

cargo build --locked --release --features desktop --bin fashioncraft

rm -rf "$dist/linux"
mkdir -p "$root/bin" "$root/share/fashioncraft/assets/makehuman" "$root/share/applications" \
    "$root/share/doc/fashioncraft"
cp target/release/fashioncraft "$root/bin/"
strip "$root/bin/fashioncraft" 2>/dev/null || true
for file in mannequin.obj arrangement.json measurements.json LICENSE.ASSETS.md provenance.json; do
    cp "assets/makehuman/$file" "$root/share/fashioncraft/assets/makehuman/"
done
cp LICENSE THIRD_PARTY_NOTICES.md "$root/share/doc/fashioncraft/"

cat > "$root/share/applications/$app_id.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=FashionCraft
GenericName=Garment design
Comment=3D garment design and cloth simulation
Exec=fashioncraft %f
Icon=$app_id
Terminal=false
Categories=Graphics;3DGraphics;
StartupWMClass=io.github.opopina.fashioncraft
DESKTOP

python3 - "$root/share/icons/hicolor" "$app_id" <<'PY'
import os, sys
from PIL import Image
out, app_id = sys.argv[1], sys.argv[2]
icon = Image.open("assets/branding/fashioncraft-icon.png").convert("RGBA")
for size in (16, 24, 32, 48, 64, 128, 256, 512):
    folder = os.path.join(out, f"{size}x{size}", "apps")
    os.makedirs(folder, exist_ok=True)
    icon.resize((size, size), Image.LANCZOS).save(os.path.join(folder, f"{app_id}.png"))
PY

tarball="$dist/fashioncraft-$version-linux-$arch.tar.gz"
tar -C "$root" -czf "$tarball" bin share
echo "$tarball"

if [ -n "${APPIMAGETOOL:-}" ]; then
    appdir="$dist/linux/FashionCraft.AppDir"
    mkdir -p "$appdir/usr"
    cp -R "$root/bin" "$root/share" "$appdir/usr/"
    cp "$root/share/applications/$app_id.desktop" "$appdir/"
    cp "$root/share/icons/hicolor/256x256/apps/$app_id.png" "$appdir/$app_id.png"
    cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/fashioncraft" "$@"
APPRUN
    chmod +x "$appdir/AppRun"
    appimage="$dist/fashioncraft-$version-linux-$arch.AppImage"
    ARCH=$arch "$APPIMAGETOOL" --no-appstream "$appdir" "$appimage"
    echo "$appimage"
fi
