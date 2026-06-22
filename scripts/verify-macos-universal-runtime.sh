#!/usr/bin/env bash
# Copyright (c) 2026 Remgrandt Works. All rights reserved.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This verifier is for macOS Mach-O runtimes only." >&2
  exit 1
fi

if [[ "$#" -lt 1 || "$#" -gt 3 ]]; then
  echo "Usage: $0 <runtime-or-app-path> [max-x86_64-min-version] [max-arm64-min-version]" >&2
  exit 1
fi

runtime_path="$1"
max_x64_min_version="${2:-}"
max_arm64_min_version="${3:-$max_x64_min_version}"

if [[ ! -e "$runtime_path" ]]; then
  echo "Missing path to verify: $runtime_path" >&2
  exit 1
fi

if ! command -v lipo >/dev/null 2>&1; then
  echo "lipo is required to verify universal binaries." >&2
  exit 1
fi

is_macho() {
  file "$1" | grep -Eq 'Mach-O'
}

version_le() {
  python3 - "$1" "$2" <<'PY'
import sys

def parts(value):
    return [int(part) for part in value.split(".")]

left = parts(sys.argv[1])
right = parts(sys.argv[2])
length = max(len(left), len(right))
left += [0] * (length - len(left))
right += [0] * (length - len(right))
sys.exit(0 if left <= right else 1)
PY
}

minos_for_arch() {
  local file_path="$1"
  local arch="$2"
  local minos=""

  if command -v vtool >/dev/null 2>&1; then
    minos="$(vtool -arch "$arch" -show-build "$file_path" 2>/dev/null |
      awk '/minos / { print $2; exit }')"
  fi

  if [[ -z "$minos" ]]; then
    minos="$(otool -arch "$arch" -l "$file_path" 2>/dev/null |
      awk '
        /cmd LC_VERSION_MIN_MACOSX/ { in_min = 1; next }
        in_min && /version / { print $2; exit }
      ')"
  fi

  printf '%s' "$minos"
}

mach_o_count=0
while IFS= read -r -d '' candidate; do
  if ! is_macho "$candidate"; then
    continue
  fi

  mach_o_count=$((mach_o_count + 1))
  lipo "$candidate" -verify_arch arm64 x86_64 >/dev/null

  if [[ -n "$max_x64_min_version" || -n "$max_arm64_min_version" ]]; then
    for arch in arm64 x86_64; do
      minos="$(minos_for_arch "$candidate" "$arch")"
      if [[ -z "$minos" ]]; then
        echo "Could not determine macOS minimum version for $arch slice in $candidate." >&2
        exit 1
      fi

      case "$arch" in
        arm64)
          max_min_version="$max_arm64_min_version"
          ;;
        x86_64)
          max_min_version="$max_x64_min_version"
          ;;
        *)
          echo "Unexpected architecture while verifying $candidate: $arch" >&2
          exit 1
          ;;
      esac

      if [[ -n "$max_min_version" ]] && ! version_le "$minos" "$max_min_version"; then
        echo "$candidate $arch slice requires macOS $minos; maximum allowed is $max_min_version." >&2
        exit 1
      fi
    done
  fi
done < <(find "$runtime_path" -type f -print0)

if [[ "$mach_o_count" -eq 0 ]]; then
  echo "No Mach-O files found under $runtime_path." >&2
  exit 1
fi

if find "$runtime_path" -type f -perm -111 -print0 |
  xargs -0 otool -L 2>/dev/null |
  grep -E '/(opt/homebrew|usr/local)/(Cellar|opt)/|/opt/local/' ; then
  echo "Runtime still references package-manager library paths." >&2
  exit 1
fi

echo "Verified $mach_o_count universal Mach-O files under $runtime_path."
