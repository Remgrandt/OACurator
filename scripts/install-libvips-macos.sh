#!/usr/bin/env bash
# Copyright (c) 2026 Remgrandt Works. All rights reserved.

set -euo pipefail

resource_path="${1:-src-tauri/resources/libvips}"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
lock_file="${OACURATOR_LIBVIPS_LOCK_FILE:-$script_dir/libvips-runtime-lock.json}"
sharp_libvips_version="${OACURATOR_SHARP_LIBVIPS_VERSION:-1.2.4}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This installer is for macOS libvips builds only." >&2
  exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "npm is required to download the macOS libvips runtime." >&2
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "node is required to read the libvips runtime lock file." >&2
  exit 1
fi

if [[ ! -f "$lock_file" ]]; then
  echo "Missing libvips runtime lock file: $lock_file" >&2
  exit 1
fi

arch="$(uname -m)"
case "$arch" in
  arm64)
    sharp_libvips_package="@img/sharp-libvips-darwin-arm64"
    ;;
  x86_64)
    sharp_libvips_package="@img/sharp-libvips-darwin-x64"
    ;;
  *)
    echo "Unsupported macOS architecture for libvips runtime: $(uname -m)" >&2
    exit 1
    ;;
esac

expected_sha256="$(
  node -e '
const fs = require("fs");
const [lockPath, arch, requestedVersion, requestedPackage] = process.argv.slice(1);
const lock = JSON.parse(fs.readFileSync(lockPath, "utf8"));
if (lock.notice !== "Copyright (c) 2026 Remgrandt Works. All rights reserved.") {
  console.error("libvips runtime lock file is missing the expected copyright notice.");
  process.exit(1);
}
const expectedVersion = lock?.macos?.sharpLibvipsVersion;
if (expectedVersion !== requestedVersion) {
  console.error(`Requested sharp-libvips version ${requestedVersion} is not pinned by ${lockPath}.`);
  process.exit(1);
}
const entry = lock?.macos?.packages?.[arch];
if (!entry) {
  console.error(`No pinned macOS libvips runtime is configured for ${arch}.`);
  process.exit(1);
}
if (entry.name !== requestedPackage) {
  console.error(`Pinned macOS libvips package ${entry.name} does not match requested package ${requestedPackage}.`);
  process.exit(1);
}
if (!/^[a-f0-9]{64}$/.test(entry.sha256)) {
  console.error(`Pinned macOS libvips SHA-256 is malformed for ${arch}.`);
  process.exit(1);
}
process.stdout.write(entry.sha256);
' "$lock_file" "$arch" "$sharp_libvips_version" "$sharp_libvips_package"
)"

mkdir -p "$resource_path"
find "$resource_path" -mindepth 1 -maxdepth 1 ! -name ".gitkeep" -exec rm -rf {} +
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

tarball_name="$(
  npm pack "${sharp_libvips_package}@${sharp_libvips_version}" \
    --pack-destination "$work_dir" \
    --silent |
    tail -n 1
)"
tarball_path="$work_dir/$tarball_name"
actual_sha256="$(shasum -a 256 "$tarball_path" | awk '{print tolower($1)}')"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "libvips runtime SHA-256 mismatch for $tarball_name. Expected $expected_sha256, got $actual_sha256." >&2
  exit 1
fi
echo "Verified ${sharp_libvips_package}@${sharp_libvips_version} SHA-256 ${actual_sha256}."

extract_dir="$work_dir/extract"
mkdir -p "$extract_dir"
tar -xzf "$tarball_path" -C "$extract_dir"

vips_cpp_source="$(
  find "$extract_dir/package/lib" -maxdepth 1 -type f -name 'libvips-cpp.*.dylib' |
    sort |
    head -n 1
)"

if [[ -z "$vips_cpp_source" ]]; then
  echo "Could not find libvips-cpp dylib in ${sharp_libvips_package}@${sharp_libvips_version}." >&2
  exit 1
fi

cp "$vips_cpp_source" "$resource_path/$(basename "$vips_cpp_source")"
cp "$vips_cpp_source" "$resource_path/libvips-cpp.42.dylib"
cp "$vips_cpp_source" "$resource_path/libvips-cpp.dylib"
cp "$extract_dir/package/README.md" "$resource_path/THIRD-PARTY-NOTICES-sharp-libvips.md"
cp "$extract_dir/package/versions.json" "$resource_path/sharp-libvips-versions.json"

dedupe_rpaths() {
  local mach_o="$1"
  local rpath
  local repeats

  repeats="$(otool -l "$mach_o" |
    awk '/cmd LC_RPATH/{in_rpath=1; next} in_rpath && /path /{print $2; in_rpath=0}' |
    sort |
    uniq -d)"

  if [[ -z "$repeats" ]]; then
    return
  fi

  chmod +w "$mach_o"
  while IFS= read -r rpath; do
    while install_name_tool -delete_rpath "$rpath" "$mach_o" 2>/dev/null; do
      :
    done
    install_name_tool -add_rpath "$rpath" "$mach_o"
  done <<<"$repeats"
}

while IFS= read -r -d '' mach_o; do
  if file "$mach_o" | grep -Eq 'Mach-O'; then
    dedupe_rpaths "$mach_o"
    codesign --force --sign - "$mach_o"
  fi
done < <(find "$resource_path" -type f -print0)

echo "Bundled ${sharp_libvips_package}@${sharp_libvips_version} as macOS libvips runtime."
