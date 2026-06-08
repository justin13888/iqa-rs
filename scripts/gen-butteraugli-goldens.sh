#!/usr/bin/env bash
#
# gen-butteraugli-goldens.sh — regenerate the reference values that
# tests/butteraugli_reference.rs checks `iqa::butteraugli` against.
#
# It builds libjxl's own `butteraugli_main` from the SAME version vendored under
# third_party/ (v0.8.2), runs it over the committed PPM fixtures, and prints the
# Rust `GOLDENS` table to paste into tests/butteraugli_reference.rs. The fixtures
# are the single source of truth: this tool and the Rust test read the identical
# `.ppm` files, so the comparison isolates exactly what our FFI wrapper adds
# (sRGB tagging, normalization, planar packing, parameter wiring).
#
# `butteraugli_main ref.ppm dist.ppm` prints two lines — the max-norm
# ButteraugliDistance, then `3-norm: <value>` from ComputeDistanceP(.., 3.0).
# Its defaults (intensity_target=80, hf_asymmetry=1.0, pnorm=3) are exactly
# `ButteraugliOptions::default()`, so the `3-norm:` line is the value we match.
#
# Not run in CI (it builds libjxl). Run it by hand after changing the fixtures
# (regenerate those first with the ignored writer test:
#   cargo test --features butteraugli --test butteraugli_reference \
#       write_butteraugli_fixtures -- --ignored
# ) or to re-verify the goldens against upstream.
#
# Build cache: $BUTTERAUGLI_REF_DIR (default target/butteraugli-ref, gitignored).
# Delete it to force a clean rebuild.
#
# macOS note: the build passes -DCMAKE_IGNORE_PREFIX_PATH=/opt/homebrew so a
# newer Homebrew libjxl header can't shadow the in-tree v0.8.2 one, and disables
# the optional image codecs (PNG/JPEG/EXR/GIF) so no system libpng/etc. is
# needed — hence PPM fixtures, which libjxl decodes natively.

set -euo pipefail

LIBJXL_TAG="v0.8.2"
REPO_ROOT="$(git rev-parse --show-toplevel)"
FIXTURES="$REPO_ROOT/tests/fixtures/butteraugli"
REF_DIR="${BUTTERAUGLI_REF_DIR:-$REPO_ROOT/target/butteraugli-ref}"
SRC="$REF_DIR/libjxl"
BIN="$SRC/build/tools/butteraugli_main"

# (case label, reference fixture, distorted fixture) — keep in lockstep with the
# CASES table in tests/butteraugli_reference.rs. Butteraugli is asymmetric, so
# the reference must come first.
CASES=(
  "gradient_lo gradient_ref.ppm gradient_lo.ppm"
  "gradient_hi gradient_ref.ppm gradient_hi.ppm"
  "solid_dist  solid_ref.ppm   solid_dist.ppm"
  "chroma_dist chroma_ref.ppm  chroma_dist.ppm"
)

build_tool() {
  if [ -x "$BIN" ]; then
    echo ">> reusing cached butteraugli_main ($BIN)" >&2
    return
  fi
  command -v cmake >/dev/null || { echo "error: cmake not found" >&2; exit 1; }
  command -v ninja >/dev/null || { echo "error: ninja not found" >&2; exit 1; }

  mkdir -p "$REF_DIR"
  if [ ! -d "$SRC" ]; then
    echo ">> cloning libjxl $LIBJXL_TAG (shallow)..." >&2
    git clone --depth 1 --branch "$LIBJXL_TAG" https://github.com/libjxl/libjxl.git "$SRC" >&2
  fi
  echo ">> fetching libjxl deps (deps.sh)..." >&2
  ( cd "$SRC" && ./deps.sh ) >&2

  echo ">> configuring (Release, devtools on, external codecs off)..." >&2
  cmake -S "$SRC" -B "$SRC/build" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DCMAKE_IGNORE_PREFIX_PATH=/opt/homebrew \
    -DBUILD_TESTING=OFF \
    -DJPEGXL_ENABLE_TOOLS=ON \
    -DJPEGXL_ENABLE_DEVTOOLS=ON \
    -DJPEGXL_ENABLE_BENCHMARK=OFF \
    -DJPEGXL_ENABLE_EXAMPLES=OFF \
    -DJPEGXL_ENABLE_DOXYGEN=OFF \
    -DJPEGXL_ENABLE_MANPAGES=OFF \
    -DJPEGXL_ENABLE_JNI=OFF \
    -DJPEGXL_ENABLE_PLUGINS=OFF \
    -DJPEGXL_ENABLE_APNG=OFF \
    -DJPEGXL_ENABLE_GIF=OFF \
    -DJPEGXL_ENABLE_EXR=OFF \
    -DJPEGXL_ENABLE_JPEG=OFF \
    -DJPEGXL_ENABLE_SJPEG=OFF >&2

  echo ">> building butteraugli_main..." >&2
  cmake --build "$SRC/build" --target butteraugli_main -j"$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)" >&2
  [ -x "$BIN" ] || { echo "error: build did not produce $BIN" >&2; exit 1; }
}

build_tool

echo ">> scoring fixtures with butteraugli_main (default params)..." >&2
echo "// libjxl $LIBJXL_TAG butteraugli_main \"3-norm:\" line, default params"
echo "// (intensity_target=80, hf_asymmetry=1.0, pnorm=3). See"
echo "// scripts/gen-butteraugli-goldens.sh."
echo "const GOLDENS: &[(&str, f64)] = &["
for entry in "${CASES[@]}"; do
  # shellcheck disable=SC2086
  set -- $entry
  label=$1 ref=$2 dist=$3
  [ -f "$FIXTURES/$ref" ]  || { echo "error: missing fixture $ref"  >&2; exit 1; }
  [ -f "$FIXTURES/$dist" ] || { echo "error: missing fixture $dist" >&2; exit 1; }
  out=$("$BIN" "$FIXTURES/$ref" "$FIXTURES/$dist")
  p3=$(printf '%s\n' "$out" | awk 'NR==2{print $2}')
  printf '    ("%s", %s),\n' "$label" "$p3"
done
echo "];"
