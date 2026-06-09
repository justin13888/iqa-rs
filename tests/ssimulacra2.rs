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

// ---------------------------------------------------------------------------
// Odd-width / SIMD row-padding coverage
//
// The `MakeBundle` shim copies packed RGB into a `jxl::Image3F`, whose rows
// Highway pads up to a SIMD-vector multiple, and edge-clamps that padding. In the
// sibling butteraugli shim that fill fixed an observable bug (its convolutions
// read past the row). SSIMULACRA2 is different: every read of the image clamps to
// `xsize()-1` (`Downsample` in ssimulacra2.cc), so the padding is not read and
// the score does not depend on it — these tests pass with or without the fill.
// 35 is deliberately odd (never a SIMD multiple), so the native image always
// carries row padding; the value of these tests is pinning the odd-width
// packing/normalization paths and guarding against a future regression that lets
// padding leak into the result.
// ---------------------------------------------------------------------------

#[test]
fn identical_odd_width_scores_exactly_100() {
    // An odd-width identity case: pins that the packing/stride path produces a
    // bit-identical bundle (hence exactly 100) when the native image carries SIMD
    // row padding. Would also catch a regression that started reading mismatched
    // padding from the two separate `Image3F` allocations.
    let img = Srgb8::base(35, 33);
    assert_eq!(ssimulacra2(&img, &img).unwrap(), 100.0);
}

#[test]
fn score_ignores_row_padding() {
    // Compute the same odd-width score repeatedly, scribbling non-zero bytes over
    // freshly-freed heap between calls so a reused `Image3F`'s padding lanes are
    // unlikely to come back zeroed. The score must be bit-identical every time.
    // SSIMULACRA2 clamps its reads to `xsize()-1`, so the padding never enters the
    // result and this holds regardless of heap state; the test is a guard against
    // a regression (here or in libjxl) that started reading uninitialized padding,
    // which on a dirty-memory allocator would surface as nondeterminism. The shim
    // edge-clamps the padding regardless, so the result stays defined either way.
    let reference = Srgb8::base(35, 33);
    let distorted = Srgb8::distort(&reference, 20.0);

    let first = ssimulacra2(&reference, &distorted).unwrap();
    for i in 0..16u8 {
        // A spread of small sizes around the per-plane Image3F allocation, so the
        // next bundle is likely to reuse one of these dirtied blocks.
        for sz in [4096usize, 8192, 16384, 65536] {
            let mut scratch = vec![0xA5u8.wrapping_add(i); sz];
            std::hint::black_box(&mut scratch);
            drop(std::hint::black_box(scratch));
        }

        let again = ssimulacra2(&reference, &distorted).unwrap();
        assert_eq!(
            first.to_bits(),
            again.to_bits(),
            "iteration {i}: score {again} != first {first}; row padding leaked into the result",
        );
    }
}

#[test]
fn score_is_invariant_to_bit_depth() {
    // 255 * 257 == 65535, so the shared fixtures' identical 8-/16-bit content
    // normalizes (in `Image::to_rgb_f32_normalized`) to essentially the same
    // floats and must score the same. The odd width additionally exercises the
    // shim's padding initialization at a non-vector-aligned width. Not bit-exact:
    // f32 rounding of `v/255` vs `v*257/65535` differs in the last place and
    // SSIMULACRA2's nonlinearities amplify it slightly, so a small tolerance
    // applies.
    let ref8 = Srgb8::base(35, 33);
    let dist8 = Srgb8::distort(&ref8, 20.0);
    let ref16 = Srgb16::base(35, 33);
    let dist16 = Srgb16::distort(&ref16, 20.0);

    let score8 = ssimulacra2(&ref8, &dist8).unwrap();
    let score16 = ssimulacra2(&ref16, &dist16).unwrap();
    let tol = 0.1 + 0.01 * score8.abs();
    eprintln!(
        "bit-depth: 8-bit={score8:.6} 16-bit={score16:.6} (Δ={:.6})",
        (score8 - score16).abs()
    );
    assert!(
        (score8 - score16).abs() <= tol,
        "8-bit ({score8}) and 16-bit ({score16}) scores for identical content \
         diverged by more than {tol}",
    );
}
