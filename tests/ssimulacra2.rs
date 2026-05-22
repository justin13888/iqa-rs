//! Integration tests for the SSIMULACRA2 FFI binding.
//!
//! The whole file compiles to nothing unless the `ssimulacra2` feature is
//! enabled, so a default `cargo test` run skips it.
#![cfg(feature = "ssimulacra2")]

use iqa_rs::{BitDepth, Channels, ColorSpace, Error, Image, ssimulacra2};

/// Builds an 8-bit sRGB RGB image from a per-sample generator `f(x, y, c)`.
fn rgb8(width: u32, height: u32, mut f: impl FnMut(u32, u32, usize) -> u8) -> Image {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                data.push(f(x, y, c));
            }
        }
    }
    Image::new(
        width,
        height,
        Channels::Rgb,
        BitDepth::Eight,
        ColorSpace::Srgb,
        data,
    )
    .unwrap()
}

/// A diagonal gradient — non-trivial structure for the metric to chew on.
fn gradient(width: u32, height: u32) -> Image {
    rgb8(width, height, |x, y, c| {
        (((x + y) * 2 + c as u32 * 40) % 256) as u8
    })
}

#[test]
fn identical_gradient_scores_near_100() {
    let img = gradient(64, 64);
    let score = ssimulacra2(&img, &img).unwrap();
    assert!(
        (99.5..=100.0).contains(&score),
        "expected ~100 for identical images, got {score}"
    );
}

#[test]
fn identical_solid_color_scores_near_100() {
    let img = rgb8(32, 32, |_, _, _| 128);
    let score = ssimulacra2(&img, &img).unwrap();
    assert!(score > 99.5 && score.is_finite(), "got {score}");
}

#[test]
fn distortion_lowers_the_score() {
    let reference = gradient(64, 64);
    // Add deterministic noise via a small LCG; clearly visible distortion.
    let mut state = 0x1234_5678u32;
    let distorted = rgb8(64, 64, |x, y, c| {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let base = (((x + y) * 2 + c as u32 * 40) % 256) as i32;
        let noise = (state >> 24) as i32 % 48 - 24;
        (base + noise).clamp(0, 255) as u8
    });

    let identical = ssimulacra2(&reference, &reference).unwrap();
    let degraded = ssimulacra2(&reference, &distorted).unwrap();

    assert!(
        degraded.is_finite(),
        "degraded score must be finite, got {degraded}"
    );
    assert!(
        degraded < identical,
        "distorted ({degraded}) should score below identical ({identical})"
    );
    assert!(
        degraded < 99.0,
        "expected a clear quality drop, got {degraded}"
    );
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = gradient(16, 16);
    let b = gradient(16, 32);
    assert!(matches!(
        ssimulacra2(&a, &b),
        Err(Error::DimensionMismatch { .. })
    ));
}

#[test]
fn image_below_8x8_is_rejected() {
    let tiny = gradient(4, 4);
    assert!(matches!(
        ssimulacra2(&tiny, &tiny),
        Err(Error::ImageTooSmall(4, 4, 8))
    ));
}
