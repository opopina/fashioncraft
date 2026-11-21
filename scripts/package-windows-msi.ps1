# Builds FashionCraft's Windows installer:
#   dist\fashioncraft-<version>-windows-x64.msi   per user (no administrator), Start menu shortcut,
#                                                  uninstall from Settings > Apps
# The name follows the ArtCraft launcher's `<app>-<version>-<os>-<arch>.<ext>`.
#
# Run scripts\package-windows-app.ps1 first: it builds and stages the app and makes the icon.
# Needs the WiX Toolset v5:  dotnet tool install --global wix --version 5.0.2
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$stage = Join-Path $PWD 'dist\windows\FashionCraft'
$icon = Join-Path $PWD 'dist\fashioncraft.ico'
if (-not (Test-Path (Join-Path $stage 'fashioncraft.exe'))) { throw 'Run scripts\package-windows-app.ps1 first' }

$msi = Join-Path 'dist' "fashioncraft-$version-windows-x64.msi"
if (Test-Path $msi) { Remove-Item $msi }
wix build packaging\windows\fashioncraft.wxs -arch x64 -d "Version=$version" -d "Stage=$stage" -d "Icon=$icon" -o $msi
if ($LASTEXITCODE -ne 0) { throw 'wix build failed' }
Write-Output $msi
