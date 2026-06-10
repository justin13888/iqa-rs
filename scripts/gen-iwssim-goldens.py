#!/usr/bin/env python3
#
# gen-iwssim-goldens.py — a no-Octave cross-check of the IW-SSIM goldens.
#
# The AUTHORITATIVE goldens come from `scripts/gen-iwssim-goldens.sh`, which runs
# the original reference `iwssim.m` (Wang & Li, "Information Content Weighting for
# Perceptual Image Quality Assessment", IEEE TIP 2011) under Octave. This script
# is an INDEPENDENT reimplementation of that same five-scale algorithm, written
# from the reference sources rather than from the Rust: a different language, and
# the Laplacian pyramid built by `pyrtools` — the Simoncelli lab's own Python
# port of the matlabPyrTools `buildLpyr` the reference uses (numerically
# identical: same sqrt(2)-scaled binom5 filter, same reflect1 edges). It exists
# so the goldens can be re-derived without Octave and so a third implementation
# has to agree — it reproduces the reference values exactly.
#
# It reads the committed grayscale PGM fixtures and prints the Rust `GOLDENS`
# table to paste into tests/iw_ssim_reference.rs. The fixtures are the single
# source of truth: this script and the Rust test read the identical .pgm files.
# It also prints the `info_content_weight_map` goldens for the src/iw_ssim.rs
# unit test, which pins the GSM information weighting directly (the end-to-end
# score barely depends on it, so it cannot be pinned through the score alone).
#
#   pip install numpy pyrtools
#   python3 scripts/gen-iwssim-goldens.py
#
# Regenerate the fixtures first if they changed:
#   cargo test --features iw-ssim --test iw_ssim_reference \
#       write_iw_ssim_fixtures -- --ignored

import os
import sys

import numpy as np
import pyrtools as pt

WINDOW = 11
SIGMA = 1.5
K1, K2 = 0.01, 0.03
L = 255.0
WEIGHTS = (0.0448, 0.2856, 0.3001, 0.2363, 0.1333)
NSC = 5
BLK = 3
PARENT = True
SIGMA_NSQ = 0.4
TOL = 1e-15

FIXTURE_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "tests",
    "fixtures",
    "iw_ssim",
)

# (case label, reference fixture, distorted fixture) — keep in lockstep with the
# CASES table in tests/iw_ssim_reference.rs.
CASES = [
    ("gradient_lo", "gradient_ref.pgm", "gradient_lo.pgm"),
    ("gradient_hi", "gradient_ref.pgm", "gradient_hi.pgm"),
    ("texture_lo", "texture_ref.pgm", "texture_lo.pgm"),
    ("texture_hi", "texture_ref.pgm", "texture_hi.pgm"),
]


