//! CIEDE2000 (ΔE₀₀) color difference — a native implementation.
//!
//! CIEDE2000 is the CIE's perceptual color-difference formula (Sharma, Wu &
//! Dalal 2005; CIE 142-2001). It is defined on pairs of CIELAB colors. This
//! metric converts each pixel from sRGB (or grayscale) to CIELAB under the D65
//! white point, computes the per-pixel ΔE₀₀ with the reference parametric
//! factors `kL = kC = kH = 1`, and pools the per-pixel differences by their
//! arithmetic mean. The result is a distance: identical images score `0.0` and
//! larger values mean a larger color difference, so **lower is better**.
//!
//! Grayscale input is treated as a neutral color (`a* = b* = 0`), so its score
//! reduces to a lightness difference. An alpha channel, if present, is ignored.
//!
//! The formula and every edge case (the achromatic hue branch, the hue-angle
//! and mean-hue wraps, the signed hue difference, and the blue-region rotation
//! term) follow `references/CIEDE2000.md`. The implementation is verified two
//! ways: the Sharma–Wu–Dalal supplemental test set is reproduced to four
//! decimals by the inline tests, and the full sRGB-image pipeline is
//! cross-validated against the `colour-science` reference in
//! `tests/ciede2000_reference.rs`.

use crate::error::{Error, Result};
use crate::format::{PixelFormat, Sample};
use crate::image::{Channels, Image};

// --- CIEDE2000 constants (references/CIEDE2000.md §0, §1, §4) ---------------

/// Lightness parametric factor under CIE reference viewing conditions.
const K_L: f64 = 1.0;
/// Chroma parametric factor under CIE reference viewing conditions.
const K_C: f64 = 1.0;
/// Hue parametric factor under CIE reference viewing conditions.
const K_H: f64 = 1.0;

/// `25_f64.powi(7)`, the constant inside `G` (§1.3) and `R_C` (§4.3). `25^7` is
/// `6_103_515_625`, exactly representable in `f64`. Spelled as a literal because
/// `powi` is not a `const fn`.
const POW25_7: f64 = 6_103_515_625.0;

// --- sRGB electro-optical transfer function (IEC 61966-2-1) -----------------

const SRGB_EOTF_THRESHOLD: f64 = 0.04045;
const SRGB_EOTF_SLOPE: f64 = 12.92;
const SRGB_EOTF_OFFSET: f64 = 0.055;
const SRGB_EOTF_SCALE: f64 = 1.055;
const SRGB_EOTF_GAMMA: f64 = 2.4;

// --- linear sRGB → CIE XYZ and CIELAB (D65) ---------------------------------

/// Linear sRGB (D65) → CIE XYZ. Rows yield `X, Y, Z` from `(R, G, B)`; the
/// high-precision sRGB matrix (its row sums are the D65 white below).
const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192_0, 0.950_304_1],
];

/// CIE D65 reference white `(Xn, Yn, Zn)`, the white CIELAB is computed against.
const D65_XN: f64 = 0.950_47;
const D65_YN: f64 = 1.0;
const D65_ZN: f64 = 1.088_83;

/// `δ = 6/29`, the CIELAB `f(t)` breakpoint parameter.
const LAB_DELTA: f64 = 6.0 / 29.0;
/// `δ³`: below this `t`, `f(t)` uses its linear branch.
const LAB_T_THRESHOLD: f64 = LAB_DELTA * LAB_DELTA * LAB_DELTA;
/// `1 / (3 δ²)`: slope of the linear branch (equals the spec's rounded `7.787`).
const LAB_LINEAR_SLOPE: f64 = 1.0 / (3.0 * LAB_DELTA * LAB_DELTA);
/// `4/29`: intercept of the linear branch (equals the spec's `16/116`).
const LAB_LINEAR_OFFSET: f64 = 4.0 / 29.0;

/// A CIELAB triple `(L*, a*, b*)` under some white point.
#[derive(Debug, Clone, Copy)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

/// Adjusted hue angle in degrees, mapped to `[0, 360)` (§1.6). The hue is
/// **defined to be 0** when `a' = b' = 0` (edge case 1), since `atan2(0, 0)` is
/// implementation-defined.
fn hue_deg(a_prime: f64, b: f64) -> f64 {
    if a_prime == 0.0 && b == 0.0 {
        0.0
    } else {
        let mut h = b.atan2(a_prime).to_degrees();
        if h < 0.0 {
            h += 360.0;
        }
        h
    }
}

