//! Universal property battery for full-reference IQA metrics.
//!
//! Every test here runs against *all* metrics registered in
//! [`common::metrics`]. The assertions encode rules that any correct
//! full-reference metric must satisfy regardless of its algorithm, so a new
//! metric implementation inherits the whole battery for free — and cannot be
//! considered correct until it passes every test below.

mod common;

use common::*;
use iqa_rs::Error;

/// Image kinds a metric must treat as perfectly identical to themselves.
fn identity_fixtures() -> Vec<(&'static str, iqa_rs::Image)> {
    vec![
        ("rgb8 gradient", base_rgb8(24, 24)),
        ("rgb8 solid", rgb8(24, 24, |_, _, _| 128)),
        ("grayscale", gray8(24, 24, |x, y| (x * 4 + y * 3) as u8)),
        ("rgb16 gradient", base_rgb16(24, 24)),
    ]
}

#[test]
fn identical_inputs_yield_the_perfect_score() {
    for spec in metrics() {
        for (label, img) in identity_fixtures() {
            let score = (spec.compute)(&img, &img)
                .unwrap_or_else(|e| panic!("{}: {label}: unexpected error: {e}", spec.name));
            assert!(
                spec.matches_identity(score),
                "{}: {label}: identical inputs scored {score}, expected {}",
                spec.name,
                spec.identity_score,
            );
        }
    }
}

#[test]
fn computation_is_deterministic() {
    let reference = base_rgb8(32, 32);
    let distorted = distort_rgb8(&reference, 20.0);
    for spec in metrics() {
        let first = (spec.compute)(&reference, &distorted).unwrap();
        let second = (spec.compute)(&reference, &distorted).unwrap();
        assert_eq!(
            first.to_bits(),
            second.to_bits(),
            "{}: repeated computation produced {first} then {second}",
            spec.name,
        );
    }
}

#[test]
fn mismatched_dimensions_are_rejected() {
    let a = base_rgb8(32, 32);
    let b = base_rgb8(32, 24);
    for spec in metrics() {
        assert!(
            matches!((spec.compute)(&a, &b), Err(Error::DimensionMismatch { .. })),
            "{}: comparing 32x32 with 32x24 should fail with DimensionMismatch",
            spec.name,
        );
    }
}

#[test]
fn images_below_the_minimum_dimension_are_rejected() {
    for spec in metrics() {
        if spec.min_dim <= 1 {
            continue;
        }
        let d = spec.min_dim - 1;
        let tiny = base_rgb8(d, d);
        assert!(
            (spec.compute)(&tiny, &tiny).is_err(),
            "{}: accepted a {d}x{d} image below the {min}x{min} minimum",
            spec.name,
            min = spec.min_dim,
        );
    }
}

#[test]
fn more_distortion_never_improves_the_score() {
    // Amplitudes are well separated so the ordering is unambiguous.
    let amplitudes = [0.0, 8.0, 22.0, 55.0];
    let base = base_rgb8(32, 32);
    for spec in metrics() {
        let scores: Vec<f64> = amplitudes
            .iter()
            .map(|&amp| (spec.compute)(&base, &distort_rgb8(&base, amp)).unwrap())
            .collect();

        assert!(
            spec.matches_identity(scores[0]),
            "{}: zero distortion scored {}, expected the perfect score",
            spec.name,
            scores[0],
        );
        // Each further step of distortion must not improve quality.
        for pair in scores.windows(2) {
            assert!(
                spec.not_better(pair[1], pair[0]),
                "{}: increasing distortion improved the score: {scores:?}",
                spec.name,
            );
        }
        // And a clearly larger distortion is strictly worse.
        assert!(
            spec.strictly_better(scores[1], scores[3]),
            "{}: light distortion ({}) did not beat heavy distortion ({})",
            spec.name,
            scores[1],
            scores[3],
        );
    }
}

#[test]
fn distorted_scores_are_finite_and_bounded() {
    let base = base_rgb8(24, 24);
    for spec in metrics() {
        for amplitude in [5.0, 25.0, 70.0] {
            let score = (spec.compute)(&base, &distort_rgb8(&base, amplitude)).unwrap();
            assert!(
                score.is_finite(),
                "{}: distortion amplitude {amplitude} produced a non-finite score {score}",
                spec.name,
            );
            assert!(
                spec.not_better(score, spec.identity_score),
                "{}: distorted score {score} is better than the perfect score {}",
                spec.name,
                spec.identity_score,
            );
        }
    }
}

#[test]
fn symmetric_metrics_ignore_argument_order() {
    let a = base_rgb8(24, 24);
    let b = distort_rgb8(&a, 18.0);
    for spec in metrics().into_iter().filter(|s| s.symmetric) {
        let forward = (spec.compute)(&a, &b).unwrap();
        let backward = (spec.compute)(&b, &a).unwrap();
        assert_eq!(
            forward.to_bits(),
            backward.to_bits(),
            "{}: declared symmetric but compute(a,b)={forward} != compute(b,a)={backward}",
            spec.name,
        );
    }
}

#[test]
fn alpha_channel_is_ignored() {
    // Two RGBA images with identical color planes but different alpha must be
    // treated as identical.
    let opaque = rgba8(24, 24, |x, y, c| {
        if c == 3 {
            255
        } else {
            gradient_sample(x, y, c)
        }
    });
    let transparent = rgba8(24, 24, |x, y, c| {
        if c == 3 {
            (x * 7 + y) as u8
        } else {
            gradient_sample(x, y, c)
        }
    });
    for spec in metrics() {
        let score = (spec.compute)(&opaque, &transparent).unwrap();
        assert!(
            spec.matches_identity(score),
            "{}: changing only the alpha channel changed the score to {score}",
            spec.name,
        );
    }
}

#[test]
fn sixteen_bit_images_are_supported() {
    let base = base_rgb16(24, 24);
    for spec in metrics() {
        let identity = (spec.compute)(&base, &base).unwrap();
        assert!(
            spec.matches_identity(identity),
            "{}: identical 16-bit images scored {identity}",
            spec.name,
        );
        let worse = (spec.compute)(&base, &distort_rgb16(&base, 1_200.0)).unwrap();
        assert!(
            spec.strictly_better(identity, worse),
            "{}: distortion of a 16-bit image was not detected ({identity} vs {worse})",
            spec.name,
        );
    }
}