def read_pgm(path):
    """Reads a binary (P5) PGM into an (h, w) float64 array."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] != b"P5":
        raise ValueError(f"{path}: not a binary PGM")
    idx, fields = 2, []
    while len(fields) < 3:
        while data[idx] in b" \t\n\r":
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


def valid_corr(a, win):
    """Correlate (== convolve, win is symmetric) with no padding ('valid')."""
    from numpy.lib.stride_tricks import sliding_window_view

    k = win.shape[0]
    w = sliding_window_view(a, (k, k))
    return np.einsum("ijkl,kl->ij", w, win)


def box3_same(a):
    """3x3 box mean with zero padding ('same'), like filter2(ones(3)/9, a)."""
    from numpy.lib.stride_tricks import sliding_window_view

    p = np.pad(a, 1)
    return sliding_window_view(p, (3, 3)).sum(axis=(-1, -2)) / 9.0


def scale_quality_maps(band_r, band_d, win):
    """cs map (and, for the coarsest scale, the full SSIM map) of one band."""
    c1 = (K1 * L) ** 2
    c2 = (K2 * L) ** 2
    mu1 = valid_corr(band_r, win)
    mu2 = valid_corr(band_d, win)
    s12 = valid_corr(band_r * band_d, win) - mu1 * mu2
    s1 = np.maximum(valid_corr(band_r * band_r, win) - mu1 * mu1, 0.0)
    s2 = np.maximum(valid_corr(band_d * band_d, win) - mu2 * mu2, 0.0)
    cs = (2 * s12 + c2) / (s1 + s2 + c2)
    luminance = (2 * mu1 * mu2 + c1) / (mu1 * mu1 + mu2 * mu2 + c1)
    return cs, luminance


def imresize_bilinear(im, out_h, out_w):
    """MATLAB/Octave imresize 'bilinear' (enlargement): in = (out-0.5)*s+0.5."""
    M, N = im.shape
    ry = (np.arange(out_h) + 0.5) * (M / out_h) + 0.5
    rx = (np.arange(out_w) + 0.5) * (N / out_w) + 0.5
    ry = np.clip(ry, 1.0, M)
    rx = np.clip(rx, 1.0, N)
    y0 = np.minimum(np.floor(ry).astype(int), M) - 1
    x0 = np.minimum(np.floor(rx).astype(int), N) - 1
    y1 = np.minimum(y0 + 1, M - 1)
    x1 = np.minimum(x0 + 1, N - 1)
    fy = (ry - (y0 + 1))[:, None]
    fx = (rx - (x0 + 1))[None, :]
    a = im[np.ix_(y0, x0)]
    b = im[np.ix_(y0, x1)]
    c = im[np.ix_(y1, x0)]
    d = im[np.ix_(y1, x1)]
    return (1 - fy) * (1 - fx) * a + (1 - fy) * fx * b + fy * (1 - fx) * c + fy * fx * d


def imenlarge2(im):
    """Doubles resolution: bilinear to (4M-3,4N-3), extrapolated border, /2."""
    M, N = im.shape
    t1 = imresize_bilinear(im, 4 * M - 3, 4 * N - 3)
    t2 = np.zeros((4 * M - 1, 4 * N - 1))
    t2[1:-1, 1:-1] = t1
    t2[0, :] = 2 * t2[1, :] - t2[2, :]
    t2[-1, :] = 2 * t2[-2, :] - t2[-3, :]
    t2[:, 0] = 2 * t2[:, 1] - t2[:, 2]
    t2[:, -1] = 2 * t2[:, -2] - t2[:, -3]
    return t2[0::2, 0::2]


def info_content_weight_map(band_r, band_d, parent_r):
    """GSM information-content weight map of one band-pass scale (h-2, w-2)."""
    bh, bw = band_r.shape
    mean_x = box3_same(band_r)
    mean_y = box3_same(band_d)
    cov_xy = box3_same(band_r * band_d) - mean_x * mean_y
    ss_x = box3_same(band_r * band_r) - mean_x * mean_x
    ss_y = box3_same(band_d * band_d) - mean_y * mean_y
    ss_x[ss_x < 0] = 0
    ss_y[ss_y < 0] = 0
    g = cov_xy / (ss_x + TOL)
    vv = ss_y - g * cov_xy
    g[ss_x < TOL] = 0
    vv[ss_x < TOL] = ss_y[ss_x < TOL]
    g[ss_y < TOL] = 0
    vv[ss_y < TOL] = 0

    nblv, nblh = bh - (BLK - 1), bw - (BLK - 1)
    cols = []
    for dy in range(BLK):
        for dx in range(BLK):
            cols.append(band_r[dy : dy + nblv, dx : dx + nblh].reshape(-1))
    if parent_r is not None:
        up = imenlarge2(parent_r)[:bh, :bw]
        cols.append(up[1 : 1 + nblv, 1 : 1 + nblh].reshape(-1))
    Y = np.stack(cols, axis=1)  # (nexp, N)
    N = Y.shape[1]

    cu = Y.T @ Y / Y.shape[0]
    evals, evec = np.linalg.eigh(cu)
    pos = np.maximum(evals, 0.0)
    sum_pos = pos.sum()
    scale = evals.sum() / (sum_pos if sum_pos > 0 else 1.0)
    lam = pos * scale
    inv_lam = np.where(lam > 0, 1.0 / lam, 0.0)
    # ss = (1/N) y' inv(Cu) y, with inv(Cu) = V diag(1/lam) V'
    proj = Y @ evec  # (nexp, N)
    ss = (proj * proj * inv_lam).sum(axis=1) / N
    ss = ss.reshape(nblv, nblh)

    gc = g[1 : 1 + nblv, 1 : 1 + nblh]
    vvc = vv[1 : 1 + nblv, 1 : 1 + nblh]
    sigma_sq = SIGMA_NSQ * SIGMA_NSQ
    infow = np.zeros((nblv, nblh))
    for lj in lam:
        numer = (vvc + (1.0 + gc * gc) * SIGMA_NSQ) * ss * lj + SIGMA_NSQ * vvc
        infow += np.log2(1.0 + numer / sigma_sq)
    infow[infow < TOL] = 0.0
    return infow


def iwssim(a, b):
    win = gaussian_window()
    weight = np.array(WEIGHTS[:NSC])
    weight = weight / weight.sum()
    pyr_r = pt.pyramids.LaplacianPyramid(a, height=NSC)
    pyr_d = pt.pyramids.LaplacianPyramid(b, height=NSC)
    bands_r = [pyr_r.pyr_coeffs[(s, 0)] for s in range(NSC)]
    bands_d = [pyr_d.pyr_coeffs[(s, 0)] for s in range(NSC)]

    bound = (WINDOW - 1) // 2  # 5
    bound1 = bound - (BLK - 1) // 2  # 4
    wmcs = np.zeros(NSC)
    for s in range(NSC):
        cs, luminance = scale_quality_maps(bands_r[s], bands_d[s], win)
        if s == NSC - 1:
            term = cs * luminance
            wmcs[s] = term.mean()
        else:
            parent = bands_r[s + 1] if (PARENT and s + 1 < NSC - 1) else None
            iw = info_content_weight_map(bands_r[s], bands_d[s], parent)
            iw = iw[bound1:-bound1, bound1:-bound1]
            wmcs[s] = (cs * iw).sum() / iw.sum()
    return float(np.prod(wmcs**weight))


def synth_band(h, w, a, b, c, d):
    """A deterministic synthetic band `a·sin(b·x+0.3y)·cos(c·y−0.2x) +
    d·sin(0.17·(x²−y))`, mirrored exactly by `synth_band` in src/iw_ssim.rs."""
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float64)
    return a * np.sin(b * xs + 0.3 * ys) * np.cos(c * ys - 0.2 * xs) + d * np.sin(
        0.17 * (xs * xs - ys)
    )


def print_icw_goldens():
    """Emit the `info_content_weight_map` unit-test goldens for src/iw_ssim.rs.

    The end-to-end IW-SSIM score is nearly insensitive to the information weights
    (they only renormalize an already near-uniform cs map, and the pool is
    invariant to globally scaling them), so they cannot be pinned through the
    score. The src unit test pins the map directly against these values. The bands
    are chosen so the GSM covariance is cleanly positive-definite, so this
    reference and an independent eigensolver agree to far tighter than the test's
    tolerance. `sigma_nsq` is the 8-bit default 0.4 (the module `SIGMA_NSQ`)."""
    band_r = synth_band(12, 12, 30.0, 0.40, 0.30, 9.0)
    band_d = band_r + 0.6 * synth_band(12, 12, 5.0, 0.70, 0.50, 2.0)
    parent = synth_band(6, 6, 22.0, 0.55, 0.25, 6.0)
    print("\n// --- info_content_weight_map goldens (paste into src/iw_ssim.rs) ---")
    for label, par in [("IW_NO_PARENT", None), ("IW_WITH_PARENT", parent)]:
        flat = info_content_weight_map(band_r, band_d, par).reshape(-1)
        print(f"        const {label}: [f64; {flat.size}] = [")
        for i in range(0, flat.size, 5):
            print("            " + ", ".join(f"{v:.9f}" for v in flat[i : i + 5]) + ",")
        print("        ];")


def main():
    print("// Independent NumPy/pyrtools IW-SSIM reference, scripts/gen-iwssim-goldens.py.")
    print("const GOLDENS: &[(&str, f64)] = &[")
    for label, ref, dist in CASES:
        a = read_pgm(os.path.join(FIXTURE_DIR, ref))
        b = read_pgm(os.path.join(FIXTURE_DIR, dist))
        print(f'    ("{label}", {iwssim(a, b):.6f}),')
    print("];")
    print_icw_goldens()


if __name__ == "__main__":
    sys.exit(main())
