# Copyright (c) 2026 Remgrandt Works. All rights reserved.

[CmdletBinding()]
param(
  [string] $Version = "8.18.3",
  [string] $ResourcePath = "src-tauri\resources\libvips",
  [string] $LockFile = ""
)

$ErrorActionPreference = "Stop"

if (-not $IsWindows -and $env:OS -ne "Windows_NT") {
  throw "This installer is for Windows libvips builds only."
}

$scriptRoot = Split-Path -Parent $PSCommandPath
if ($env:OACURATOR_LIBVIPS_LOCK_FILE) {
  $lockPath = $env:OACURATOR_LIBVIPS_LOCK_FILE
} elseif ($LockFile) {
  $lockPath = $LockFile
} else {
  $lockPath = Join-Path $scriptRoot "libvips-runtime-lock.json"
}

if (-not [System.IO.Path]::IsPathRooted($lockPath)) {
  $lockPath = Join-Path (Get-Location) $lockPath
}

if (-not (Test-Path -LiteralPath $lockPath)) {
  throw "Missing libvips runtime lock file: $lockPath."
}

$lock = Get-Content -Raw -LiteralPath $lockPath | ConvertFrom-Json
if ($lock.notice -ne "Copyright (c) 2026 Remgrandt Works. All rights reserved.") {
  throw "libvips runtime lock file is missing the expected copyright notice."
}

$assetName = "vips-dev-x64-web-$Version-static-ffi.zip"
$url = "https://github.com/libvips/build-win64-mxe/releases/download/v$Version/$assetName"
$expected = $lock.windows
if ($expected.libvipsVersion -ne $Version) {
  throw "Requested libvips version $Version is not pinned by $lockPath."
}
if ($expected.assetName -ne $assetName) {
  throw "Pinned Windows libvips asset does not match requested asset $assetName."
}
if ($expected.url -ne $url) {
  throw "Pinned Windows libvips URL does not match requested URL $url."
}
$expectedHash = ([string] $expected.sha256).ToLowerInvariant()
if ($expectedHash -notmatch "^[a-f0-9]{64}$") {
  throw "Pinned Windows libvips SHA-256 is malformed."
}

$workRoot = Join-Path ([System.IO.Path]::GetTempPath()) "oac-libvips-$Version"
$zipPath = Join-Path $workRoot $assetName
$extractPath = Join-Path $workRoot "extract"

New-Item -ItemType Directory -Force $workRoot | Out-Null
Remove-Item -LiteralPath $extractPath -Recurse -Force -ErrorAction SilentlyContinue

if (-not (Test-Path -LiteralPath $zipPath)) {
  Invoke-WebRequest -Uri $url -OutFile $zipPath
}

$actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
  throw "libvips runtime SHA-256 mismatch for $assetName. Expected $expectedHash, got $actualHash."
}
Write-Host "Verified $assetName SHA-256 $actualHash."

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
  Remove-Item -Recurse -Force
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
