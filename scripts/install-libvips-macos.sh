#!/usr/bin/env bash
# Copyright (c) 2026 Remgrandt Works. All rights reserved.

set -euo pipefail

resource_path="${1:-src-tauri/resources/libvips}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This installer is for macOS libvips builds only." >&2
  exit 1
fi

if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required to bundle libvips on macOS." >&2
  exit 1
fi

brew install vips dylibbundler

mkdir -p "$resource_path"
find "$resource_path" -mindepth 1 -maxdepth 1 ! -name ".gitkeep" -exec rm -rf {} +
lib_stage="$(mktemp -d)"
trap 'rm -rf "$lib_stage"' EXIT

vips_prefix="$(brew --prefix vips)"
vips_bin="$vips_prefix/bin/vips"
vipsheader_bin="$vips_prefix/bin/vipsheader"

if [[ ! -x "$vips_bin" || ! -x "$vipsheader_bin" ]]; then
  echo "Could not find Homebrew vips command-line tools under $vips_prefix." >&2
  exit 1
fi

cp "$vips_bin" "$resource_path/vips"
cp "$vipsheader_bin" "$resource_path/vipsheader"
chmod 755 "$resource_path/vips" "$resource_path/vipsheader"

while IFS= read -r -d '' module_dir; do
  cp -R "$module_dir" "$resource_path/$(basename "$module_dir")"
done < <(find "$vips_prefix/lib" -maxdepth 1 -type d -name 'vips-modules-*' -print0)

dylib_inputs=("$resource_path/vips" "$resource_path/vipsheader")
while IFS= read -r -d '' mach_o; do
  if file "$mach_o" | grep -Eq 'Mach-O.*(dynamically linked shared library|bundle)'; then
    dylib_inputs+=("$mach_o")
  fi
done < <(find "$resource_path" -type f -print0)

dylib_args=()
for input in "${dylib_inputs[@]}"; do
  dylib_args+=("-x" "$input")
done

dylibbundler -od -b "${dylib_args[@]}" -d "$lib_stage" -p "@executable_path"
cp -R "$lib_stage"/. "$resource_path"/

ensure_link_alias() {
  local existing="$1"
  local alias="$2"

  if [[ ! -e "$existing" ]]; then
    echo "Missing libvips link input: $existing" >&2
    exit 1
  fi
  ln -sf "$(basename "$existing")" "$alias"
}

ensure_link_alias "$resource_path/libvips.42.dylib" "$resource_path/libvips.dylib"
ensure_link_alias "$resource_path/libglib-2.0.0.dylib" "$resource_path/libglib-2.0.dylib"
ensure_link_alias "$resource_path/libgobject-2.0.0.dylib" "$resource_path/libgobject-2.0.dylib"

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

"$resource_path/vips" --version
