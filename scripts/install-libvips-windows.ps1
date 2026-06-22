# Copyright (c) 2026 Remgrandt Works. All rights reserved.

[CmdletBinding()]
param(
  [string] $Version = "8.18.3",
  [string] $ResourcePath = "src-tauri\resources\libvips"
)

$ErrorActionPreference = "Stop"

if (-not $IsWindows -and $env:OS -ne "Windows_NT") {
  throw "This installer is for Windows libvips builds only."
}

$assetName = "vips-dev-x64-web-$Version-static-ffi.zip"
$url = "https://github.com/libvips/build-win64-mxe/releases/download/v$Version/$assetName"
$workRoot = Join-Path ([System.IO.Path]::GetTempPath()) "oac-libvips-$Version"
$zipPath = Join-Path $workRoot $assetName
$extractPath = Join-Path $workRoot "extract"

New-Item -ItemType Directory -Force $workRoot | Out-Null
Remove-Item -LiteralPath $extractPath -Recurse -Force -ErrorAction SilentlyContinue

if (-not (Test-Path -LiteralPath $zipPath)) {
  Invoke-WebRequest -Uri $url -OutFile $zipPath
}

Expand-Archive -LiteralPath $zipPath -DestinationPath $extractPath -Force
$vipsRoot = Get-ChildItem -LiteralPath $extractPath -Directory |
  Where-Object { $_.Name -like "vips-dev-*" } |
  Select-Object -First 1
if (-not $vipsRoot) {
  throw "Could not find extracted libvips root in $extractPath."
}

New-Item -ItemType Directory -Force $ResourcePath | Out-Null
Get-ChildItem -LiteralPath $ResourcePath -Force |
  Where-Object { $_.Name -ne ".gitkeep" } |
  Remove-Item -Force
Get-ChildItem -LiteralPath (Join-Path $vipsRoot.FullName "bin") -Filter *.exe |
  Copy-Item -Destination $ResourcePath
Get-ChildItem -LiteralPath (Join-Path $vipsRoot.FullName "bin") -Filter *.dll |
  Copy-Item -Destination $ResourcePath

$libResourcePath = Join-Path $ResourcePath "lib"
New-Item -ItemType Directory -Force $libResourcePath | Out-Null
Get-ChildItem -LiteralPath (Join-Path $vipsRoot.FullName "lib") -Filter *.lib |
  Copy-Item -Destination $libResourcePath

$importLibraryAliases = @{
  "libvips.lib" = "vips.lib"
  "libglib-2.0.lib" = "glib-2.0.lib"
  "libgobject-2.0.lib" = "gobject-2.0.lib"
}
foreach ($entry in $importLibraryAliases.GetEnumerator()) {
  $source = Join-Path $libResourcePath $entry.Key
  $destination = Join-Path $libResourcePath $entry.Value
  if (-not (Test-Path -LiteralPath $source)) {
    throw "Missing libvips import library $($entry.Key)."
  }
  Copy-Item -LiteralPath $source -Destination $destination -Force
}

& (Join-Path $ResourcePath "vips.exe") --version
