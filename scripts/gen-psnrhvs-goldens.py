#!/usr/bin/env python3
#
# gen-psnrhvs-goldens.py — a no-dependency cross-check of the PSNR-HVS-M goldens.
#
# The AUTHORITATIVE goldens come from `scripts/gen-psnrhvs-goldens.sh`, which runs
# the original reference `psnrhvsm.m` (Ponomarenko et al. 2007) under Octave. This
# script is an INDEPENDENT NumPy reimplementation of that same algorithm (its
# CSFCof/MaskCof tables, its maskeff with the N-1 sample variance MATLAB's `var`
# uses, its DC-exempt masking threshold, and the 255^2/MSE dB conversion), written
# separately from both the Rust and the .m. It exists so the goldens can be
# re-derived without Octave and so a third implementation has to agree — it
# reproduces the reference values exactly.
#
# It reads the committed grayscale PGM fixtures and prints the Rust `GOLDENS`
# table to paste into tests/psnr_hvs_m_reference.rs.
#
#   python3 scripts/gen-psnrhvs-goldens.py
#
# Regenerate the fixtures first if they changed:
#   cargo test --features psnr-hvs-m --test psnr_hvs_m_reference \
#       write_psnr_hvs_m_fixtures -- --ignored

import os
import sys

import numpy as np

BLOCK = 8
STEP = 8

# CSFCof and MaskCof from the reference psnrhvsm.m, [row][col].
CSF_COF = np.array(
    [
        [1.608443, 2.339554, 2.573509, 1.608443, 1.072295, 0.643377, 0.504610, 0.421887],
        [2.144591, 2.144591, 1.838221, 1.354478, 0.989811, 0.443708, 0.428918, 0.467911],
        [1.838221, 1.979622, 1.608443, 1.072295, 0.643377, 0.451493, 0.372972, 0.459555],
        [1.838221, 1.513829, 1.169777, 0.887417, 0.504610, 0.295806, 0.321689, 0.415082],
        [1.429727, 1.169777, 0.695543, 0.459555, 0.378457, 0.236102, 0.249855, 0.334222],
        [1.072295, 0.735288, 0.467911, 0.402111, 0.317717, 0.247453, 0.227744, 0.279729],
        [0.525206, 0.402111, 0.329937, 0.295806, 0.249855, 0.212687, 0.214459, 0.254803],
        [0.357432, 0.279729, 0.270896, 0.262603, 0.229778, 0.257351, 0.249855, 0.259950],
    ]
)
MASK_COF = np.array(
    [
        [0.390625, 0.826446, 1.000000, 0.390625, 0.173611, 0.062500, 0.038447, 0.026874],
        [0.694444, 0.694444, 0.510204, 0.277008, 0.147929, 0.029727, 0.027778, 0.033058],
        [0.510204, 0.591716, 0.390625, 0.173611, 0.062500, 0.030779, 0.021004, 0.031888],
        [0.510204, 0.346021, 0.206612, 0.118906, 0.038447, 0.013212, 0.015625, 0.026015],
        [0.308642, 0.206612, 0.073046, 0.031888, 0.021626, 0.008417, 0.009426, 0.016866],
        [0.173611, 0.081633, 0.033058, 0.024414, 0.015242, 0.009246, 0.007831, 0.011815],
        [0.041649, 0.024414, 0.016437, 0.013212, 0.009426, 0.006830, 0.006944, 0.009803],
        [0.019290, 0.011815, 0.011080, 0.010412, 0.007972, 0.010000, 0.009426, 0.010203],
    ]
)

FIXTURE_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "tests",
    "fixtures",
    "psnr_hvs_m",
)

CASES = [
    ("gradient_lo", "gradient_ref.pgm", "gradient_lo.pgm"),
    ("gradient_hi", "gradient_ref.pgm", "gradient_hi.pgm"),
    ("solid_dist", "solid_ref.pgm", "solid_dist.pgm"),
    ("texture_dist", "texture_ref.pgm", "texture_dist.pgm"),
]


def read_pgm(path):
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
    idx += 1
    buf = np.frombuffer(data[idx : idx + w * h], dtype=np.uint8).astype(np.float64)
    return buf.reshape(h, w)


def dct_basis():
    """8x8 orthonormal DCT-II basis C, so dct2(block) = C @ block @ C.T."""
    n = BLOCK
    k = np.arange(n).reshape(n, 1)
    x = np.arange(n).reshape(1, n)
    c = np.cos(np.pi * (2 * x + 1) * k / (2 * n))
    alpha = np.full((n, 1), np.sqrt(2.0 / n))
    alpha[0, 0] = np.sqrt(1.0 / n)
    return alpha * c


C = dct_basis()


def dct2(block):
    return C @ block @ C.T


def vari(arr):
    # var with the N-1 (sample) denominator, times N — exactly MATLAB var()*len.
    flat = arr.ravel()
    return float(np.var(flat, ddof=1) * flat.size)


def maskeff(z, zdct):
    m = float(np.sum((zdct**2) * MASK_COF)) - (zdct[0, 0] ** 2) * MASK_COF[0, 0]
    pop = vari(z)
    if pop != 0.0:
        quads = vari(z[0:4, 0:4]) + vari(z[0:4, 4:8]) + vari(z[4:8, 4:8]) + vari(z[4:8, 0:4])
        pop = quads / pop
    return np.sqrt(m * pop) / 32.0


def psnr_hvs_m(a, b):
    h, w = a.shape
    s1 = 0.0
    num = 0
    y = 0
    while y + BLOCK <= h:
        x = 0
        while x + BLOCK <= w:
            ablk = a[y : y + BLOCK, x : x + BLOCK]
            bblk = b[y : y + BLOCK, x : x + BLOCK]
            adct = dct2(ablk)
            bdct = dct2(bblk)
            mask = max(maskeff(ablk, adct), maskeff(bblk, bdct))
            for k in range(BLOCK):
                for l in range(BLOCK):
                    u = abs(adct[k, l] - bdct[k, l])
                    if k != 0 or l != 0:
                        thr = mask / MASK_COF[k, l]
                        u = 0.0 if u < thr else u - thr
                    e = u * CSF_COF[k, l]
                    s1 += e * e
                    num += 1
            x += STEP
        y += STEP
    mse = s1 / num
    return 1e9 if mse == 0.0 else 10.0 * np.log10(255.0 * 255.0 / mse)


def main():
    print('// Independent NumPy psnrhvsm.m reference, scripts/gen-psnrhvs-goldens.py.')
    print("const GOLDENS: &[(&str, f64)] = &[")
    for label, ref, dist in CASES:
        a = read_pgm(os.path.join(FIXTURE_DIR, ref))
        b = read_pgm(os.path.join(FIXTURE_DIR, dist))
        print(f'    ("{label}", {psnr_hvs_m(a, b):.6f}),')
    print("];")


if __name__ == "__main__":
    sys.exit(main())
