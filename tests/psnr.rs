//! Exact-value black-box tests for PSNR.
//!
//! PSNR has a closed form, so these tests pin precise values rather than mere
//! orderings: `PSNR = 20 * log10(MAX / d)` for a uniform per-sample error `d`.
//! Together with the universal battery in `tests/properties.rs`, an
//! implementation that passes both is exercised against the exact definition.
#![cfg(feature = "psnr")]

mod common;

use common::*;
use iqa::{Error, PsnrMode, PsnrOptions, psnr};

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
    // One case per pixel format: identical inputs must score infinite PSNR.
    let rgb8 = Srgb8::base(8, 8);
    let rgb16 = Srgb16::base(8, 8);
    let gray = gray8(8, 8, |x, y| (x + y) as u8);
    let rgba = rgba8(8, 8, |x, y, c| (x + y + c as u32) as u8);
    for opts in [RGB_AVERAGED, LUMA709] {
        assert_eq!(psnr(&rgb8, &rgb8, opts).unwrap(), f64::INFINITY, "rgb8");
        assert_eq!(psnr(&rgb16, &rgb16, opts).unwrap(), f64::INFINITY, "rgb16");
        assert_eq!(
            psnr(&gray, &gray, opts).unwrap(),
            f64::INFINITY,
            "grayscale"
        );
        assert_eq!(psnr(&rgba, &rgba, opts).unwrap(), f64::INFINITY, "rgba8");
    }
}

#[test]
fn black_versus_white_is_zero_db() {
    // d = MAX, so PSNR = 20*log10(1) = 0 dB exactly.
    let black = srgb8(4, 4, |_, _, _| 0);
    let white = srgb8(4, 4, |_, _, _| 255);
    let result = psnr(&black, &white, RGB_AVERAGED).unwrap();
    assert!(result.abs() < 1e-12, "expected 0 dB, got {result}");
}

#[test]
fn uniform_error_matches_the_closed_form_8bit() {
    for d in [1u8, 7, 64, 200] {
        let reference = srgb8(6, 6, |_, _, _| 0);
        let distorted = srgb8(6, 6, |_, _, _| d);
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
        let reference = srgb16(6, 6, |_, _, _| 0);
        let distorted = srgb16(6, 6, |_, _, _| d);
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
    let reference = srgb8(4, 4, |_, _, _| 100);
    let distorted = srgb8(
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
    let reference = srgb8(6, 6, |_, _, _| 50);
    let distorted = srgb8(6, 6, |_, _, c| if c == 0 { 90 } else { 50 });
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
    let a = Srgb8::base(8, 8);
    let b = Srgb8::base(8, 4);
    assert!(matches!(
        psnr(&a, &b, RGB_AVERAGED),
        Err(Error::DimensionMismatch { .. })
    ));
}
