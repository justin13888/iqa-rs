//! SSIM-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to SSIM — its perfect score of `1.0`, a
//! closed-form value for uniform inputs, the difference between its two modes,
//! and the 11x11 minimum. The whole file compiles to nothing unless the `ssim`
//! feature is enabled.
#![cfg(feature = "ssim")]

mod common;

use common::*;
use iqa::{Error, SsimMode, SsimOptions, ssim};

const RGB_AVERAGED: SsimOptions = SsimOptions {
    mode: SsimMode::RgbAveraged,
};
const LUMA709: SsimOptions = SsimOptions {
    mode: SsimMode::Luma709,
};

fn assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "{context}: got {actual}, expected {expected}",
    );
}

#[test]
fn identical_images_score_one() {
    // One case per pixel format, in both modes: identical inputs score 1.0.
    let rgb8 = Srgb8::base(16, 16);
    let rgb16 = Srgb16::base(16, 16);
    let gray8 = Gray8::base(16, 16);
    let gray16 = Gray16::base(16, 16);
    let rgba8 = Rgba8::base(16, 16);
    let rgba16 = Rgba16::base(16, 16);
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_close(ssim(&rgb8, &rgb8, opts).unwrap(), 1.0, "srgb8");
        assert_close(ssim(&rgb16, &rgb16, opts).unwrap(), 1.0, "srgb16");
        assert_close(ssim(&gray8, &gray8, opts).unwrap(), 1.0, "gray8");
        assert_close(ssim(&gray16, &gray16, opts).unwrap(), 1.0, "gray16");
        assert_close(ssim(&rgba8, &rgba8, opts).unwrap(), 1.0, "rgba8");
        assert_close(ssim(&rgba16, &rgba16, opts).unwrap(), 1.0, "rgba16");
    }
}

#[test]
fn uniform_inputs_match_the_closed_form() {
    // With both images uniform the variance and covariance terms vanish at
    // every window, so SSIM reduces to (2ab + C1) / (a² + b² + C1). This pins
    // the formula and the C1 constant exactly, independent of the Gaussian
    // window. At 8-bit, C1 = (0.01 * 255)² = 6.5025.
    let reference = srgb8(16, 16, |_, _, _| 100);
    let distorted = srgb8(16, 16, |_, _, _| 120);
    let c1 = (0.01 * 255.0_f64).powi(2);
    let expected = (2.0 * 100.0 * 120.0 + c1) / (100.0 * 100.0 + 120.0 * 120.0 + c1);
    assert_close(
        ssim(&reference, &distorted, RGB_AVERAGED).unwrap(),
        expected,
        "uniform 100 vs 120",
    );
}

#[test]
fn argument_order_does_not_matter() {
    // SSIM is symmetric in its two inputs, down to the last bit.
    let a = Srgb8::base(20, 20);
    let b = Srgb8::distort(&a, 18.0);
    for opts in [RGB_AVERAGED, LUMA709] {
        let forward = ssim(&a, &b, opts).unwrap();
        let backward = ssim(&b, &a, opts).unwrap();
        assert_eq!(
            forward.to_bits(),
            backward.to_bits(),
            "asymmetric: {opts:?}"
        );
    }
}

#[test]
fn quality_decreases_with_distortion() {
    let base = Srgb8::base(32, 32);
    let scores: Vec<f64> = [0.0, 8.0, 24.0, 60.0]
        .iter()
        .map(|&amp| ssim(&base, &Srgb8::distort(&base, amp), RGB_AVERAGED).unwrap())
        .collect();
    assert_close(scores[0], 1.0, "zero distortion");
    for pair in scores.windows(2) {
        assert!(
            pair[1] <= pair[0],
            "score increased with distortion: {scores:?}"
        );
    }
    assert!(scores[1] > scores[3], "light should beat heavy: {scores:?}");
}

#[test]
fn luma_mode_is_lenient_on_blue() {
    // Distorting only blue barely moves Rec.709 luma (weight 0.0722), so the
    // luma score stays closer to 1.0 than the channel-averaged score, where
    // blue counts for a full third.
    let reference = srgb8(16, 16, |_, _, _| 128);
    let distorted = srgb8(16, 16, |_, _, c| if c == 2 { 200 } else { 128 });
    let rgb = ssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = ssim(&reference, &distorted, LUMA709).unwrap();
    assert!(luma > rgb, "rgb={rgb}, luma={luma}");
}

#[test]
fn grayscale_modes_agree() {
    // For a single-channel image, channel-averaging and Rec.709 luma both
    // reduce to the one gray channel, so the two modes are bit-identical.
    let reference = Gray8::base(16, 16);
    let distorted = Gray8::distort(&reference, 20.0);
    let rgb = ssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = ssim(&reference, &distorted, LUMA709).unwrap();
    assert_eq!(rgb.to_bits(), luma.to_bits(), "gray modes differ");
    assert!(rgb < 1.0, "distorted grayscale scored {rgb}");
}

#[test]
fn alpha_channel_is_ignored() {
    let base = Rgba8::base(16, 16);
    let variant = Rgba8::alpha_variant(&base).expect("rgba8 has alpha");
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_close(
            ssim(&base, &variant, opts).unwrap(),
            1.0,
            "alpha changed score",
        );
    }
}

#[test]
fn scores_never_exceed_one() {
    let base = Srgb8::base(24, 24);
    for amplitude in [5.0, 25.0, 70.0] {
        let score = ssim(&base, &Srgb8::distort(&base, amplitude), RGB_AVERAGED).unwrap();
        assert!(
            score <= 1.0 + 1e-9 && score.is_finite(),
            "amplitude {amplitude}: score {score} is out of range",
        );
    }
}

#[test]
fn images_below_11x11_are_rejected() {
    // Below the window in either axis: nowhere for an 11x11 window to sit.
    for (w, h) in [(10, 10), (11, 10), (10, 11)] {
        let tiny = srgb8(w, h, |_, _, _| 0);
        assert!(
            matches!(ssim(&tiny, &tiny, RGB_AVERAGED), Err(Error::ImageTooSmall(tw, th, 11)) if tw == w && th == h),
            "{w}x{h} should be rejected",
        );
    }
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = srgb8(16, 16, |_, _, _| 0);
    let b = srgb8(16, 12, |_, _, _| 0);
    assert!(matches!(
        ssim(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. }),
    ));
}
