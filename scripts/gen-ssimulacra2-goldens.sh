#!/usr/bin/env bash
#
# gen-ssimulacra2-goldens.sh — regenerate the reference values that
# tests/ssimulacra2_reference.rs checks `iqa::ssimulacra2` against.
#
# It builds the `ssimulacra2` reference tool from the EXACT cloudinary source we
# vendor (third_party/ssimulacra2/src — SSIMULACRA 2.1), runs it over the
# committed PPM fixtures, and prints the Rust `GOLDENS` table to paste into
# tests/ssimulacra2_reference.rs. The fixtures are the single source of truth:
# this tool and the Rust test read the identical `.ppm` files, so the comparison
# isolates exactly what our FFI wrapper adds (sRGB tagging, normalization, planar
# packing, the SIMD row-padding fill).
#
# Why build the vendored submodule and not a libjxl release: SSIMULACRA2 changed
# numerically between 2.0 and 2.1 (April 2023 — different XYB rescaling and
# retuned weights). libjxl <= v0.8.2 ships 2.0, which does NOT match our vendored
# 2.1 (gradient_lo scores 19.3 under 2.0 vs 53.8 under 2.1). Building the pinned
# submodule source is the only oracle guaranteed to be the same algorithm version
# our shim binds — and its `ssimulacra2_main.cc` loads images via its own
# libjxl path (SetFromFile -> CodecInOut), independent of our shim's MakeBundle,
# so it still cross-validates our wrapper rather than tautologically agreeing.
#
# `ssimulacra2 ref.ppm dist.ppm` prints one line: the score (-inf..100, 100 =
# identical). SSIMULACRA2 takes no tuning parameters.
#
# Not run in CI (it builds the vendored libjxl subset). Run it by hand after
# changing the fixtures (regenerate those first with the ignored writer test:
#   cargo test --features ssimulacra2 --test ssimulacra2_reference \
#       write_ssimulacra2_fixtures -- --ignored
# ) or to re-verify the goldens against upstream.
#
# Build cache: $SSIMULACRA2_REF_DIR (default target/ssimulacra2-ref, gitignored).
# Delete it to force a clean rebuild.
#
# System dependencies (the vendored src links them; only the PNM decoder is
# exercised, but the tool links the full extras codec set):
#   macOS:  brew install cmake ninja highway little-cms2 jpeg-turbo libpng
#   Debian: sudo apt install cmake ninja-build libhwy-dev liblcms2-dev \
#                            libjpeg-dev libpng-dev
# Goldens were generated with Highway 1.4.0 + lcms2 2.19; a different SIMD target
# or library version shifts the value only in the last few digits, well inside
# the 1% test tolerance.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SRC="$REPO_ROOT/third_party/ssimulacra2/src"
FIXTURES="$REPO_ROOT/tests/fixtures/ssimulacra2"
REF_DIR="${SSIMULACRA2_REF_DIR:-$REPO_ROOT/target/ssimulacra2-ref}"
BUILD="$REF_DIR/build"
BIN="$BUILD/ssimulacra2"

# (case label, reference fixture, distorted fixture) — keep in lockstep with the
# CASES table in tests/ssimulacra2_reference.rs. SSIMULACRA2 is asymmetric, so
# the reference must come first.
CASES=(
  "gradient_lo   gradient_ref.ppm gradient_lo.ppm"
  "gradient_hi   gradient_ref.ppm gradient_hi.ppm"
  "chroma_dist   chroma_ref.ppm   chroma_dist.ppm"
  "oddwidth_dist oddwidth_ref.ppm oddwidth_dist.ppm"
)

# Non-aligned-width cases whose reference value we only trust if the tool prints
# the same score across repeated runs — i.e. the reference's own image loader
# must initialize its SIMD row padding deterministically for the golden to be
# admissible. (It does, as of the pinned submodule; this gate guards regressions.)
ODD_CASES="oddwidth_dist"

