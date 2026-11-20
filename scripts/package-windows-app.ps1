# Builds FashionCraft's portable Windows release:
#   dist\fashioncraft-<version>-windows-x64-portable.zip   fashioncraft.exe, assets\ and the licences
#   dist\fashioncraft.ico                                   the icon, 16 to 256 px, for an installer
# The name follows the ArtCraft launcher's `<app>-<version>-<os>-<arch>[-portable].<ext>`. The
# program finds its assets in assets\ beside it and keeps its work in %APPDATA%\FashionCraft.
#
# Needs Rust and Python 3 with Pillow. (scripts\package-windows.ps1 is the separate ClothCraft
# developer preview of cloth-fit and its tools.)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$dist = 'dist'
$stage = Join-Path $dist 'windows\FashionCraft'

cargo build --locked --release --features desktop --bin fashioncraft
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

if (Test-Path (Join-Path $dist 'windows')) { Remove-Item -Recurse -Force (Join-Path $dist 'windows') }
New-Item -ItemType Directory -Force -Path (Join-Path $stage 'assets\makehuman') | Out-Null
Copy-Item target\release\fashioncraft.exe $stage
foreach ($file in 'mannequin.obj', 'arrangement.json', 'measurements.json', 'LICENSE.ASSETS.md', 'provenance.json') {
    Copy-Item (Join-Path 'assets\makehuman' $file) (Join-Path $stage 'assets\makehuman')
}
Copy-Item LICENSE, THIRD_PARTY_NOTICES.md $stage

python -c "from PIL import Image; Image.open('assets/branding/fashioncraft-icon.png').save('$dist/fashioncraft.ico', sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])"
if ($LASTEXITCODE -ne 0) { throw 'icon conversion failed' }

$zip = Join-Path $dist "fashioncraft-$version-windows-x64-portable.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
Write-Output $zip
