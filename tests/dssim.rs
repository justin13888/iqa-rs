//! DSSIM-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to DSSIM — that it is exactly `(1 - SSIM) / 2`,
//! its perfect score of `0.0`, closed-form values for uniform and black/white
//! inputs, the full `(-1, 1] -> [0, 1)` range under anti-correlation, the
//! difference between its two modes, bit-depth consistency, and the 11x11
//! minimum. The whole file compiles to nothing unless the `dssim` feature is
//! enabled.
#![cfg(feature = "dssim")]

mod common;

use common::*;
use iqa::{DssimOptions, Error, SsimMode, SsimOptions, dssim, ssim};

const RGB_AVERAGED: DssimOptions = DssimOptions {
    mode: SsimMode::RgbAveraged,
};
const LUMA709: DssimOptions = DssimOptions {
    mode: SsimMode::Luma709,
};

fn assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "{context}: got {actual}, expected {expected}",
    );
}

#[test]
fn identical_images_score_exactly_zero() {
    // SSIM is exactly 1.0 for identical input (its deviation-form keeps the
    // variance terms at zero even at 16-bit), so the transform yields a
    // bit-exact 0.0 — not merely something within tolerance.
    let rgb8 = Srgb8::base(16, 16);
    let rgb16 = Srgb16::base(16, 16);
    let gray8 = Gray8::base(16, 16);
    let gray16 = Gray16::base(16, 16);
    let rgba8 = Rgba8::base(16, 16);
    let rgba16 = Rgba16::base(16, 16);
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_eq!(dssim(&rgb8, &rgb8, opts).unwrap(), 0.0, "srgb8 {opts:?}");
        assert_eq!(dssim(&rgb16, &rgb16, opts).unwrap(), 0.0, "srgb16 {opts:?}");
        assert_eq!(dssim(&gray8, &gray8, opts).unwrap(), 0.0, "gray8 {opts:?}");
        assert_eq!(
            dssim(&gray16, &gray16, opts).unwrap(),
            0.0,
            "gray16 {opts:?}"
        );
        assert_eq!(dssim(&rgba8, &rgba8, opts).unwrap(), 0.0, "rgba8 {opts:?}");
        assert_eq!(
            dssim(&rgba16, &rgba16, opts).unwrap(),
            0.0,
            "rgba16 {opts:?}"
        );
    }
}

#[test]
fn is_exactly_one_minus_ssim_over_two() {
    // The whole metric is a pure transform of SSIM and nothing else. Pin that,
    // bit-for-bit, across every format, both modes, and several distortions —
    // this is what ties DSSIM to SSIM's own (separately tested) output.
    fn check<F: Fixture>(label: &str) {
        let base = F::base(24, 24);
        for amp in [3.0, 15.0, 40.0] {
            let distorted = F::distort(&base, amp);
            for (opts, smode) in [
                (RGB_AVERAGED, SsimMode::RgbAveraged),
                (LUMA709, SsimMode::Luma709),
            ] {
                let d = dssim(&base, &distorted, opts).unwrap();
                let s = ssim(&base, &distorted, SsimOptions { mode: smode }).unwrap();
                assert_eq!(
                    d.to_bits(),
                    ((1.0 - s) / 2.0).to_bits(),
                    "{label} amp={amp} {opts:?}: dssim={d}, ssim={s}",
                );
            }
        }
    }
    check::<Srgb8>("srgb8");
    check::<Srgb16>("srgb16");
    check::<Gray8>("gray8");
    check::<Gray16>("gray16");
    check::<Rgba8>("rgba8");
    check::<Rgba16>("rgba16");
}

#[test]
fn uniform_inputs_match_the_closed_form() {
    // With both images uniform the variance/covariance terms vanish, so SSIM
    // reduces to (2ab + C1) / (a² + b² + C1) and DSSIM to (1 - that) / 2. This
    // is computed independently of `ssim()`, so a bug shared by both functions
    // cannot cancel out. At 8-bit, C1 = (0.01 * 255)² = 6.5025.
    let reference = srgb8(16, 16, |_, _, _| 100);
    let distorted = srgb8(16, 16, |_, _, _| 120);
    let c1 = (0.01 * 255.0_f64).powi(2);
    let ssim_expected = (2.0 * 100.0 * 120.0 + c1) / (100.0 * 100.0 + 120.0 * 120.0 + c1);
    let expected = (1.0 - ssim_expected) / 2.0;
    assert_close(
        dssim(&reference, &distorted, RGB_AVERAGED).unwrap(),
        expected,
        "uniform 100 vs 120",
    );
}

#[test]
fn black_versus_white_is_about_one_half() {
    // The largest possible *brightness* gap. SSIM here collapses to
    // C1 / (255² + C1) ≈ 1e-4, so DSSIM ≈ 0.49995 — close to 0.5, NOT 1.0.
    // (Reaching DSSIM = 1.0 needs anti-correlated structure, not just a big
    // mean difference; see `anti_correlation_pushes_past_one_half`.)
    let black = srgb8(16, 16, |_, _, _| 0);
    let white = srgb8(16, 16, |_, _, _| 255);
    let c1 = (0.01 * 255.0_f64).powi(2);
    let expected = (1.0 - c1 / (255.0 * 255.0 + c1)) / 2.0;
    let score = dssim(&black, &white, RGB_AVERAGED).unwrap();
    assert_close(score, expected, "black vs white");
    assert!(
        (0.49..0.5).contains(&score),
        "black/white DSSIM {score} should sit just under 0.5",
    );
}