build_tool() {
  if [ -x "$BIN" ]; then
    echo ">> reusing cached ssimulacra2 ($BIN)" >&2
    return
  fi
  command -v cmake >/dev/null || { echo "error: cmake not found" >&2; exit 1; }
  command -v ninja >/dev/null || { echo "error: ninja not found" >&2; exit 1; }
  [ -f "$SRC/ssimulacra2.cc" ] || {
    echo "error: vendored source missing at $SRC; run" >&2
    echo "       git submodule update --init third_party/ssimulacra2" >&2
    exit 1
  }

  echo ">> configuring vendored cloudinary src (system highway + lcms2)..." >&2
  cmake -S "$SRC" -B "$BUILD" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DJPEGXL_FORCE_SYSTEM_HWY=true \
    -DJPEGXL_FORCE_SYSTEM_LCMS2=true \
    -DJPEGXL_ENABLE_BENCHMARK=OFF \
    -DJPEGXL_ENABLE_EXAMPLES=OFF \
    -DJPEGXL_ENABLE_DOXYGEN=OFF \
    -DJPEGXL_ENABLE_MANPAGES=OFF \
    -DJPEGXL_ENABLE_JNI=OFF \
    -DJPEGXL_ENABLE_PLUGINS=OFF \
    -DJPEGXL_ENABLE_SKCMS=OFF >&2

  echo ">> building ssimulacra2..." >&2
  cmake --build "$BUILD" --target ssimulacra2 -j"$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)" >&2
  [ -x "$BIN" ] || { echo "error: build did not produce $BIN" >&2; exit 1; }
}

# Prints the single score the tool emits for ref/dist.
score() {
  "$BIN" "$1" "$2" | awk 'NR==1{print $1}'
}

# Returns 0 if the tool prints the same score across 5 runs.
is_stable() {
  local first run
  first=$(score "$1" "$2")
  for _ in 1 2 3 4; do
    run=$(score "$1" "$2")
    [ "$run" = "$first" ] || return 1
  done
  return 0
}

build_tool

# Confirm the binary is 2.1, not a stray 2.0 — the usage banner prints the version.
ver=$("$BIN" 2>&1 | head -1 || true)
case "$ver" in
  *"SSIMULACRA 2.1"*) : ;;
  *) echo "error: oracle is not SSIMULACRA 2.1 (banner: $ver)" >&2; exit 1 ;;
esac

echo ">> scoring fixtures with ssimulacra2 (SSIMULACRA 2.1)..." >&2
echo "// Vendored cloudinary ssimulacra2 (SSIMULACRA 2.1, no params)."
echo "// See scripts/gen-ssimulacra2-goldens.sh."
echo "const GOLDENS: &[(&str, f64)] = &["
for entry in "${CASES[@]}"; do
  # shellcheck disable=SC2086
  set -- $entry
  label=$1 ref=$2 dist=$3
  [ -f "$FIXTURES/$ref" ]  || { echo "error: missing fixture $ref"  >&2; exit 1; }
  [ -f "$FIXTURES/$dist" ] || { echo "error: missing fixture $dist" >&2; exit 1; }

  if [[ " $ODD_CASES " == *" $label "* ]]; then
    if ! is_stable "$FIXTURES/$ref" "$FIXTURES/$dist"; then
      echo "warning: $label is non-deterministic across runs (the reference does" >&2
      echo "         not initialize its SIMD row padding); dropping it." >&2
      echo "         Remove the \"$label\" case + fixtures from" >&2
      echo "         tests/ssimulacra2_reference.rs; the in-crate odd-width tests" >&2
      echo "         (tests/ssimulacra2.rs) still cover the padding fix." >&2
      continue
    fi
  fi

  val=$(score "$FIXTURES/$ref" "$FIXTURES/$dist")
  printf '    ("%s", %s),\n' "$label" "$val"
done
echo "];"
