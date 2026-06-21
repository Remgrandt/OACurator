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

& (Join-Path $ResourcePath "vips.exe") --version
