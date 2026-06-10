//! IW-SSIM-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to IW-SSIM — its perfect score of `1.0`, the
//! collapse to plain SSIM when only one scale fits, the multi-scale pyramid on a
//! large image, a closed-form value for uniform inputs, its (deliberate)
//! asymmetry, and the difference between its two modes. The whole file compiles
//! to nothing unless the `iw-ssim` feature is enabled.
#![cfg(feature = "iw-ssim")]

mod common;

use common::*;
use iqa::{Error, IwssimOptions, SsimMode, SsimOptions, iwssim, ssim};

const RGB_AVERAGED: IwssimOptions = IwssimOptions {
    mode: SsimMode::RgbAveraged,
};
const LUMA709: IwssimOptions = IwssimOptions {
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
    // scales (48 → 24 → 12): identical inputs must still score exactly 1.0,
    // including the information-weighted pooling on the band-pass scales.
    let rgb8 = Srgb8::base(48, 48);
    let rgb16 = Srgb16::base(48, 48);
    let gray8 = Gray8::base(48, 48);
    let gray16 = Gray16::base(48, 48);
    let rgba8 = Rgba8::base(48, 48);
    let rgba16 = Rgba16::base(48, 48);
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_close(iwssim(&rgb8, &rgb8, opts).unwrap(), 1.0, "srgb8");
        assert_close(iwssim(&rgb16, &rgb16, opts).unwrap(), 1.0, "srgb16");
        assert_close(iwssim(&gray8, &gray8, opts).unwrap(), 1.0, "gray8");
        assert_close(iwssim(&gray16, &gray16, opts).unwrap(), 1.0, "gray16");
        assert_close(iwssim(&rgba8, &rgba8, opts).unwrap(), 1.0, "rgba8");
        assert_close(iwssim(&rgba16, &rgba16, opts).unwrap(), 1.0, "rgba16");
    }
}

#[test]
fn single_scale_equals_plain_ssim() {
    // 16x16 is too small to build a pyramid (the second scale would be 8x8,
    // below the 11x11 window), so IW-SSIM collapses to one full-SSIM scale on the
    // original image and must equal `ssim` bit-for-bit.
    let reference = Srgb8::base(16, 16);
    let distorted = Srgb8::distort(&reference, 20.0);
    for (iw_opts, ssim_opts) in [
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
        let iw = iwssim(&reference, &distorted, iw_opts).unwrap();
        let plain = ssim(&reference, &distorted, ssim_opts).unwrap();
        assert_eq!(
            iw.to_bits(),
            plain.to_bits(),
            "{iw_opts:?}: {iw} vs {plain}"
        );
    }
}

#[test]
fn uniform_inputs_match_the_closed_form() {
    // With both images uniform every band-pass scale's contrast-structure term
    // is exactly 1.0 and carries zero information content, so its weighted pool
    // falls back to the (unit) mean; IW-SSIM reduces to the coarsest scale's
    // luminance term raised to that scale's renormalized weight. At 48x48 three
    // scales fit, so the exponent is WEIGHT[2] / (WEIGHT[0]+WEIGHT[1]+WEIGHT[2]).
    //
    // The luminance is measured on the *low-pass residual*, not the original
    // pixels: the Laplacian pyramid's sqrt(2)-scaled binom5 filter gives each
    // reduction a DC gain of 2, so after `nsc - 1 = 2` reductions a uniform
    // value `v` becomes `4v` in the coarsest band.
    let reference = srgb8(48, 48, |_, _, _| 100);
    let distorted = srgb8(48, 48, |_, _, _| 120);
    let c1 = (0.01 * 255.0_f64).powi(2);
    let (a, b) = (4.0 * 100.0, 4.0 * 120.0);
    let luminance = (2.0 * a * b + c1) / (a * a + b * b + c1);
    let exponent = 0.3001 / (0.0448 + 0.2856 + 0.3001);
    assert_close(
        iwssim(&reference, &distorted, RGB_AVERAGED).unwrap(),
        luminance.powf(exponent),
        "uniform 100 vs 120",
    );
}

#[test]
fn full_five_scales_run_on_a_large_image() {
    // 176x176 is the smallest image where all five scales fit (the coarsest is
    // 11x11). The score must be a valid, distortion-sensitive similarity.
    let base = Srgb8::base(176, 176);
    let identical = iwssim(&base, &base, RGB_AVERAGED).unwrap();
    assert_close(identical, 1.0, "identical 176x176");
    let distorted = iwssim(&base, &Srgb8::distort(&base, 25.0), RGB_AVERAGED).unwrap();
    assert!(
        distorted < 1.0 && distorted > -1.0 && distorted.is_finite(),
        "distorted 176x176 scored {distorted}",
    );
}