#[test]
fn anti_correlation_pushes_past_one_half() {
    // A high-frequency checkerboard versus its photometric negative is locally
    // anti-correlated: within a window cov(v, 255 - v) = -var(v), driving SSIM
    // strongly negative while the means stay near 127.5. DSSIM = (1 - SSIM)/2
    // therefore exceeds 0.5 and approaches — but never reaches or exceeds — 1.0.
    // This exercises the full (-1, 1] -> [0, 1) range and proves the transform
    // never clamps or goes negative.
    let reference = srgb8(24, 24, |x, y, _| if (x + y) % 2 == 0 { 40 } else { 215 });
    let negative = srgb8(24, 24, |x, y, _| if (x + y) % 2 == 0 { 215 } else { 40 });

    let s = ssim(
        &reference,
        &negative,
        SsimOptions {
            mode: SsimMode::RgbAveraged,
        },
    )
    .unwrap();
    let d = dssim(&reference, &negative, RGB_AVERAGED).unwrap();
    assert!(
        s < 0.0,
        "expected negative SSIM for anti-correlation, got {s}"
    );
    assert!(
        d > 0.5 && d <= 1.0,
        "anti-correlated DSSIM {d} should be in (0.5, 1.0]",
    );
}

#[test]
fn quality_decreases_with_distortion() {
    // More distortion must never lower DSSIM, and clearly heavier distortion
    // must score worse (higher) than lighter.
    let base = Srgb8::base(32, 32);
    let scores: Vec<f64> = [0.0, 8.0, 24.0, 60.0]
        .iter()
        .map(|&amp| dssim(&base, &Srgb8::distort(&base, amp), RGB_AVERAGED).unwrap())
        .collect();
    assert_eq!(scores[0], 0.0, "zero distortion should be exactly 0.0");
    for pair in scores.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "DSSIM decreased as distortion grew: {scores:?}",
        );
    }
    assert!(
        scores[1] < scores[3],
        "light distortion should beat heavy: {scores:?}",
    );
}

#[test]
fn luma_mode_is_lenient_on_blue() {
    // Distorting only blue barely moves Rec.709 luma (weight 0.0722), so the
    // luma DSSIM stays closer to 0.0 than the channel-averaged DSSIM, where
    // blue counts for a full third. (The inverse of SSIM's blue-channel test.)
    let reference = srgb8(16, 16, |_, _, _| 128);
    let distorted = srgb8(16, 16, |_, _, c| if c == 2 { 200 } else { 128 });
    let rgb = dssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = dssim(&reference, &distorted, LUMA709).unwrap();
    assert!(luma < rgb, "rgb={rgb}, luma={luma}");
}

#[test]
fn grayscale_is_accepted() {
    let reference = Gray8::base(16, 16);
    let identical = dssim(&reference, &reference, RGB_AVERAGED).unwrap();
    assert_eq!(identical, 0.0, "identical grayscale should be 0.0");

    let distorted = Gray8::distort(&reference, 20.0);
    let score = dssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    assert!(score > 0.0, "distorted grayscale scored {score}");
}

#[test]
fn is_symmetric_to_the_last_bit() {
    let a = Srgb8::base(20, 20);
    let b = Srgb8::distort(&a, 18.0);
    for opts in [RGB_AVERAGED, LUMA709] {
        let forward = dssim(&a, &b, opts).unwrap();
        let backward = dssim(&b, &a, opts).unwrap();
        assert_eq!(
            forward.to_bits(),
            backward.to_bits(),
            "asymmetric: {opts:?}"
        );
    }
}

#[test]
fn score_is_consistent_across_bit_depth() {
    // The 16-bit fixtures are the 8-bit ones scaled by 257, and SSIM's
    // constants scale with the dynamic range, so DSSIM is (up to sample
    // rounding) invariant to bit depth.
    let base8 = Srgb8::base(24, 24);
    let base16 = Srgb16::base(24, 24);
    for amp in [10.0, 30.0] {
        let d8 = dssim(&base8, &Srgb8::distort(&base8, amp), RGB_AVERAGED).unwrap();
        let d16 = dssim(&base16, &Srgb16::distort(&base16, amp), RGB_AVERAGED).unwrap();
        assert!(
            (d8 - d16).abs() < 1e-3,
            "amp {amp}: 8-bit DSSIM {d8} vs 16-bit DSSIM {d16}",
        );
    }
}

#[test]
fn images_below_11x11_are_rejected() {
    // Below the window in either axis: nowhere for an 11x11 window to sit.
    for (w, h) in [(10, 10), (11, 10), (10, 11)] {
        let tiny = srgb8(w, h, |_, _, _| 0);
        assert!(
            matches!(dssim(&tiny, &tiny, RGB_AVERAGED), Err(Error::ImageTooSmall(tw, th, 11)) if tw == w && th == h),
            "{w}x{h} should be rejected",
        );
    }
}

#[test]
fn smallest_accepted_size_works() {
    // 11x11 is exactly one window: identical -> 0.0, distorted -> finite > 0.
    let reference = srgb8(11, 11, |x, y, c| {
        (60 + (x * 3 + y * 5 + c as u32 * 11) % 120) as u8
    });
    assert_eq!(
        dssim(&reference, &reference, RGB_AVERAGED).unwrap(),
        0.0,
        "11x11 identical",
    );
    let distorted = srgb8(11, 11, |x, y, c| {
        let base = 60 + (x * 3 + y * 5 + c as u32 * 11) % 120;
        let bumped = if (x + y) % 3 == 0 { base + 40 } else { base };
        bumped.min(255) as u8
    });
    let score = dssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    assert!(
        score.is_finite() && score > 0.0,
        "11x11 distorted scored {score}"
    );
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = srgb8(16, 16, |_, _, _| 0);
    let b = srgb8(16, 12, |_, _, _| 0);
    assert!(matches!(
        dssim(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. }),
    ));
}