/// CIEDE2000 color difference ΔE₀₀ between two CIELAB samples
/// (`kL = kC = kH = 1`).
///
/// The combination in §5 is written so the result is **bit-identical under
/// input swap**: the lightness/chroma/hue terms are squared (never `abs`-ed) so
/// their sign drops out, the rotation term is a single three-factor product so
/// the two sign flips cancel exactly, and every mean-derived weight is itself
/// swap-invariant. The property suite checks this with a `to_bits()` comparison.
fn delta_e_2000(lab1: Lab, lab2: Lab) -> f64 {
    let Lab {
        l: l1,
        a: a1,
        b: b1,
    } = lab1;
    let Lab {
        l: l2,
        a: a2,
        b: b2,
    } = lab2;

    // §1 — adjusted chroma C'_i and adjusted hue h'_i.
    let c1_ab = (a1 * a1 + b1 * b1).sqrt();
    let c2_ab = (a2 * a2 + b2 * b2).sqrt();
    let c_bar_ab = (c1_ab + c2_ab) / 2.0;
    let c_bar_ab7 = c_bar_ab.powi(7);
    let g = 0.5 * (1.0 - (c_bar_ab7 / (c_bar_ab7 + POW25_7)).sqrt());

    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = (a1p * a1p + b1 * b1).sqrt();
    let c2p = (a2p * a2p + b2 * b2).sqrt();
    let h1p = hue_deg(a1p, b1);
    let h2p = hue_deg(a2p, b2);

    // §2 — signed differences ΔL', ΔC', Δh', ΔH'.
    let delta_l = l2 - l1;
    let delta_c = c2p - c1p;
    let c1c2 = c1p * c2p; // achromatic guard: zero when either sample is neutral.

    let dh = h2p - h1p;
    let delta_h_small = if c1c2 == 0.0 {
        0.0
    } else if dh.abs() <= 180.0 {
        dh
    } else if dh > 180.0 {
        dh - 360.0
    } else {
        dh + 360.0
    };
    let delta_h_big = 2.0 * c1c2.sqrt() * (delta_h_small / 2.0).to_radians().sin();

    // §3 — means L̄', C̄', h̄'.
    let l_bar = (l1 + l2) / 2.0;
    let c_bar = (c1p + c2p) / 2.0;

    let h_sum = h1p + h2p;
    let h_diff_abs = (h1p - h2p).abs();
    let h_bar = if c1c2 == 0.0 {
        h_sum // achromatic: pass the meaningful hue through (not s/2).
    } else if h_diff_abs <= 180.0 {
        h_sum / 2.0
    } else if h_sum < 360.0 {
        (h_sum + 360.0) / 2.0
    } else {
        (h_sum - 360.0) / 2.0
    };

    // §4 — weighting functions (every trig argument is in degrees).
    let t = 1.0 - 0.17 * (h_bar - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar).to_radians().cos()
        + 0.32 * (3.0 * h_bar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar - 63.0).to_radians().cos();

    let delta_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let c_bar7 = c_bar.powi(7);
    let r_c = 2.0 * (c_bar7 / (c_bar7 + POW25_7)).sqrt();
    let s_l = 1.0 + (0.015 * (l_bar - 50.0).powi(2)) / (20.0 + (l_bar - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * c_bar;
    let s_h = 1.0 + 0.015 * c_bar * t;
    let r_t = -((2.0 * delta_theta).to_radians().sin()) * r_c;

    // §5 — ΔE₀₀.
    let term_l = delta_l / (K_L * s_l);
    let term_c = delta_c / (K_C * s_c);
    let term_h = delta_h_big / (K_H * s_h);
    (term_l * term_l + term_c * term_c + term_h * term_h + r_t * term_c * term_h).sqrt()
}

/// sRGB EOTF: a nonlinear sRGB channel in `[0, 1]` → linear light in `[0, 1]`.
fn srgb_eotf(c: f64) -> f64 {
    if c <= SRGB_EOTF_THRESHOLD {
        c / SRGB_EOTF_SLOPE
    } else {
        ((c + SRGB_EOTF_OFFSET) / SRGB_EOTF_SCALE).powf(SRGB_EOTF_GAMMA)
    }
}

/// The CIELAB nonlinearity `f(t)`.
fn lab_f(t: f64) -> f64 {
    if t > LAB_T_THRESHOLD {
        t.cbrt()
    } else {
        LAB_LINEAR_SLOPE * t + LAB_LINEAR_OFFSET
    }
}

/// Linear-light, normalized `[0, 1]` RGB → CIELAB under D65.
fn linear_rgb_to_lab(r: f64, g: f64, b: f64) -> Lab {
    let x = SRGB_TO_XYZ[0][0] * r + SRGB_TO_XYZ[0][1] * g + SRGB_TO_XYZ[0][2] * b;
    let y = SRGB_TO_XYZ[1][0] * r + SRGB_TO_XYZ[1][1] * g + SRGB_TO_XYZ[1][2] * b;
    let z = SRGB_TO_XYZ[2][0] * r + SRGB_TO_XYZ[2][1] * g + SRGB_TO_XYZ[2][2] * b;
    let fx = lab_f(x / D65_XN);
    let fy = lab_f(y / D65_YN);
    let fz = lab_f(z / D65_ZN);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIELAB of the pixel at `(x, y)`: sample → normalize by `Sample::MAX` → sRGB
/// EOTF → XYZ → Lab. Grayscale replicates its one channel to `R = G = B` (the
/// neutral axis); an alpha channel is dropped.
fn lab_at<F: PixelFormat>(img: &Image<F>, x: u32, y: u32) -> Lab {
    let max = <F::Sample as Sample>::MAX;
    let (r, g, b) = match F::CHANNELS {
        Channels::Gray => {
            let v = img.sample_at(x, y, 0) / max;
            (v, v, v)
        }
        Channels::Rgb | Channels::Rgba => (
            img.sample_at(x, y, 0) / max,
            img.sample_at(x, y, 1) / max,
            img.sample_at(x, y, 2) / max,
        ),
    };
    linear_rgb_to_lab(srgb_eotf(r), srgb_eotf(g), srgb_eotf(b))
}

/// Options controlling a CIEDE2000 computation.
///
/// Per-pixel ΔE₀₀ values are pooled by their arithmetic mean. This struct
/// currently exposes no tunable fields; it exists so options can be added later
/// without a breaking change.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ciede2000Options {}

/// Computes the mean CIEDE2000 (ΔE₀₀) color difference between `reference` and
/// `distorted`.
///
/// Each pixel is converted from sRGB (grayscale is treated as neutral) to
/// CIELAB under D65, the per-pixel ΔE₀₀ is computed with `kL = kC = kH = 1`, and
/// the per-pixel values are averaged. The score is a distance: identical images
/// yield `0.0`, and larger values mean a larger color difference. An alpha
/// channel, if present, is ignored. The metric has no minimum size.
///
/// Both images share the pixel format `F`, so a difference in color space,
/// channel layout, or bit depth is rejected by the compiler rather than at run
/// time.
///
/// # Errors
///
/// Returns [`Error::DimensionMismatch`] if the images differ in size.
///
/// # Examples
///
/// ```
/// use iqa::{Image, Ciede2000Options, ciede2000};
///
/// let reference = Image::srgb8(2, 2, vec![10, 20, 30, 10, 20, 30, 10, 20, 30, 10, 20, 30])?;
/// let distorted = Image::srgb8(2, 2, vec![12, 22, 33, 12, 22, 33, 12, 22, 33, 12, 22, 33])?;
/// let score = ciede2000(&reference, &distorted, Ciede2000Options::default())?;
/// assert!(score >= 0.0);
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn ciede2000<F: PixelFormat>(
    reference: &Image<F>,
    distorted: &Image<F>,
    _opts: Ciede2000Options,
) -> Result<f64> {
    if reference.dimensions() != distorted.dimensions() {
        return Err(Error::DimensionMismatch {
            a: reference.dimensions(),
            b: distorted.dimensions(),
        });
    }

    let mut sum = 0.0;
    let mut count = 0usize;
    for y in 0..reference.height() {
        for x in 0..reference.width() {
            sum += delta_e_2000(lab_at(reference, x, y), lab_at(distorted, x, y));
            count += 1;
        }
    }
    Ok(if count == 0 { 0.0 } else { sum / count as f64 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Image;

    /// The complete 34-pair Sharma–Wu–Dalal supplemental set (Table I), the
    /// definitive CIEDE2000 reference: `(L1, a1, b1, L2, a2, b2, ΔE₀₀)`. The set
    /// is constructed to exercise every branch of the formula — the blue/R_T
    /// region (1–6), the achromatic hue branch (7–8), the near-180° ΔC'/Δh' sign
    /// cases (9–16), large differences (17–20), small differences (21–24), the
    /// CIE-published colors (25–34), and the dark/near-neutral pairs (33–34).
    /// Reproducing all 34 to four decimals is the canonical correctness bar [S3].
    const SHARMA: &[(f64, f64, f64, f64, f64, f64, f64)] = &[
        (50.0, 2.6772, -79.7751, 50.0, 0.0, -82.7485, 2.0425),
        (50.0, 3.1571, -77.2803, 50.0, 0.0, -82.7485, 2.8615),
        (50.0, 2.8361, -74.02, 50.0, 0.0, -82.7485, 3.4412),
        (50.0, -1.3802, -84.2814, 50.0, 0.0, -82.7485, 1.0000),
        (50.0, -1.1848, -84.8006, 50.0, 0.0, -82.7485, 1.0000),
        (50.0, -0.9009, -85.5211, 50.0, 0.0, -82.7485, 1.0000),
        (50.0, 0.0, 0.0, 50.0, -1.0, 2.0, 2.3669),
        (50.0, -1.0, 2.0, 50.0, 0.0, 0.0, 2.3669),
        (50.0, 2.49, -0.001, 50.0, -2.49, 0.0009, 7.1792),
        (50.0, 2.49, -0.001, 50.0, -2.49, 0.001, 7.1792),
        (50.0, 2.49, -0.001, 50.0, -2.49, 0.0011, 7.2195),
        (50.0, 2.49, -0.001, 50.0, -2.49, 0.0012, 7.2195),
        (50.0, -0.001, 2.49, 50.0, 0.0009, -2.49, 4.8045),
        (50.0, -0.001, 2.49, 50.0, 0.001, -2.49, 4.8045),
        (50.0, -0.001, 2.49, 50.0, 0.0011, -2.49, 4.7461),
        (50.0, 2.5, 0.0, 50.0, 0.0, -2.5, 4.3065),
        (50.0, 2.5, 0.0, 73.0, 25.0, -18.0, 27.1492),
        (50.0, 2.5, 0.0, 61.0, -5.0, 29.0, 22.8977),
        (50.0, 2.5, 0.0, 56.0, -27.0, -3.0, 31.9030),
        (50.0, 2.5, 0.0, 58.0, 24.0, 15.0, 19.4535),
        (50.0, 2.5, 0.0, 50.0, 3.1736, 0.5854, 1.0000),
        (50.0, 2.5, 0.0, 50.0, 3.2972, 0.0, 1.0000),
        (50.0, 2.5, 0.0, 50.0, 1.8634, 0.5757, 1.0000),
        (50.0, 2.5, 0.0, 50.0, 3.2592, 0.335, 1.0000),
        (
            60.2574, -34.0099, 36.2677, 60.4626, -34.1751, 39.4387, 1.2644,
        ),
        (
            63.0109, -31.0961, -5.8663, 62.8187, -29.7946, -4.0864, 1.2630,
        ),
        (61.2901, 3.7196, -5.3901, 61.4292, 2.248, -4.962, 1.8731),
        (35.0831, -44.1164, 3.7933, 35.0232, -40.0716, 1.5901, 1.8645),
        (22.7233, 20.0904, -46.694, 23.0331, 14.973, -42.5619, 2.0373),
        (36.4612, 47.858, 18.3852, 36.2715, 50.5065, 21.2231, 1.4146),
        (90.8027, -2.0831, 1.441, 91.1528, -1.6435, 0.0447, 1.4441),
        (90.9257, -0.5406, -0.9208, 88.6381, -0.8985, -0.7239, 1.5381),
        (6.7747, -0.2908, -2.4247, 5.8714, -0.0985, -2.2286, 0.6377),
        (2.0776, 0.0795, -1.1350, 0.9033, -0.0636, -0.5514, 0.9082),
    ];

    fn lab(l: f64, a: f64, b: f64) -> Lab {
        Lab { l, a, b }
    }

    #[test]
    fn matches_sharma_table() {
        for &(l1, a1, b1, l2, a2, b2, expected) in SHARMA {
            let got = delta_e_2000(lab(l1, a1, b1), lab(l2, a2, b2));
            assert!(
                (got - expected).abs() <= 5e-4,
                "ΔE00({l1}, {a1}, {b1} | {l2}, {a2}, {b2}) = {got:.6}, expected {expected:.4}",
            );
        }
    }

    #[test]
    fn delta_e_is_bit_exact_under_swap() {
        for &(l1, a1, b1, l2, a2, b2, _) in SHARMA {
            let p = lab(l1, a1, b1);
            let q = lab(l2, a2, b2);
            assert_eq!(
                delta_e_2000(p, q).to_bits(),
                delta_e_2000(q, p).to_bits(),
                "asymmetric on ({l1}, {a1}, {b1} | {l2}, {a2}, {b2})",
            );
        }
        // An achromatic pair (a* = b* = 0 → C1'C2' = 0): the grayscale branch.
        let g1 = lab(50.0, 0.0, 0.0);
        let g2 = lab(60.0, 0.0, 0.0);
        assert_eq!(
            delta_e_2000(g1, g2).to_bits(),
            delta_e_2000(g2, g1).to_bits()
        );
    }

    #[test]
    fn srgb_to_lab_landmarks() {
        // Published CIELAB (D65) values for sRGB landmarks. The tolerance absorbs
        // inter-source rounding of the matrix/white point; the rigorous pipeline
        // check is tests/ciede2000_reference.rs (vs colour-science).
        let cases = [
            ((255.0, 255.0, 255.0), (100.0, 0.0, 0.0)),
            ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
            ((128.0, 128.0, 128.0), (53.585, 0.0, 0.0)),
            ((255.0, 0.0, 0.0), (53.2408, 80.0925, 67.2032)),
            ((0.0, 255.0, 0.0), (87.7347, -86.1827, 83.1793)),
            ((0.0, 0.0, 255.0), (32.2970, 79.1875, -107.8602)),
        ];
        for ((r, g, b), (el, ea, eb)) in cases {
            let got = linear_rgb_to_lab(
                srgb_eotf(r / 255.0),
                srgb_eotf(g / 255.0),
                srgb_eotf(b / 255.0),
            );
            assert!(
                (got.l - el).abs() < 0.1 && (got.a - ea).abs() < 0.1 && (got.b - eb).abs() < 0.1,
                "sRGB({r}, {g}, {b}) → Lab({:.4}, {:.4}, {:.4}), expected ({el}, {ea}, {eb})",
                got.l,
                got.a,
                got.b,
            );
        }
    }

    #[test]
    fn solid_color_image_end_to_end() {
        // A whole-image pipeline check with no external dependency: every pixel
        // is the same red / green, so the mean ΔE₀₀ equals the single-pair ΔE₀₀
        // of those colors' published Labs (which srgb_to_lab_landmarks pins).
        let red = Image::srgb8(8, 8, [255, 0, 0].repeat(64)).unwrap();
        let green = Image::srgb8(8, 8, [0, 255, 0].repeat(64)).unwrap();
        let expected = delta_e_2000(
            lab(53.2408, 80.0925, 67.2032),
            lab(87.7347, -86.1827, 83.1793),
        );
        let got = ciede2000(&red, &green, Ciede2000Options::default()).unwrap();
        assert!(
            (got - expected).abs() < 0.5,
            "mean ΔE00 = {got:.4}, expected ≈ {expected:.4}",
        );
    }

    #[test]
    fn identical_image_is_zero() {
        let img = Image::srgb8(4, 4, vec![123; 48]).unwrap();
        let score = ciede2000(&img, &img, Ciede2000Options::default()).unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn grayscale_difference_is_lightness_only() {
        // Two uniform grays differ only in L*, so ΔE₀₀ is finite, positive, and
        // exercises the achromatic (C1'C2' = 0) branch on every pixel.
        let dark = Image::gray8(4, 4, vec![80; 16]).unwrap();
        let light = Image::gray8(4, 4, vec![160; 16]).unwrap();
        let score = ciede2000(&dark, &light, Ciede2000Options::default()).unwrap();
        assert!(score.is_finite() && score > 0.0, "got {score}");
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = Image::srgb8(2, 2, vec![0; 12]).unwrap();
        let b = Image::srgb8(1, 1, vec![0; 3]).unwrap();
        let err = ciede2000(&a, &b, Ciede2000Options::default()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }
}
