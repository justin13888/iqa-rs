#!/usr/bin/env bash
#
# gen-msssim-goldens.sh — regenerate the reference values that
# tests/ms_ssim_reference.rs checks `iqa::msssim` against, by running the
# ORIGINAL reference implementation: Wang/Simoncelli/Bovik's `msssim.m`
# (https://ece.uwaterloo.ca/~z70wang/research/ssim/, msssim.zip — contains
# `msssim.m` and `ssim_index_new.m`). That is the canonical five-scale MS-SSIM;
# `iqa::msssim` reproduces it (the same 11x11 Gaussian, weights, and
# cs-then-full-SSIM combination), so this matches our output against the source
# of truth on identical pixels (the committed grayscale PGM fixtures, which
# Octave's `imread` and the Rust test both read).
#
#   scripts/gen-msssim-goldens.sh
#
# Regenerate the fixtures first if they changed:
#   cargo test --features ms-ssim --test ms_ssim_reference \
#       write_ms_ssim_fixtures -- --ignored
#
# Not run in CI (it needs Octave). The committed goldens are pinned from its
# output; `tests/ms_ssim_reference.rs` does the comparison in CI.
# `scripts/gen-msssim-goldens.py` is an independent NumPy reimplementation kept
# as a no-Octave cross-check; both produce identical values.
#
# Dependencies: `octave` with the `image` package (for `imread`, `fspecial`,
# `imfilter`). On macOS:
#   brew install octave && octave --eval "pkg install -forge image"
#
# `msssim.m`/`ssim_index_new.m` are downloaded into $REF_DIR (default
# target/ref-oracle, gitignored and excluded from the packaged crate). They are
# academically licensed — research use only — so they are fetched on demand and
# never committed or published.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
FIXTURES="$REPO_ROOT/tests/fixtures/ms_ssim"
REF_DIR="${IQA_REF_ORACLE_DIR:-$REPO_ROOT/target/ref-oracle}"
MSSSIM_ZIP_URL="https://ece.uwaterloo.ca/~z70wang/research/ssim/msssim.zip"

command -v octave >/dev/null || { echo "error: octave not found (brew install octave)" >&2; exit 1; }

mkdir -p "$REF_DIR"

if [ ! -f "$REF_DIR/msssim.m" ] || [ ! -f "$REF_DIR/ssim_index_new.m" ]; then
  echo ">> downloading reference msssim.m (research-use-only; not committed)..." >&2
  curl -fsSL -o "$REF_DIR/msssim.zip" "$MSSSIM_ZIP_URL"
  ( cd "$REF_DIR" && unzip -o msssim.zip msssim.m ssim_index_new.m >/dev/null )
fi

# Driver: print the Rust GOLDENS table from msssim.m over each fixture pair.
cat > "$REF_DIR/gen_msssim.m" <<'OCTAVE'
pkg load image;
fixdir = argv(){1};
cases = {
  'gradient_lo',  'gradient_ref.pgm', 'gradient_lo.pgm';
  'gradient_hi',  'gradient_ref.pgm', 'gradient_hi.pgm';
  'solid_dist',   'solid_ref.pgm',    'solid_dist.pgm';
  'texture_dist', 'texture_ref.pgm',  'texture_dist.pgm';
};
printf('// Wang msssim.m MS-SSIM, via scripts/gen-msssim-goldens.sh.\n');
printf('const GOLDENS: &[(&str, f64)] = &[\n');
for i = 1:size(cases, 1)
  a = double(imread(fullfile(fixdir, cases{i, 2})));
  b = double(imread(fullfile(fixdir, cases{i, 3})));
  printf('    ("%s", %.6f),\n', cases{i, 1}, msssim(a, b));
end
printf('];\n');
OCTAVE

echo ">> scoring fixtures with msssim.m..." >&2
octave --no-gui --norc --path "$REF_DIR" "$REF_DIR/gen_msssim.m" "$FIXTURES" 2>/dev/null
