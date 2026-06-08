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
fn larger_pnorm_strictly_increases_distance() {
    // The p-norm pooling is a power mean, strictly increasing in p whenever the
    // diffmap is non-constant (which a real distortion guarantees). Asserting a
    // *strict* ordering across three exponents is what catches a `pnorm` that is
    // silently ignored or hard-coded: with `>=`, a wrapper that pinned the
    // exponent at 3.0 would still pass, since equal values satisfy it.
    let base = Srgb8::base(64, 64);
    let dist = Srgb8::distort(&base, 30.0);
    let at = |pnorm| {
        butteraugli(
            &base,
            &dist,
            ButteraugliOptions {
                pnorm,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let (one, three, twelve) = (at(1.0), at(3.0), at(12.0));
    assert!(
        one < three && three < twelve,
        "p-norm pooling not strictly increasing in p: p1={one}, p3={three}, p12={twelve}",
    );
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

#[test]
fn hf_asymmetry_changes_the_distance() {
    // `hf_asymmetry` is the one tuning parameter the reference cross-validation
    // can't exercise (it pins only the defaults), so this is the sole guard that
    // it is wired through to libjxl at all. A distortion that injects new
    // high-frequency content is penalized more heavily as the asymmetry rises
    // above the neutral 1.0; a wrapper that dropped the parameter on the floor
    // (or hard-coded it) would return the same score for both.
    //
    // Only the >1.0 side is asserted: the pooled distance is *not* monotonic in
    // hf_asymmetry (1.0 sits near a local minimum, since the diffmap mixes added
    // and removed detail), so a value below 1.0 is deliberately not compared.
    let base = Srgb8::base(64, 64);
    let dist = Srgb8::distort(&base, 30.0);
    let neutral = butteraugli(
        &base,
        &dist,
        ButteraugliOptions {
            hf_asymmetry: 1.0,
            ..Default::default()
        },
    )
    .unwrap();
    let weighted = butteraugli(
        &base,
        &dist,
        ButteraugliOptions {
            hf_asymmetry: 2.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        weighted > neutral,
        "raising hf_asymmetry 1.0->2.0 did not increase the penalty on new \
         high-frequency detail: neutral={neutral}, weighted={weighted}",
    );
}

#[test]
fn argument_order_matters() {
    // Butteraugli's masking is derived from the reference image, so the metric
    // is asymmetric: `d(a, b) != d(b, a)`. This pins that asymmetry directly --
    // a wrapper that accidentally symmetrized the metric (e.g. averaging both
    // directions, or feeding the same buffer to both planes) would collapse the
    // gap and fail. The complementary failure, silently *swapping* reference and
    // distorted, is caught by the reference goldens, which fix the direction:
    // the ~9% gap here is comfortably outside the 1% band that test allows.
    let reference = chroma_bands(64, 64);
    let distorted = Srgb8::distort(&reference, 25.0);
    let forward = butteraugli(&reference, &distorted, ButteraugliOptions::default()).unwrap();
    let backward = butteraugli(&distorted, &reference, ButteraugliOptions::default()).unwrap();
    assert!(
        (forward - backward).abs() > 0.01 * forward,
        "expected an order-dependent (asymmetric) distance, got \
         forward={forward}, backward={backward}",
    );
}

#[test]
fn grayscale_equals_replicated_rgb() {
    // A grayscale image reaches the metric by replicating its single channel to
    // R=G=B (in `Image::to_rgb_f32_normalized`). Feeding an sRGB image whose
    // three channels already hold that same gray value must therefore produce a
    // *bit-identical* packed buffer, and so a bit-identical score. This pins the
    // grayscale-expansion path end to end; a transposed or mis-strided
    // replication would diverge from the explicit RGB build.
    let gray_ref = gray8(48, 40, gray_pattern);
    let gray_dist = Gray8::distort(&gray_ref, 18.0);
    let rgb_ref = replicate_gray(&gray_ref);
    let rgb_dist = replicate_gray(&gray_dist);

    let gray_score = butteraugli(&gray_ref, &gray_dist, ButteraugliOptions::default()).unwrap();
    let rgb_score = butteraugli(&rgb_ref, &rgb_dist, ButteraugliOptions::default()).unwrap();
    assert_eq!(
        gray_score.to_bits(),
        rgb_score.to_bits(),
        "grayscale score {gray_score} differs from the replicated-RGB score {rgb_score}",
    );
}

#[test]
fn score_is_invariant_to_bit_depth() {
    // The `/max` normalization in `Image::to_rgb_f32_normalized` divides by the
    // sample type's full range (255 for 8-bit, 65535 for 16-bit). The shared
    // fixtures scale the 8-bit design domain by exactly 257 to reach 16-bit, so
    // 255*257 == 65535: the *same* content at either depth must normalize to the
    // (essentially) same floats and score the same. A normalization that used a
    // fixed 255 divisor for 16-bit would inflate the input ~257x and shift the
    // score by orders of magnitude -- this catches that. Not bit-exact: the f32
    // rounding of `v/255` vs `v*257/65535` differs in the last place and the
    // metric's nonlinearities amplify it slightly, so a small tolerance applies.
    //
    // The width is deliberately odd, so the native image always carries SIMD row
    // padding -- this also exercises the shim's padding initialization (a buggy,
    // uninitialized padding is what first surfaced through this invariant). That
    // failure mode is heap-state dependent and so not reliably caught by a value
    // assertion in a warm test run; the durable guard is the shim itself.
    let ref8 = Srgb8::base(41, 40);
    let dist8 = Srgb8::distort(&ref8, 20.0);
    let ref16 = Srgb16::base(41, 40);
    let dist16 = Srgb16::distort(&ref16, 20.0);

    let score8 = butteraugli(&ref8, &dist8, ButteraugliOptions::default()).unwrap();
    let score16 = butteraugli(&ref16, &dist16, ButteraugliOptions::default()).unwrap();
    let tol = 3e-3 + 0.02 * score8;
    assert!(
        (score8 - score16).abs() <= tol,
        "8-bit ({score8}) and 16-bit ({score16}) scores for identical content \
         diverged by more than {tol}",
    );
}

/// Mid-key gray ramp, kept clear of clipping so the largest distortion the
/// suite applies never needs clamping (mirrors the design domain in `common`).
fn gray_pattern(x: u32, y: u32) -> u8 {
    (96 + ((x * 2 + y * 3) % 64)) as u8
}

/// Expands a grayscale fixture into an sRGB image with R=G=B set to the gray
/// value, the explicit form of the metric's internal replication.
fn replicate_gray(gray: &iqa::Image<Gray8>) -> iqa::Image<Srgb8> {
    let (w, h) = gray.dimensions();
    let samples = gray.samples();
    srgb8(w, h, |x, y, _| {
        samples[y as usize * w as usize + x as usize]
    })
}

/// Saturated vertical color bands: a strongly chromatic reference so the
/// asymmetry it induces in Butteraugli's masking is large enough to assert on.
fn chroma_bands(width: u32, height: u32) -> Srgb8Img {
    const PALETTE: [[u8; 3]; 4] = [[255, 10, 10], [10, 255, 10], [10, 10, 255], [240, 240, 10]];
    srgb8(width, height, |x, _, c| PALETTE[((x / 8) % 4) as usize][c])
}

type Srgb8Img = iqa::Image<Srgb8>;
