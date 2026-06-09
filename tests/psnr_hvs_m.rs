//! PSNR-HVS-M-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to PSNR-HVS-M — its infinite perfect score, the
//! negative score that CSF weighting can produce, the contrast-masking effect,
//! and the difference between its two modes. The whole file compiles to nothing
//! unless the `psnr-hvs-m` feature is enabled.
#![cfg(feature = "psnr-hvs-m")]

mod common;

use common::*;
use iqa::{Error, PsnrHvsMode, PsnrHvsOptions, psnr_hvs_m};

const RGB_AVERAGED: PsnrHvsOptions = PsnrHvsOptions {
    mode: PsnrHvsMode::RgbAveraged,
};
const LUMA709: PsnrHvsOptions = PsnrHvsOptions {
    mode: PsnrHvsMode::Luma709,
};

#[test]
fn identical_images_are_infinite() {
    // One case per pixel format, in both modes: identical inputs score infinite.
    let rgb8 = Srgb8::base(16, 16);
    let rgb16 = Srgb16::base(16, 16);
    let gray8 = Gray8::base(16, 16);
    let gray16 = Gray16::base(16, 16);
    let rgba8 = Rgba8::base(16, 16);
    let rgba16 = Rgba16::base(16, 16);
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_eq!(
            psnr_hvs_m(&rgb8, &rgb8, opts).unwrap(),
            f64::INFINITY,
            "srgb8"
        );
        assert_eq!(
            psnr_hvs_m(&rgb16, &rgb16, opts).unwrap(),
            f64::INFINITY,
            "srgb16"
        );
        assert_eq!(
            psnr_hvs_m(&gray8, &gray8, opts).unwrap(),
            f64::INFINITY,
            "gray8"
        );
        assert_eq!(
            psnr_hvs_m(&gray16, &gray16, opts).unwrap(),
            f64::INFINITY,
            "gray16"
        );
        assert_eq!(
            psnr_hvs_m(&rgba8, &rgba8, opts).unwrap(),
            f64::INFINITY,
            "rgba8"
        );
        assert_eq!(
            psnr_hvs_m(&rgba16, &rgba16, opts).unwrap(),
            f64::INFINITY,
            "rgba16"
        );
    }
}

#[test]
fn black_versus_white_goes_negative() {
    // The maximal DC error is never masked, and the CSF weights it above unity,
    // so the perceptual MSE exceeds the 255² reference: PSNR-HVS-M is not floored
    // at 0 dB the way plain PSNR is. This pins that distinguishing property.
    let black = gray8(16, 16, |_, _| 0);
    let white = gray8(16, 16, |_, _| 255);
    let score = psnr_hvs_m(&black, &white, RGB_AVERAGED).unwrap();
    assert!(score.is_finite() && score < 0.0, "got {score}");
}

#[test]
fn argument_order_does_not_matter() {
    // The masking threshold is max(reference, distorted), so the metric is
    // symmetric down to the last bit.
    let a = Srgb8::base(24, 24);
    let b = Srgb8::distort(&a, 18.0);
    for opts in [RGB_AVERAGED, LUMA709] {
        let forward = psnr_hvs_m(&a, &b, opts).unwrap();
        let backward = psnr_hvs_m(&b, &a, opts).unwrap();
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
        .map(|&amp| psnr_hvs_m(&base, &Srgb8::distort(&base, amp), RGB_AVERAGED).unwrap())
        .collect();
    assert_eq!(
        scores[0],
        f64::INFINITY,
        "zero distortion should be infinite"
    );
    for pair in scores[1..].windows(2) {
        assert!(
            pair[1] <= pair[0],
            "score increased with distortion: {scores:?}"
        );
    }
    assert!(scores[1] > scores[3], "light should beat heavy: {scores:?}");
}

#[test]
fn masking_hides_error_in_busy_regions() {
    // The same per-pixel error is penalized less over a high-energy (busy)
    // region than over a flat one, because the busy region's own AC energy masks
    // it — the defining feature of the "-M" variant.
    let flat = gray8(32, 32, |_, _| 128);
    let busy = gray8(32, 32, |x, y| {
        let v = 128.0 + 90.0 * ((x as f64 * 1.3).sin() * (y as f64 * 1.1).cos());
        v.round().clamp(0.0, 255.0) as u8
    });

    let perturb = |img: &Image<Gray8>| {
        let w = img.width();
        gray8(w, img.height(), |x, y| {
            let base = img.samples()[(y * w + x) as usize] as i32;
            let d = if (x + y) % 2 == 0 { 14 } else { -14 };
            (base + d).clamp(0, 255) as u8
        })
    };

    let flat_score = psnr_hvs_m(&flat, &perturb(&flat), RGB_AVERAGED).unwrap();
    let busy_score = psnr_hvs_m(&busy, &perturb(&busy), RGB_AVERAGED).unwrap();
    assert!(
        busy_score > flat_score,
        "busy={busy_score}, flat={flat_score}"
    );
}

#[test]
fn luma_mode_is_lenient_on_blue() {
    // Distorting only blue barely moves Rec.709 luma (weight 0.0722), so the
    // luma-mode score stays higher than the channel-pooled score.
    let reference = srgb8(32, 32, |_, _, _| 128);
    let distorted = srgb8(32, 32, |_, _, c| if c == 2 { 190 } else { 128 });
    let rgb = psnr_hvs_m(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = psnr_hvs_m(&reference, &distorted, LUMA709).unwrap();
    assert!(luma > rgb, "rgb={rgb}, luma={luma}");
}

#[test]
fn grayscale_modes_agree() {
    // For a single-channel image, channel-pooling and Rec.709 luma both reduce
    // to the one gray channel, so the two modes are bit-identical.
    let reference = Gray8::base(32, 32);
    let distorted = Gray8::distort(&reference, 20.0);
    let rgb = psnr_hvs_m(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = psnr_hvs_m(&reference, &distorted, LUMA709).unwrap();
    assert_eq!(rgb.to_bits(), luma.to_bits(), "gray modes differ");
    assert!(rgb.is_finite(), "distorted grayscale scored {rgb}");
}

#[test]
fn alpha_channel_is_ignored() {
    let base = Rgba8::base(16, 16);
    let variant = Rgba8::alpha_variant(&base).expect("rgba8 has alpha");
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_eq!(
            psnr_hvs_m(&base, &variant, opts).unwrap(),
            f64::INFINITY,
            "alpha changed the score",
        );
    }
}

#[test]
fn images_below_8x8_are_rejected() {
    // Below the block in either axis: no full 8x8 block fits.
    for (w, h) in [(7, 7), (8, 7), (7, 8)] {
        let tiny = srgb8(w, h, |_, _, _| 0);
        assert!(
            matches!(psnr_hvs_m(&tiny, &tiny, RGB_AVERAGED), Err(Error::ImageTooSmall(tw, th, 8)) if tw == w && th == h),
            "{w}x{h} should be rejected",
        );
    }
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = srgb8(16, 16, |_, _, _| 0);
    let b = srgb8(16, 8, |_, _, _| 0);
    assert!(matches!(
        psnr_hvs_m(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. }),
    ));
}
