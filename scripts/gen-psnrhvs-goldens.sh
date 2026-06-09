#!/usr/bin/env bash
#
# gen-psnrhvs-goldens.sh — regenerate the reference values that
# tests/psnr_hvs_m_reference.rs checks `iqa::psnr_hvs_m` against, by running the
# ORIGINAL reference implementation: Nikolay Ponomarenko's `psnrhvsm.m`
# (https://www.ponomarenko.info/psnrhvsm.m). That is the canonical definition of
# PSNR-HVS-M; `iqa::psnr_hvs_m` is a direct port of it, so this matches our output
# against the source of truth on identical pixels (the committed grayscale PGM
# fixtures, which Octave's `imread` and the Rust test both read).
#
#   scripts/gen-psnrhvs-goldens.sh
#
# Regenerate the fixtures first if they changed:
#   cargo test --features psnr-hvs-m --test psnr_hvs_m_reference \
#       write_psnr_hvs_m_fixtures -- --ignored
#
# Not run in CI (it needs Octave). The committed goldens are pinned from its
# output; `tests/psnr_hvs_m_reference.rs` does the comparison in CI.
# `scripts/gen-psnrhvs-goldens.py` is an independent NumPy reimplementation kept
# as a no-Octave cross-check; both produce identical values.
#
# Dependencies: `octave` with the `image` package (for `imread`). On macOS:
#   brew install octave && octave --eval "pkg install -forge image"
#
# `psnrhvsm.m` is downloaded into $REF_DIR (default target/ref-oracle, gitignored
# and excluded from the packaged crate). It is academically licensed — research
# use only — so it is fetched on demand and never committed or published. The
# only thing it needs that the image package doesn't ship is `dct2`, the standard
# orthonormal 2-D DCT-II, which this script writes as a shim.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
FIXTURES="$REPO_ROOT/tests/fixtures/psnr_hvs_m"
REF_DIR="${IQA_REF_ORACLE_DIR:-$REPO_ROOT/target/ref-oracle}"
PSNRHVSM_URL="https://www.ponomarenko.info/psnrhvsm.m"

command -v octave >/dev/null || { echo "error: octave not found (brew install octave)" >&2; exit 1; }

mkdir -p "$REF_DIR"

if [ ! -f "$REF_DIR/psnrhvsm.m" ]; then
  echo ">> downloading reference psnrhvsm.m (research-use-only; not committed)..." >&2
  curl -fsSL -o "$REF_DIR/psnrhvsm.m" "$PSNRHVSM_URL"
fi

# dct2 shim: the standard orthonormal 2-D DCT-II MATLAB's dct2 computes. Not part
# of the masking algorithm; written here only because the image package omits it.
cat > "$REF_DIR/dct2.m" <<'OCTAVE'
function B = dct2(A)
  A = double(A);
  [m, n] = size(A);
  B = dctmat(m) * A * dctmat(n).';
end

function D = dctmat(N)
  k = (0:N-1).';
  x = (0:N-1);
  D = sqrt(2 / N) * cos(pi * (2 * x + 1) .* k / (2 * N));
  D(1, :) = D(1, :) / sqrt(2);
end
OCTAVE

# Driver: print the Rust GOLDENS table from psnrhvsm.m over each fixture pair.
cat > "$REF_DIR/gen_psnrhvs.m" <<'OCTAVE'
pkg load image;
fixdir = argv(){1};
cases = {
  'gradient_lo',  'gradient_ref.pgm', 'gradient_lo.pgm';
  'gradient_hi',  'gradient_ref.pgm', 'gradient_hi.pgm';
  'solid_dist',   'solid_ref.pgm',    'solid_dist.pgm';
  'texture_dist', 'texture_ref.pgm',  'texture_dist.pgm';
};
printf('// Ponomarenko psnrhvsm.m PSNR-HVS-M, via scripts/gen-psnrhvs-goldens.sh.\n');
printf('const GOLDENS: &[(&str, f64)] = &[\n');
for i = 1:size(cases, 1)
  a = double(imread(fullfile(fixdir, cases{i, 2})));
  b = double(imread(fullfile(fixdir, cases{i, 3})));
  [phvsm, ~] = psnrhvsm(a, b);
  printf('    ("%s", %.6f),\n', cases{i, 1}, phvsm);
end
printf('];\n');
OCTAVE

echo ">> scoring fixtures with psnrhvsm.m..." >&2
octave --no-gui --norc --path "$REF_DIR" "$REF_DIR/gen_psnrhvs.m" "$FIXTURES" 2>/dev/null