#[test]
fn weighting_makes_it_asymmetric() {
    // IW-SSIM's pooling weights are estimated from the *reference*, so swapping
    // the arguments of two structurally different images changes the score —
    // unlike SSIM/MS-SSIM, which are exactly symmetric.
    let texture = gray8(64, 64, |x, y| {
        (128.0 + 90.0 * ((x as f64) * 0.5).sin() * ((y as f64) * 0.4).cos()).round() as u8
    });
    let gradient = gray8(64, 64, |x, y| ((x + y) % 200 + 20) as u8);
    let forward = iwssim(&texture, &gradient, LUMA709).unwrap();
    let backward = iwssim(&gradient, &texture, LUMA709).unwrap();
    assert!(
        (forward - backward).abs() > 1e-6,
        "expected asymmetry, got forward={forward}, backward={backward}",
    );
}

#[test]
fn quality_decreases_with_distortion() {
    let base = Srgb8::base(48, 48);
    let scores: Vec<f64> = [0.0, 8.0, 24.0, 60.0]
        .iter()
        .map(|&amp| iwssim(&base, &Srgb8::distort(&base, amp), RGB_AVERAGED).unwrap())
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
    let rgb = iwssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = iwssim(&reference, &distorted, LUMA709).unwrap();
    assert!(luma > rgb, "rgb={rgb}, luma={luma}");
}

#[test]
fn grayscale_modes_agree() {
    // For a single-channel image, channel-averaging and Rec.709 luma both reduce
    // to the one gray channel, so the two modes are bit-identical.
    let reference = Gray8::base(48, 48);
    let distorted = Gray8::distort(&reference, 20.0);
    let rgb = iwssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = iwssim(&reference, &distorted, LUMA709).unwrap();
    assert_eq!(rgb.to_bits(), luma.to_bits(), "gray modes differ");
    assert!(rgb < 1.0, "distorted grayscale scored {rgb}");
}

#[test]
fn alpha_channel_is_ignored() {
    let base = Rgba8::base(48, 48);
    let variant = Rgba8::alpha_variant(&base).expect("rgba8 has alpha");
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_close(
            iwssim(&base, &variant, opts).unwrap(),
            1.0,
            "alpha changed score",
        );
    }
}

#[test]
fn accepts_exactly_11x11() {
    // The window size itself is the smallest accepted image (one scale, so it
    // collapses to plain SSIM); only *below* 11 is rejected.
    let img = Srgb8::base(11, 11);
    let score = iwssim(&img, &img, RGB_AVERAGED).expect("11x11 must be accepted");
    assert_close(score, 1.0, "identical 11x11");
}

#[test]
fn scale_invariant_across_bit_depth() {
    // IW-SSIM is invariant to the dynamic range: every pyramid/statistics step is
    // linear in the pixels, and C1, C2, and the GSM noise term all scale with L².
    // An 8-bit image and its exact 257x lift to 16-bit must therefore score the
    // same — which pins the L-scaling of all three constants (sigma_nsq included).
    let reference8 = Gray8::base(48, 48);
    let distorted8 = Gray8::distort(&reference8, 20.0);
    let lift = |img: &Image<Gray8>| {
        let data: Vec<u16> = img.samples().iter().map(|&v| u16::from(v) * 257).collect();
        Image::gray16(img.width(), img.height(), data).unwrap()
    };
    let reference16 = lift(&reference8);
    let distorted16 = lift(&distorted8);
    let s8 = iwssim(&reference8, &distorted8, LUMA709).unwrap();
    let s16 = iwssim(&reference16, &distorted16, LUMA709).unwrap();
    assert!(
        (s8 - s16).abs() < 1e-9,
        "not scale-invariant: 8-bit={s8}, 16-bit={s16}",
    );
}

#[test]
fn images_below_11x11_are_rejected() {
    // Below the window in either axis: nowhere for an 11x11 window to sit even at
    // the finest scale.
    for (w, h) in [(10, 10), (11, 10), (10, 11)] {
        let tiny = srgb8(w, h, |_, _, _| 0);
        assert!(
            matches!(iwssim(&tiny, &tiny, RGB_AVERAGED), Err(Error::ImageTooSmall(tw, th, 11)) if tw == w && th == h),
            "{w}x{h} should be rejected",
        );
    }
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = srgb8(16, 16, |_, _, _| 0);
    let b = srgb8(16, 12, |_, _, _| 0);
    assert!(matches!(
        iwssim(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. }),
    ));
}
