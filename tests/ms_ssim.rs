//! MS-SSIM-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to MS-SSIM — its perfect score of `1.0`, the
//! collapse to plain SSIM when only one scale fits, the multi-scale pyramid on
//! a large image, a closed-form value for uniform inputs (which exercises the
//! weight renormalization), and the difference between its two modes. The whole
//! file compiles to nothing unless the `ms-ssim` feature is enabled.
#![cfg(feature = "ms-ssim")]

mod common;

use common::*;
use iqa::{Error, MsssimOptions, SsimMode, SsimOptions, msssim, ssim};

const RGB_AVERAGED: MsssimOptions = MsssimOptions {
    mode: SsimMode::RgbAveraged,
};
const LUMA709: MsssimOptions = MsssimOptions {
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
    // One case per pixel format, in both modes, at a size that runs several
    // scales (48 → 24 → 12): identical inputs must still score exactly 1.0.
    let rgb8 = Srgb8::base(48, 48);
    let rgb16 = Srgb16::base(48, 48);
    let gray8 = Gray8::base(48, 48);
    let gray16 = Gray16::base(48, 48);
    let rgba8 = Rgba8::base(48, 48);
    let rgba16 = Rgba16::base(48, 48);
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_close(msssim(&rgb8, &rgb8, opts).unwrap(), 1.0, "srgb8");
        assert_close(msssim(&rgb16, &rgb16, opts).unwrap(), 1.0, "srgb16");
        assert_close(msssim(&gray8, &gray8, opts).unwrap(), 1.0, "gray8");
        assert_close(msssim(&gray16, &gray16, opts).unwrap(), 1.0, "gray16");
        assert_close(msssim(&rgba8, &rgba8, opts).unwrap(), 1.0, "rgba8");
        assert_close(msssim(&rgba16, &rgba16, opts).unwrap(), 1.0, "rgba16");
    }
}

#[test]
fn single_scale_equals_plain_ssim() {
    // 16x16 is too small to downsample (the second scale would be 8x8, below
    // the 11x11 window), so MS-SSIM collapses to one full-SSIM scale and must
    // equal `ssim` bit-for-bit.
    let reference = Srgb8::base(16, 16);
    let distorted = Srgb8::distort(&reference, 20.0);
    for (ms_opts, ssim_opts) in [
        (
            RGB_AVERAGED,
            SsimOptions {
                mode: SsimMode::RgbAveraged,
            },
        ),
        (
            LUMA709,
            SsimOptions {
                mode: SsimMode::Luma709,
            },
        ),
    ] {
        let ms = msssim(&reference, &distorted, ms_opts).unwrap();
        let plain = ssim(&reference, &distorted, ssim_opts).unwrap();
        assert_eq!(
            ms.to_bits(),
            plain.to_bits(),
            "{ms_opts:?}: {ms} vs {plain}"
        );
    }
}

#[test]
fn uniform_inputs_match_the_closed_form() {
    // With both images uniform the contrast-structure term is exactly 1.0 at
    // every scale, so MS-SSIM reduces to the coarsest scale's luminance term
    // raised to that scale's renormalized weight. At 48x48 three scales fit, so
    // the exponent is WEIGHT[2] / (WEIGHT[0] + WEIGHT[1] + WEIGHT[2]).
    let reference = srgb8(48, 48, |_, _, _| 100);
    let distorted = srgb8(48, 48, |_, _, _| 120);
    let c1 = (0.01 * 255.0_f64).powi(2);
    let luminance = (2.0 * 100.0 * 120.0 + c1) / (100.0 * 100.0 + 120.0 * 120.0 + c1);
    let exponent = 0.3001 / (0.0448 + 0.2856 + 0.3001);
    assert_close(
        msssim(&reference, &distorted, RGB_AVERAGED).unwrap(),
        luminance.powf(exponent),
        "uniform 100 vs 120",
    );
}

#[test]
fn full_five_scales_run_on_a_large_image() {
    // 176x176 is the smallest image where all five scales fit (the coarsest is
    // 11x11). The score must be a valid, distortion-sensitive similarity.
    let base = Srgb8::base(176, 176);
    let identical = msssim(&base, &base, RGB_AVERAGED).unwrap();
    assert_close(identical, 1.0, "identical 176x176");
    let distorted = msssim(&base, &Srgb8::distort(&base, 25.0), RGB_AVERAGED).unwrap();
    assert!(
        distorted < 1.0 && distorted > -1.0 && distorted.is_finite(),
        "distorted 176x176 scored {distorted}",
    );
}

#[test]
fn argument_order_does_not_matter() {
    // MS-SSIM is a product of bit-exactly symmetric per-scale SSIM terms.
    let a = Srgb8::base(48, 48);
    let b = Srgb8::distort(&a, 18.0);
    for opts in [RGB_AVERAGED, LUMA709] {
        let forward = msssim(&a, &b, opts).unwrap();
        let backward = msssim(&b, &a, opts).unwrap();
        assert_eq!(
            forward.to_bits(),
            backward.to_bits(),
            "asymmetric: {opts:?}"
        );
    }
}

#[test]
fn quality_decreases_with_distortion() {
    let base = Srgb8::base(48, 48);
    let scores: Vec<f64> = [0.0, 8.0, 24.0, 60.0]
        .iter()
        .map(|&amp| msssim(&base, &Srgb8::distort(&base, amp), RGB_AVERAGED).unwrap())
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
    // luma score stays closer to 1.0 than the channel-averaged score.
    let reference = srgb8(48, 48, |_, _, _| 128);
    let distorted = srgb8(48, 48, |_, _, c| if c == 2 { 200 } else { 128 });
    let rgb = msssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = msssim(&reference, &distorted, LUMA709).unwrap();
    assert!(luma > rgb, "rgb={rgb}, luma={luma}");
}

#[test]
fn grayscale_modes_agree() {
    // For a single-channel image, channel-averaging and Rec.709 luma both
    // reduce to the one gray channel, so the two modes are bit-identical.
    let reference = Gray8::base(48, 48);
    let distorted = Gray8::distort(&reference, 20.0);
    let rgb = msssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = msssim(&reference, &distorted, LUMA709).unwrap();
    assert_eq!(rgb.to_bits(), luma.to_bits(), "gray modes differ");
    assert!(rgb < 1.0, "distorted grayscale scored {rgb}");
}

#[test]
fn alpha_channel_is_ignored() {
    let base = Rgba8::base(48, 48);
    let variant = Rgba8::alpha_variant(&base).expect("rgba8 has alpha");
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_close(
            msssim(&base, &variant, opts).unwrap(),
            1.0,
            "alpha changed score",
        );
    }
}

#[test]
fn images_below_11x11_are_rejected() {
    // Below the window in either axis: nowhere for an 11x11 window to sit even
    // at the finest scale.
    for (w, h) in [(10, 10), (11, 10), (10, 11)] {
        let tiny = srgb8(w, h, |_, _, _| 0);
        assert!(
            matches!(msssim(&tiny, &tiny, RGB_AVERAGED), Err(Error::ImageTooSmall(tw, th, 11)) if tw == w && th == h),
            "{w}x{h} should be rejected",
        );
    }
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = srgb8(16, 16, |_, _, _| 0);
    let b = srgb8(16, 12, |_, _, _| 0);
    assert!(matches!(
        msssim(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. }),
    ));
}
