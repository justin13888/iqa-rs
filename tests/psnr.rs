//! Exact-value black-box tests for PSNR.
//!
//! PSNR has a closed form, so these tests pin precise values rather than mere
//! orderings: `PSNR = 20 * log10(MAX / d)` for a uniform per-sample error `d`.
//! Together with the universal battery in `tests/properties.rs`, an
//! implementation that passes both is exercised against the exact definition.
#![cfg(feature = "psnr")]

mod common;

use common::*;
use iqa_rs::{Error, PsnrMode, PsnrOptions, psnr};

const RGB_AVERAGED: PsnrOptions = PsnrOptions {
    mode: PsnrMode::RgbAveraged,
};
const LUMA709: PsnrOptions = PsnrOptions {
    mode: PsnrMode::Luma709,
};

/// PSNR in dB for a uniform per-sample error `d` at the given `max` value.
fn expected_psnr(max: f64, d: f64) -> f64 {
    20.0 * (max / d).log10()
}

fn assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "{context}: got {actual}, expected {expected}",
    );
}

#[test]
fn identical_images_are_infinite() {
    let fixtures = [
        ("rgb8", base_rgb8(8, 8)),
        ("rgb16", base_rgb16(8, 8)),
        ("grayscale", gray8(8, 8, |x, y| (x + y) as u8)),
        ("rgba8", rgba8(8, 8, |x, y, c| (x + y + c as u32) as u8)),
    ];
    for (label, img) in fixtures {
        for opts in [RGB_AVERAGED, LUMA709] {
            assert_eq!(
                psnr(&img, &img, opts).unwrap(),
                f64::INFINITY,
                "{label}: identical images must score infinite PSNR",
            );
        }
    }
}

#[test]
fn black_versus_white_is_zero_db() {
    // d = MAX, so PSNR = 20*log10(1) = 0 dB exactly.
    let black = rgb8(4, 4, |_, _, _| 0);
    let white = rgb8(4, 4, |_, _, _| 255);
    let result = psnr(&black, &white, RGB_AVERAGED).unwrap();
    assert!(result.abs() < 1e-12, "expected 0 dB, got {result}");
}

#[test]
fn uniform_error_matches_the_closed_form_8bit() {
    for d in [1u8, 7, 64, 200] {
        let reference = rgb8(6, 6, |_, _, _| 0);
        let distorted = rgb8(6, 6, |_, _, _| d);
        let result = psnr(&reference, &distorted, RGB_AVERAGED).unwrap();
        assert_close(
            result,
            expected_psnr(255.0, d as f64),
            &format!("8-bit d={d}"),
        );
    }
}

#[test]
fn uniform_error_matches_the_closed_form_grayscale() {
    for d in [1u8, 25, 128] {
        let reference = gray8(6, 6, |_, _| 0);
        let distorted = gray8(6, 6, |_, _| d);
        let result = psnr(&reference, &distorted, RGB_AVERAGED).unwrap();
        assert_close(
            result,
            expected_psnr(255.0, d as f64),
            &format!("gray d={d}"),
        );
    }
}

#[test]
fn uniform_error_matches_the_closed_form_16bit() {
    // MAX is 65535 for 16-bit, so the same absolute error scores far higher.
    for d in [1u16, 100, 30_000] {
        let reference = rgb16(6, 6, |_, _, _| 0);
        let distorted = rgb16(6, 6, |_, _, _| d);
        let result = psnr(&reference, &distorted, RGB_AVERAGED).unwrap();
        assert_close(
            result,
            expected_psnr(65535.0, d as f64),
            &format!("16-bit d={d}"),
        );
    }
}

#[test]
fn single_sample_error_matches_hand_computed_mse() {
    // One sample of a 4x4 RGB image (48 samples) is off by 1: MSE = 1/48.
    let reference = rgb8(4, 4, |_, _, _| 100);
    let distorted = rgb8(
        4,
        4,
        |x, y, c| if x == 0 && y == 0 && c == 0 { 101 } else { 100 },
    );
    let result = psnr(&reference, &distorted, RGB_AVERAGED).unwrap();
    let expected = 10.0 * (255.0_f64 * 255.0 / (1.0 / 48.0)).log10();
    assert_close(result, expected, "single off-by-one sample");
}

#[test]
fn luma_applies_rec709_weights() {
    // Distort only the red channel by d. Rec.709 luma error is 0.2126*d,
    // giving PSNR = 20*log10(MAX / (0.2126*d)).
    let d = 40.0;
    let reference = rgb8(6, 6, |_, _, _| 50);
    let distorted = rgb8(6, 6, |_, _, c| if c == 0 { 90 } else { 50 });
    let result = psnr(&reference, &distorted, LUMA709).unwrap();
    assert_close(
        result,
        expected_psnr(255.0, 0.2126 * d),
        "luma red-only distortion",
    );
}

#[test]
fn luma_and_rgb_agree_on_grayscale() {
    // For a grayscale image luma equals the single channel, so both modes
    // must yield bit-identical results.
    let reference = gray8(8, 8, |x, y| (x * 5 + y * 3) as u8);
    let distorted = gray8(8, 8, |x, y| (x * 5 + y * 3 + 7) as u8);
    let rgb = psnr(&reference, &distorted, RGB_AVERAGED).unwrap();
    let luma = psnr(&reference, &distorted, LUMA709).unwrap();
    assert_eq!(rgb.to_bits(), luma.to_bits(), "grayscale: {rgb} vs {luma}");
}

#[test]
fn alpha_differences_are_ignored() {
    let opaque = rgba8(5, 5, |x, y, c| if c == 3 { 255 } else { (x + y) as u8 });
    let transparent = rgba8(5, 5, |x, y, c| if c == 3 { 0 } else { (x + y) as u8 });
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_eq!(
            psnr(&opaque, &transparent, opts).unwrap(),
            f64::INFINITY,
            "alpha-only differences must not affect PSNR",
        );
    }
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = base_rgb8(8, 8);
    let b = base_rgb8(8, 4);
    assert!(matches!(
        psnr(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. })
    ));
}

#[test]
fn channel_mismatch_is_an_error() {
    let rgb = base_rgb8(8, 8);
    let gray = gray8(8, 8, |_, _| 0);
    assert!(matches!(
        psnr(&rgb, &gray, RGB_AVERAGED),
        Err(Error::Incompatible(_))
    ));
}

#[test]
fn bit_depth_mismatch_is_an_error() {
    let eight = base_rgb8(8, 8);
    let sixteen = base_rgb16(8, 8);
    assert!(matches!(
        psnr(&eight, &sixteen, RGB_AVERAGED),
        Err(Error::Incompatible(_))
    ));
}
