//! Butteraugli-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to Butteraugli (distance `0.0` for identical
//! images, lower-is-better ordering, the configurable options, and the 8x8
//! minimum). The whole file compiles to nothing unless the `butteraugli`
//! feature is enabled.
#![cfg(feature = "butteraugli")]

mod common;

use common::*;
use iqa::{ButteraugliOptions, Error, butteraugli};

#[test]
fn identical_image_scores_zero() {
    let img = Srgb8::base(64, 64);
    assert_eq!(
        butteraugli(&img, &img, ButteraugliOptions::default()).unwrap(),
        0.0
    );
}

#[test]
fn grayscale_input_is_accepted() {
    // Compiles only because `Gray8: ButteraugliInput`; grayscale is replicated
    // to R=G=B internally.
    let img = gray8(32, 32, |x, y| (x * 4 + y * 3) as u8);
    assert_eq!(
        butteraugli(&img, &img, ButteraugliOptions::default()).unwrap(),
        0.0
    );
}

#[test]
fn smallest_accepted_size_does_not_panic() {
    // 8x8 is the minimum; the reference aborts below it, so this guards the
    // boundary the shim enforces.
    let base = Srgb8::base(8, 8);
    assert_eq!(
        butteraugli(&base, &base, ButteraugliOptions::default()).unwrap(),
        0.0
    );
    let score = butteraugli(
        &base,
        &Srgb8::distort(&base, 30.0),
        ButteraugliOptions::default(),
    )
    .unwrap();
    assert!(score > 0.0 && score.is_finite(), "8x8 distorted: {score}");
}

#[test]
fn distance_grows_with_distortion() {
    let base = Srgb8::base(64, 64);
    let light = butteraugli(
        &base,
        &Srgb8::distort(&base, 6.0),
        ButteraugliOptions::default(),
    )
    .unwrap();
    let heavy = butteraugli(
        &base,
        &Srgb8::distort(&base, 60.0),
        ButteraugliOptions::default(),
    )
    .unwrap();
    assert!(
        0.0 < light && light < heavy,
        "light distortion {light} should be a smaller distance than heavy {heavy}"
    );
}

#[test]
fn larger_pnorm_is_at_least_as_large() {
    // The p-norm pooling is a power mean, monotonically non-decreasing in p:
    // a larger exponent weights the worst regions more heavily.
    let base = Srgb8::base(64, 64);
    let dist = Srgb8::distort(&base, 30.0);
    let three = butteraugli(
        &base,
        &dist,
        ButteraugliOptions {
            pnorm: 3.0,
            ..Default::default()
        },
    )
    .unwrap();
    let twelve = butteraugli(
        &base,
        &dist,
        ButteraugliOptions {
            pnorm: 12.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(twelve >= three, "pnorm 12 ({twelve}) < pnorm 3 ({three})");
}

#[test]
fn higher_intensity_target_increases_distance() {
    let base = Srgb8::base(64, 64);
    let dist = Srgb8::distort(&base, 30.0);
    let dim = butteraugli(
        &base,
        &dist,
        ButteraugliOptions {
            intensity_target: 80.0,
            ..Default::default()
        },
    )
    .unwrap();
    let bright = butteraugli(
        &base,
        &dist,
        ButteraugliOptions {
            intensity_target: 255.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(bright > dim, "brighter target {bright} should exceed {dim}");
}

#[test]
fn dimension_mismatch_is_an_error() {
    let a = Srgb8::base(32, 32);
    let b = Srgb8::base(32, 24);
    assert!(matches!(
        butteraugli(&a, &b, ButteraugliOptions::default()),
        Err(Error::DimensionMismatch { .. }),
    ));
}

#[test]
fn images_below_8x8_are_rejected() {
    let tiny = Srgb8::base(7, 7);
    assert!(matches!(
        butteraugli(&tiny, &tiny, ButteraugliOptions::default()),
        Err(Error::ImageTooSmall(7, 7, 8)),
    ));
}
