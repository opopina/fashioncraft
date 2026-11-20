#!/bin/sh
# Builds FashionCraft.app and its disk image for a release:
#   dist/FashionCraft.app
#   dist/fashioncraft-<version>-macos-<arch>.dmg   (arch: universal when both Mac targets are
#                                                    installed, otherwise this Mac's own)
#   dist/SHA256SUMS.txt
# Names follow the ArtCraft launcher's `<app>-<version>-<os>-<arch>.<ext>`.
#
# Signing: with FASHIONCRAFT_SIGN_IDENTITY set (a "Developer ID Application: …" identity in the
# keychain) the app is signed with it and the hardened runtime; otherwise it is signed ad hoc,
# which runs on the Mac that built it but not, without a warning, elsewhere.
#
# Needs Rust, Python 3 with Pillow, and the macOS tools sips, iconutil, codesign and hdiutil.
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
bundle_id=io.github.opopina.fashioncraft
dist=dist
app="$dist/FashionCraft.app"

# The program: universal when both Mac targets are installed.
targets=$(rustup target list --installed 2>/dev/null || true)
if echo "$targets" | grep -q aarch64-apple-darwin && echo "$targets" | grep -q x86_64-apple-darwin; then
    arch=universal
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --locked --release --features desktop --bin fashioncraft --target "$target"
    done
    binaries="target/aarch64-apple-darwin/release/fashioncraft target/x86_64-apple-darwin/release/fashioncraft"
else
    case $(uname -m) in arm64) arch=aarch64 ;; *) arch=x86_64 ;; esac
    cargo build --locked --release --features desktop --bin fashioncraft
    binaries=target/release/fashioncraft
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/assets"
if [ "$arch" = universal ]; then
    # shellcheck disable=SC2086
    lipo -create -output "$app/Contents/MacOS/FashionCraft" $binaries
else
    cp "$binaries" "$app/Contents/MacOS/FashionCraft"
fi

# The mannequin and the files that sit beside it, with their licence.
mkdir -p "$app/Contents/Resources/assets/makehuman"
for file in mannequin.obj arrangement.json measurements.json LICENSE.ASSETS.md provenance.json; do
    cp "assets/makehuman/$file" "$app/Contents/Resources/assets/makehuman/"
done
cp LICENSE THIRD_PARTY_NOTICES.md "$app/Contents/Resources/"

# The icon on Apple's grid (the tile drawn 824 of 1024 wide, clear round it), at every size.
iconset="$dist/FashionCraft.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
python3 - "$iconset" <<'PY'
import sys
from PIL import Image
out = sys.argv[1]
canvas = Image.open("assets/branding/fashioncraft-icon-macos.png").convert("RGBA")
for size in (16, 32, 128, 256, 512):
    canvas.resize((size, size), Image.LANCZOS).save(f"{out}/icon_{size}x{size}.png")
    canvas.resize((2 * size, 2 * size), Image.LANCZOS).save(f"{out}/icon_{size}x{size}@2x.png")
PY
iconutil -c icns "$iconset" -o "$app/Contents/Resources/FashionCraft.icns"
rm -rf "$iconset"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>FashionCraft</string>
    <key>CFBundleExecutable</key><string>FashionCraft</string>
    <key>CFBundleIconFile</key><string>FashionCraft</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>FashionCraft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>MIT License. Built on ClothCraft by erythrofr and contributors.</string>
</dict>
</plist>
PLIST

if [ -n "${FASHIONCRAFT_SIGN_IDENTITY:-}" ]; then
    codesign --force --options runtime --timestamp --sign "$FASHIONCRAFT_SIGN_IDENTITY" "$app"
else
    codesign --force --sign - "$app"
fi
codesign --verify --strict "$app"

# The disk image: the app beside a link to Applications.
dmg="$dist/fashioncraft-$version-macos-$arch.dmg"
stage="$dist/dmg"
rm -rf "$stage" "$dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -quiet -volname FashionCraft -srcfolder "$stage" -ov -format UDZO "$dmg"
rm -rf "$stage"

(cd "$dist" && shasum -a 256 "$(basename "$dmg")" > SHA256SUMS.txt)
echo "$app"
echo "$dmg"
cat "$dist/SHA256SUMS.txt"
