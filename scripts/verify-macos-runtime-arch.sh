#!/usr/bin/env bash
# Copyright (c) 2026 Remgrandt Works. All rights reserved.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This verifier is for macOS Mach-O runtimes only." >&2
  exit 1
fi

if [[ "$#" -ne 3 ]]; then
  echo "Usage: $0 <runtime-or-app-path> <arch> <max-min-version>" >&2
  exit 1
fi

runtime_path="$1"
expected_arch="$2"
max_min_version="$3"

if [[ ! -e "$runtime_path" ]]; then
  echo "Missing path to verify: $runtime_path" >&2
  exit 1
fi

if ! command -v lipo >/dev/null 2>&1; then
  echo "lipo is required to verify Mach-O architectures." >&2
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
failure_count=0
while IFS= read -r -d '' candidate; do
  if ! is_macho "$candidate"; then
    continue
  fi

  mach_o_count=$((mach_o_count + 1))
  if ! lipo "$candidate" -verify_arch "$expected_arch" >/dev/null; then
    echo "$candidate does not contain required architecture $expected_arch." >&2
    failure_count=$((failure_count + 1))
    continue
  fi

  minos="$(minos_for_arch "$candidate" "$expected_arch")"
  if [[ -z "$minos" ]]; then
    echo "Could not determine macOS minimum version for $expected_arch slice in $candidate." >&2
    failure_count=$((failure_count + 1))
    continue
  fi

  if ! version_le "$minos" "$max_min_version"; then
    echo "$candidate $expected_arch slice requires macOS $minos; maximum allowed is $max_min_version." >&2
    failure_count=$((failure_count + 1))
  fi
done < <(find "$runtime_path" -type f -print0)

if [[ "$mach_o_count" -eq 0 ]]; then
  echo "No Mach-O files found under $runtime_path." >&2
  exit 1
fi

if [[ "$failure_count" -ne 0 ]]; then
  echo "$failure_count compatibility issue(s) found while checking $expected_arch runtime." >&2
  exit 1
fi

echo "Verified $mach_o_count $expected_arch Mach-O files under $runtime_path for macOS $max_min_version compatibility."
