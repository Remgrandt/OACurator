#!/usr/bin/env bash
# Copyright (c) 2026 Remgrandt Works. All rights reserved.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This merger is for macOS libvips runtimes only." >&2
  exit 1
fi

if [[ "$#" -ne 3 ]]; then
  echo "Usage: $0 <arm64-libvips-dir> <x64-libvips-dir> <output-libvips-dir>" >&2
  exit 1
fi

arm64_dir="$1"
x64_dir="$2"
output_dir="$3"

if [[ ! -d "$arm64_dir" ]]; then
  echo "Missing arm64 libvips directory: $arm64_dir" >&2
  exit 1
fi
if [[ ! -d "$x64_dir" ]]; then
  echo "Missing x64 libvips directory: $x64_dir" >&2
  exit 1
fi
if ! command -v lipo >/dev/null 2>&1; then
  echo "lipo is required to create the universal libvips runtime." >&2
  exit 1
fi

is_macho() {
  file "$1" | grep -Eq 'Mach-O'
}

rm -rf "$output_dir"
mkdir -p "$(dirname "$output_dir")"
ditto "$arm64_dir" "$output_dir"

relative_files="$(mktemp)"
trap 'rm -f "$relative_files"' EXIT

(
  cd "$arm64_dir"
  find . -type f -print
  cd "$x64_dir"
  find . -type f -print
) | sort -u > "$relative_files"

while IFS= read -r relative_path; do
  relative_path="${relative_path#./}"
  arm64_file="$arm64_dir/$relative_path"
  x64_file="$x64_dir/$relative_path"
  output_file="$output_dir/$relative_path"

  if [[ -f "$arm64_file" && -f "$x64_file" ]]; then
    arm64_is_macho=0
    x64_is_macho=0
    if is_macho "$arm64_file"; then
      arm64_is_macho=1
    fi
    if is_macho "$x64_file"; then
      x64_is_macho=1
    fi

    if [[ "$arm64_is_macho" -eq 1 || "$x64_is_macho" -eq 1 ]]; then
      if [[ "$arm64_is_macho" -ne 1 || "$x64_is_macho" -ne 1 ]]; then
        echo "Cannot merge $relative_path because it is Mach-O in only one input runtime." >&2
        exit 1
      fi

      mkdir -p "$(dirname "$output_file")"
      rm -f "$output_file"
      lipo -create "$arm64_file" "$x64_file" -output "$output_file"
      if [[ -x "$arm64_file" || -x "$x64_file" ]]; then
        chmod 755 "$output_file"
      else
        chmod 644 "$output_file"
      fi
      lipo "$output_file" -verify_arch arm64 x86_64 >/dev/null
    elif ! cmp -s "$arm64_file" "$x64_file"; then
      echo "Keeping arm64 copy of differing non-Mach-O runtime file: $relative_path"
    fi
  elif [[ -f "$x64_file" && ! -e "$output_file" ]]; then
    mkdir -p "$(dirname "$output_file")"
    ditto "$x64_file" "$output_file"
  fi
done < "$relative_files"

for required_library in libvips-cpp.dylib libvips-cpp.42.dylib; do
  library_path="$output_dir/$required_library"
  if [[ ! -f "$library_path" ]]; then
    echo "Universal libvips runtime is missing $required_library." >&2
    exit 1
  fi
  lipo "$library_path" -verify_arch arm64 x86_64 >/dev/null
done

echo "Created universal macOS libvips runtime at $output_dir."
