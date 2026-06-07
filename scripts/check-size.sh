#!/usr/bin/env bash
#
# check-size.sh — measure iqa's compiled footprint per feature combination and
# verify the SSIMULACRA2 FFI dependency compiles away cleanly when its feature
# is disabled.
#
# For every unique feature-flag combination it builds the library and reports:
#   * the size of the compiled `libiqa.rlib`
#   * the total bytes of native `.a` archives the build script emits (the
#     `cc`-compiled libssimulacra2_shim.a + libhwy.a — see build.rs)
#
# It then enforces two hard invariants for every *pure-Rust* combo (anything
# without `ssimulacra2`):
#   1. zero native `.a` bytes — the FFI must contribute no compiled code, and
#   2. neither `cc` nor `pkg-config` appears in the build-dependency graph.
# A violation exits non-zero so CI can gate on it.
#
# Finally it reports the published-crate size (`cargo package --list`) as an
# informational signal for "is the library reasonably sized given its scope".
#
# Portability: sizes are measured with `wc -c` (POSIX, identical on macOS/BSD
# and Linux/GNU) rather than `stat`, whose size flag differs between the two
# (`stat -f%z` vs `stat -c%s`).

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

PROFILE="release"
BASE_TARGET="${SIZE_TARGET_DIR:-target/size-check}"

# combo label | cargo feature flags | kind (pure|ffi)
COMBOS=(
  "none|--no-default-features|pure"
  "psnr|--no-default-features --features psnr|pure"
  "ssim|--no-default-features --features ssim|pure"
  "psnr+ssim|--no-default-features --features psnr,ssim|pure"
  "ssimulacra2|--no-default-features --features ssimulacra2,vendored-lcms2|ffi"
  "all|--no-default-features --features psnr,ssim,ssimulacra2,vendored-lcms2|ffi"
)

# Total size in bytes of the given files. Missing paths contribute 0. Uses
# `wc -c` for cross-platform consistency (see header note).
bytes() {
  local total=0 n f
  for f in "$@"; do
    [ -f "$f" ] || continue
    n=$(wc -c < "$f")
    total=$((total + n))
  done
  printf '%s' "$total"
}

# Render a byte count as a human-readable string (display only).
human() {
  local b=$1
  if [ "$b" -ge 1048576 ]; then
    awk -v b="$b" 'BEGIN { printf "%.2f MiB", b / 1048576 }'
  elif [ "$b" -ge 1024 ]; then
    awk -v b="$b" 'BEGIN { printf "%.1f KiB", b / 1024 }'
  else
    printf '%s B' "$b"
  fi
}

fail=0
rows=()

for entry in "${COMBOS[@]}"; do
  IFS='|' read -r label flags kind <<< "$entry"

  echo ">> building '$label' ($flags)" >&2
  tdir="$BASE_TARGET/$label"
  # shellcheck disable=SC2086
  CARGO_TARGET_DIR="$tdir" cargo build --quiet --profile "$PROFILE" --lib $flags

  rlib=$(find "$tdir/$PROFILE" -maxdepth 1 -name 'libiqa*.rlib' -print -quit 2>/dev/null || true)
  rlib_bytes=$(bytes "$rlib")

  # cc emits its archives into target/<profile>/build/iqa-*/out/.
  native_files=()
  while IFS= read -r f; do
    [ -n "$f" ] && native_files+=("$f")
  done < <(find "$tdir/$PROFILE/build" -path '*/out/*.a' 2>/dev/null || true)
  native_bytes=$(bytes "${native_files[@]+"${native_files[@]}"}")

  if [ "$kind" = "pure" ]; then
    if [ "$native_bytes" -ne 0 ]; then
      echo "::error::FFI did not compile away for '$label': ${native_bytes} bytes of native archives present" >&2
      fail=1
    fi
    # shellcheck disable=SC2086
    tree=$(CARGO_TARGET_DIR="$tdir" cargo tree --quiet -e build $flags 2>/dev/null || true)
    if printf '%s\n' "$tree" | grep -Eq '(^|[^[:alnum:]_-])(cc|pkg-config)[[:space:]]+v'; then
      echo "::error::a native build-dependency (cc/pkg-config) leaked into pure-Rust combo '$label'" >&2
      fail=1
    fi
  fi

  expected=$([ "$kind" = "ffi" ] && echo "yes" || echo "no")
  rows+=("| \`$label\` | $(human "$rlib_bytes") | $(human "$native_bytes") | $expected |")
done

# Published-crate size (informational; feature-independent — the vendored C++
# always ships in the tarball).
pkg_files=$(cargo package --list --allow-dirty 2>/dev/null)
pkg_count=$(printf '%s\n' "$pkg_files" | grep -c . || true)
pkg_bytes=0
while IFS= read -r f; do
  [ -f "$f" ] || continue
  n=$(wc -c < "$f")
  pkg_bytes=$((pkg_bytes + n))
done <<< "$pkg_files"

echo "## iqa size report"
echo
echo "### Compiled footprint per feature combination"
echo
echo "| feature combo | libiqa.rlib | native \`.a\` | FFI expected |"
echo "|---|---|---|---|"
printf '%s\n' "${rows[@]}"
echo
echo "_Native \`.a\` bytes must be 0 for every pure-Rust combo; non-zero fails the check._"
echo
echo "### Published crate (\`cargo package\`)"
echo
echo "- files: $pkg_count"
echo "- uncompressed: $(human "$pkg_bytes")"

if [ "$fail" -ne 0 ]; then
  echo >&2
  echo "FAIL: FFI-compiles-away invariant violated (see ::error:: above)." >&2
fi

exit "$fail"
