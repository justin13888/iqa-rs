#!/usr/bin/env bash
#
# gen-iwssim-goldens.sh — regenerate the reference values that
# tests/iw_ssim_reference.rs checks `iqa::iwssim` against, by running the
# ORIGINAL reference implementation: Wang & Li's `iwssim.m`
# (https://ece.uwaterloo.ca/~z70wang/research/iwssim/, iwssim_iwpsnr.zip —
# contains `iwssim.m`, `scale_quality_maps.m`, `info_content_weight_map.m`, and
# `imenlarge2.m`) on top of Simoncelli's matlabPyrTools (for `buildLpyr` and
# friends). `iqa::iwssim` reproduces that five-scale metric exactly (the same
# sqrt(2)-scaled binom5 Laplacian pyramid, the same GSM information weighting,
# the same cs-then-full-SSIM combination), so this matches our output against the
# source of truth on identical pixels (the committed grayscale PGM fixtures,
# which Octave's `imread` and the Rust test both read).
#
#   scripts/gen-iwssim-goldens.sh
#
# Regenerate the fixtures first if they changed:
#   cargo test --features iw-ssim --test iw_ssim_reference \
#       write_iw_ssim_fixtures -- --ignored
#
# Not run in CI (it needs Octave). The committed goldens are pinned from its
# output; `tests/iw_ssim_reference.rs` does the comparison in CI.
# `scripts/gen-iwssim-goldens.py` is an independent NumPy/`pyrtools`
# reimplementation kept as a no-Octave cross-check; both produce identical values.
#
# Dependencies: `octave` with the `image` package (for `imread`/`imresize`) and
# `git` (to fetch matlabPyrTools). On macOS:
#   brew install octave && octave --eval "pkg install -forge image"
#
# No MEX compilation is required: matlabPyrTools ships pure-`.m` fallbacks
# (`corrDn.m`/`upConv.m` via `rconv2`, plus `innerProd.m`) that implement the
# exact reflect1 behavior the metric needs; Octave uses them automatically when
# the compiled MEX is absent.
#
# `iwssim_iwpsnr.zip` and matlabPyrTools are fetched into $REF_DIR (default
# target/ref-oracle, gitignored and excluded from the packaged crate). The
# Waterloo code is academically licensed — research use only — so it is fetched
# on demand and never committed or published.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
FIXTURES="$REPO_ROOT/tests/fixtures/iw_ssim"
REF_DIR="${IQA_REF_ORACLE_DIR:-$REPO_ROOT/target/ref-oracle}"
IWSSIM_ZIP_URL="https://ece.uwaterloo.ca/~z70wang/research/iwssim/iwssim_iwpsnr.zip"
PYRTOOLS_REPO="https://github.com/LabForComputationalVision/matlabPyrTools.git"

command -v octave >/dev/null || { echo "error: octave not found (brew install octave)" >&2; exit 1; }
command -v git >/dev/null || { echo "error: git not found" >&2; exit 1; }

mkdir -p "$REF_DIR"

if [ ! -f "$REF_DIR/iwssim.m" ]; then
  echo ">> downloading reference iwssim.m (research-use-only; not committed)..." >&2
  curl -fsSL -o "$REF_DIR/iwssim_iwpsnr.zip" "$IWSSIM_ZIP_URL"
  ( cd "$REF_DIR" && unzip -o iwssim_iwpsnr.zip \
      iwssim.m scale_quality_maps.m info_content_weight_map.m imenlarge2.m >/dev/null )
fi

if [ ! -d "$REF_DIR/matlabPyrTools" ]; then
  echo ">> cloning matlabPyrTools (Simoncelli; not committed)..." >&2
  git clone --depth 1 "$PYRTOOLS_REPO" "$REF_DIR/matlabPyrTools" >/dev/null 2>&1
fi

# Driver: print the Rust GOLDENS table from iwssim.m over each fixture pair.
# `evalc` swallows the matlabPyrTools "compile the MEX" notices the pure-.m
# fallbacks print, so only the data lines reach stdout.
cat > "$REF_DIR/gen_iwssim.m" <<'OCTAVE'
addpath('matlabPyrTools');
addpath('.');
pkg load image;
fixdir = argv(){1};
cases = {
  'gradient_lo',  'gradient_ref.pgm', 'gradient_lo.pgm';
  'gradient_hi',  'gradient_ref.pgm', 'gradient_hi.pgm';
  'texture_lo',   'texture_ref.pgm',  'texture_lo.pgm';
  'texture_hi',   'texture_ref.pgm',  'texture_hi.pgm';
};
printf('// Wang & Li iwssim.m IW-SSIM, via scripts/gen-iwssim-goldens.sh.\n');
printf('const GOLDENS: &[(&str, f64)] = &[\n');
for i = 1:size(cases, 1)
  a = double(imread(fullfile(fixdir, cases{i, 2})));
  b = double(imread(fullfile(fixdir, cases{i, 3})));
  evalc('s = iwssim(a, b);');
  printf('    ("%s", %.6f),\n', cases{i, 1}, s);
end
printf('];\n');
OCTAVE

echo ">> scoring fixtures with iwssim.m..." >&2
# Run from $REF_DIR so the relative addpath('matlabPyrTools') / addpath('.') in
# the driver resolve; $FIXTURES is absolute.
( cd "$REF_DIR" && octave --no-gui --norc gen_iwssim.m "$FIXTURES" )
