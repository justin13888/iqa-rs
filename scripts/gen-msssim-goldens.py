#!/usr/bin/env python3
#
# gen-msssim-goldens.py — regenerate the reference values that
# tests/ms_ssim_reference.rs checks `iqa::msssim` against.
#
# This is an INDEPENDENT NumPy reimplementation of the published five-scale
# MS-SSIM (Wang, Simoncelli & Bovik, "Multiscale Structural Similarity for Image
# Quality Assessment", 2003) — the same algorithm Daala's tools/dump_msssim.c
# implements. It is written from the paper rather than from the Rust: a
# different language, a separate convolution, and the textbook E[x^2]-E[x]^2
# variance form (the crate uses the deviation form). Agreement between the two
# is therefore real evidence the port is correct, not a restatement of it.
#
# It reads the committed grayscale PGM fixtures and prints the Rust `GOLDENS`
# table to paste into tests/ms_ssim_reference.rs. The fixtures are the single
# source of truth: this script and the Rust test read the identical .pgm files.
#
#   python3 scripts/gen-msssim-goldens.py
#
# Regenerate the fixtures first if they changed:
#   cargo test --features ms-ssim --test ms_ssim_reference \
#       write_ms_ssim_fixtures -- --ignored
#
# ---------------------------------------------------------------------------
# Authoritative re-pin (TODO): the canonical oracle is Daala's own dump_msssim
# binary. Daala builds with autotools and the tool consumes Y4M, so the recipe
# is, roughly:
#
#   git clone https://github.com/xiph/daala && cd daala
#   git checkout <PINNED_COMMIT>          # pin and record the commit here
#   ./autogen.sh && ./configure && make tools/dump_msssim
#   # wrap each grayscale fixture as a luma Y4M (Y = gray, neutral chroma):
#   ffmpeg -i tests/fixtures/ms_ssim/<f>.pgm -pix_fmt yuv420p -f yuv4mpegpipe <f>.y4m
#   tools/dump_msssim <ref>.y4m <dist>.y4m   # read the Y/luma MS-SSIM it prints
#
# Doing this needs autoconf/automake + ffmpeg, which were unavailable where the
# current values were generated. Once re-pinned, update this header and flip the
# README MS-SSIM row to "Stable and tested".
# ---------------------------------------------------------------------------

import os
import sys

import numpy as np
from numpy.lib.stride_tricks import sliding_window_view

WINDOW = 11
SIGMA = 1.5
K1, K2 = 0.01, 0.03
WEIGHTS = (0.0448, 0.2856, 0.3001, 0.2363, 0.1333)

FIXTURE_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "tests",
    "fixtures",
    "ms_ssim",
)

# (case label, reference fixture, distorted fixture) — keep in lockstep with the
# CASES table in tests/ms_ssim_reference.rs.
CASES = [
    ("gradient_lo", "gradient_ref.pgm", "gradient_lo.pgm"),
    ("gradient_hi", "gradient_ref.pgm", "gradient_hi.pgm"),
    ("solid_dist", "solid_ref.pgm", "solid_dist.pgm"),
    ("texture_dist", "texture_ref.pgm", "texture_dist.pgm"),
]


def read_pgm(path):
    """Reads a binary (P5) PGM into an (h, w) float64 array."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] != b"P5":
        raise ValueError(f"{path}: not a binary PGM")
    idx = 2
    fields = []
    while len(fields) < 3:
        while idx < len(data) and data[idx] in b" \t\n\r":
            idx += 1
        if data[idx : idx + 1] == b"#":
            while data[idx] not in b"\n":
                idx += 1
            continue
        start = idx
        while data[idx] not in b" \t\n\r":
            idx += 1
        fields.append(int(data[start:idx]))
    w, h, _maxv = fields
    idx += 1  # exactly one whitespace byte follows maxval
    buf = np.frombuffer(data[idx : idx + w * h], dtype=np.uint8).astype(np.float64)
    return buf.reshape(h, w)


def gaussian_window():
    c = (WINDOW - 1) / 2.0
    ax = np.arange(WINDOW) - c
    xx, yy = np.meshgrid(ax, ax)
    w = np.exp(-(xx**2 + yy**2) / (2 * SIGMA**2))
    return w / w.sum()


def scale_means(a, b, win, c1, c2):
    """Mean full-SSIM and mean contrast-structure over all valid windows."""
    wa = sliding_window_view(a, (WINDOW, WINDOW))
    wb = sliding_window_view(b, (WINDOW, WINDOW))
    mu_a = np.einsum("ijkl,kl->ij", wa, win)
    mu_b = np.einsum("ijkl,kl->ij", wb, win)
    aa = np.einsum("ijkl,kl->ij", wa * wa, win)
    bb = np.einsum("ijkl,kl->ij", wb * wb, win)
    ab = np.einsum("ijkl,kl->ij", wa * wb, win)
    var_a = aa - mu_a * mu_a
    var_b = bb - mu_b * mu_b
    cov = ab - mu_a * mu_b
    luminance = (2 * mu_a * mu_b + c1) / (mu_a * mu_a + mu_b * mu_b + c1)
    cs = (2 * cov + c2) / (var_a + var_b + c2)
    return float((luminance * cs).mean()), float(cs.mean())


def downsample(img):
    """Average each aligned 2x2 block, dropping any odd trailing row/column."""
    h, w = img.shape
    h2, w2 = (h // 2) * 2, (w // 2) * 2
    img = img[:h2, :w2]
    return (
        img[0::2, 0::2] + img[1::2, 0::2] + img[0::2, 1::2] + img[1::2, 1::2]
    ) / 4.0


def msssim(a, b):
    c1 = (K1 * 255.0) ** 2
    c2 = (K2 * 255.0) ** 2
    win = gaussian_window()
    n = len(WEIGHTS)
    product = 1.0
    for i in range(n):
        full, cs = scale_means(a, b, win, c1, c2)
        term = full if i == n - 1 else cs
        product *= max(term, 1e-12) ** WEIGHTS[i]
        if i < n - 1:
            a, b = downsample(a), downsample(b)
    return product


def main():
    print('// Independent NumPy MS-SSIM reference, scripts/gen-msssim-goldens.py.')
    print("const GOLDENS: &[(&str, f64)] = &[")
    for label, ref, dist in CASES:
        a = read_pgm(os.path.join(FIXTURE_DIR, ref))
        b = read_pgm(os.path.join(FIXTURE_DIR, dist))
        print(f'    ("{label}", {msssim(a, b):.6f}),')
    print("];")


if __name__ == "__main__":
    sys.exit(main())
