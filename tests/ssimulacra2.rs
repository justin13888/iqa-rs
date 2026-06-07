//! SSIMULACRA2-specific black-box tests.
//!
//! The universal property battery lives in `tests/properties.rs`; this file
//! pins behaviour particular to SSIMULACRA2 (its exact perfect score, its
//! `0..=100` range, and the 8x8 minimum). The whole file compiles to nothing
//! unless the `ssimulacra2` feature is enabled.
#![cfg(feature = "ssimulacra2")]

mod common;

use common::*;
use iqa::{Error, ssimulacra2};

#[test]
fn identical_image_scores_exactly_100() {
    let img = Srgb8::base(64, 64);
    assert_eq!(ssimulacra2(&img, &img).unwrap(), 100.0);
}

#[test]
fn grayscale_input_is_accepted() {
    // Compiles only because `Gray8: Ssimulacra2Input`; grayscale is replicated
    // to R=G=B internally.
    let img = gray8(32, 32, |x, y| (x * 4 + y * 3) as u8);
    assert_eq!(ssimulacra2(&img, &img).unwrap(), 100.0);
}

#[test]
fn score_never_exceeds_100() {
    let base = Srgb8::base(64, 64);
    for amplitude in [4.0, 16.0, 48.0] {
        let score = ssimulacra2(&base, &Srgb8::distort(&base, amplitude)).unwrap();
        assert!(
            score <= 100.0 && score.is_finite(),
            "amplitude {amplitude}: score {score} is out of range",
        );
    }
}

#[test]
fn quality_decreases_with_distortion() {
    let base = Srgb8::base(64, 64);
    let light = ssimulacra2(&base, &Srgb8::distort(&base, 6.0)).unwrap();
    let heavy = ssimulacra2(&base, &Srgb8::distort(&base, 60.0)).unwrap();
    assert!(
        light > heavy,
        "light distortion {light} should beat heavy {heavy}"
    );
    assert!(
        heavy < 100.0,
        "heavy distortion {heavy} should be clearly below 100"
    );
}

#[test]
fn images_below_8x8_are_rejected() {
    let tiny = Srgb8::base(7, 7);
    assert!(matches!(
        ssimulacra2(&tiny, &tiny),
        Err(Error::ImageTooSmall(7, 7, 8)),
    ));
}
