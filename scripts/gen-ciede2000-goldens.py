#!/usr/bin/env python3
#
# gen-ciede2000-goldens.py — authoritative cross-check of the CIEDE2000 goldens.
#
# Computes the mean CIEDE2000 (ΔE₀₀) over the committed sRGB PPM fixtures using
# colour-science (https://www.colour-science.org/), the de-facto reference for
# color science in Python. The pipeline mirrors the Rust crate exactly:
#
#   RGB  = uint8 / 255.0                         # normalize
#   XYZ  = colour.sRGB_to_XYZ(RGB)               # sRGB EOTF + sRGB→XYZ (D65)
#   Lab  = colour.XYZ_to_Lab(XYZ)                # XYZ→CIELAB (D65)
#   dE   = colour.difference.delta_E_CIE2000(Lab_ref, Lab_dist)   # per pixel
#   golden = dE.mean()
#
# colour's sRGB colourspace is natively D65 and XYZ_to_Lab defaults to the same
# D65 white, so no chromatic adaptation is applied — exactly like the crate. The
# only differences from the crate are 4th–5th-digit: colour derives its sRGB→XYZ
# matrix and white point from the primaries/chromaticities to full precision,
# whereas the crate uses the standard rounded matrix and D65 = (0.95047, 1.0,
# 1.08883). That shifts the mean ΔE₀₀ far below the test's 1% tolerance.
#
# Reads the committed binary (P6) PPM fixtures and prints the Rust `GOLDENS`
# table to paste into tests/ciede2000_reference.rs. The fixtures are the single
# source of truth: this script and the Rust test read the identical .ppm files.
#
#   pip install "colour-science>=0.4.4" numpy
#   python3 scripts/gen-ciede2000-goldens.py
#
# Regenerate the fixtures first if they changed:
#   cargo test --features ciede2000 --test ciede2000_reference \
#       write_ciede2000_fixtures -- --ignored

import os
import sys

import numpy as np
import colour

FIXTURE_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "tests",
    "fixtures",
    "ciede2000",
)

# (case label, reference fixture, distorted fixture) — keep in lockstep with the
# CASES table in tests/ciede2000_reference.rs.
CASES = [
    ("gradient_lo", "gradient_ref.ppm", "gradient_lo.ppm"),
    ("gradient_hi", "gradient_ref.ppm", "gradient_hi.ppm"),
    ("solid_dist", "solid_ref.ppm", "solid_dist.ppm"),
    ("chroma_dist", "chroma_ref.ppm", "chroma_dist.ppm"),
]


def read_ppm(path):
    """Reads a binary (P6) PPM into an (h, w, 3) float64 array in 0..255."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] != b"P6":
        raise ValueError(f"{path}: not a binary PPM")
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
    buf = np.frombuffer(data[idx : idx + w * h * 3], dtype=np.uint8).astype(np.float64)
    return buf.reshape(h, w, 3)


def mean_ciede2000(ref, dist):
    """Mean ΔE₀₀ between two (h, w, 3) sRGB arrays in 0..255, via colour-science."""
    lab_ref = colour.XYZ_to_Lab(colour.sRGB_to_XYZ(ref / 255.0))
    lab_dist = colour.XYZ_to_Lab(colour.sRGB_to_XYZ(dist / 255.0))
    return float(colour.difference.delta_E_CIE2000(lab_ref, lab_dist).mean())


def main():
    print("// colour-science CIEDE2000 reference, scripts/gen-ciede2000-goldens.py.")
    print("const GOLDENS: &[(&str, f64)] = &[")
    for label, ref, dist in CASES:
        a = read_ppm(os.path.join(FIXTURE_DIR, ref))
        b = read_ppm(os.path.join(FIXTURE_DIR, dist))
        print(f'    ("{label}", {mean_ciede2000(a, b):.6f}),')
    print("];")


if __name__ == "__main__":
    sys.exit(main())
