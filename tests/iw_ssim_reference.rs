//! Numerical cross-validation of `iqa::iwssim` against an independent reference.
//!
//! Unlike the property battery (which checks algorithm-agnostic invariants),
//! this pins absolute IW-SSIM values on fixed inputs, so a transcription or
//! algebra slip in the port — a misbuilt pyramid, a misaligned pooling crop, a
//! wrong GSM term — moves the number out of band and fails here.
//!
//! The committed [`GOLDENS`] are produced by `scripts/gen-iwssim-goldens.sh`,
//! which runs the **original reference implementation** — Wang & Li's `iwssim.m`
//! (with its `scale_quality_maps.m`, `info_content_weight_map.m`, `imenlarge2.m`
//! helpers and Simoncelli's matlabPyrTools) — under Octave on these exact
//! fixtures. `iqa::iwssim` reproduces that five-scale metric, so this pins our
//! output to the canonical source of truth, not a paraphrase of it.
//! `scripts/gen-iwssim-goldens.py` is an independent NumPy/`pyrtools`
//! reimplementation kept as a no-Octave cross-check; it reproduces the same
//! values. The fixtures are **grayscale**, so the luma path has no
//! color-conversion freedom and a single plane is compared.
//!
//! The references are non-degenerate (a gradient and a high-frequency texture):
//! IW-SSIM weights come from a model of the *reference*, so a flat reference
//! would make the reference implementation's covariance singular. All fixtures
//! are 192x192 — the smallest multiple-of-pyramid size at which all five
//! IW-SSIM scales run (the reference requires at least 176x176).
//!
//! Fixtures are written by the `#[ignore]`d [`write_iw_ssim_fixtures`] below; if
//! you regenerate them you must regenerate the goldens too.
#![cfg(feature = "iw-ssim")]

mod common;

use std::path::{Path, PathBuf};

use common::{Fixture, Gray8, Image, gray8};
use iqa::{IwssimOptions, SsimMode, iwssim};

/// Committed PGM fixtures live here, relative to the crate root.
const FIXTURE_DIR: &str = "tests/fixtures/iw_ssim";

/// `(case, reference.pgm, distorted.pgm)`, kept in lockstep with the generator.
const CASES: &[(&str, &str, &str)] = &[
    ("gradient_lo", "gradient_ref.pgm", "gradient_lo.pgm"),
    ("gradient_hi", "gradient_ref.pgm", "gradient_hi.pgm"),
    ("texture_lo", "texture_ref.pgm", "texture_lo.pgm"),
    ("texture_hi", "texture_ref.pgm", "texture_hi.pgm"),
];

/// Five-scale IW-SSIM from the reference `iwssim.m`, via
/// `scripts/gen-iwssim-goldens.sh`. Regenerate after changing the fixtures.
const GOLDENS: &[(&str, f64)] = &[
    ("gradient_lo", 0.970210),
    ("gradient_hi", 0.754273),
    ("texture_lo", 0.997193),
    ("texture_hi", 0.981140),
];

#[test]
fn matches_reference_iwssim() {
    for &(label, ref_pgm, dist_pgm) in CASES {
        let reference = load(&fixture_path(ref_pgm));
        let distorted = load(&fixture_path(dist_pgm));

        // Grayscale input: the two modes coincide, so either pins the luma path.
        let ours = iwssim(
            &reference,
            &distorted,
            IwssimOptions {
                mode: SsimMode::Luma709,
            },
        )
        .unwrap_or_else(|e| panic!("{label}: iwssim errored: {e}"));
        let golden = golden(label);

        // 1% relative + a small absolute floor, matching the MS-SSIM cross-check:
        // enough slack for cross-target float divergence, far tighter than the
        // tens-of-percent a real algorithm bug would cause. (On the generating
        // host our output matches the reference to within rounding.)
        let tol = 1e-3 + 0.01 * golden;
        let delta = (ours - golden).abs();
        eprintln!("{label}: ours={ours:.6} reference={golden:.6} (Δ={delta:.6})");
        assert!(
            delta <= tol,
            "{label}: ours={ours:.6} reference={golden:.6} (|Δ|={delta:.6} > tol={tol:.6})",
        );
    }
}

/// Decodes a committed grayscale fixture into the `iqa` input type.
fn load(path: &Path) -> Image<Gray8> {
    let decoded = image::open(path)
        .unwrap_or_else(|e| {
            panic!(
                "open {}: {e}\n\
                 Run `cargo test --features iw-ssim --test iw_ssim_reference \
                 write_iw_ssim_fixtures -- --ignored` to (re)generate fixtures.",
                path.display(),
            )
        })
        .to_luma8();
    let (width, height) = decoded.dimensions();
    Image::gray8(width, height, decoded.into_raw()).expect("fixture dimensions match its buffer")
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_DIR)
        .join(name)
}

fn golden(label: &str) -> f64 {
    GOLDENS
        .iter()
        .find(|(l, _)| *l == label)
        .map(|&(_, v)| v)
        .unwrap_or_else(|| panic!("no golden recorded for case `{label}`"))
}

// ---------------------------------------------------------------------------
// Fixture generation
// ---------------------------------------------------------------------------

/// The complete set of committed fixture images, by filename. Both the Rust test
/// and the reference generator consume these exact files, so they are the single
/// source of truth. All are 192x192 8-bit grayscale; distortions reuse the
/// deterministic `Fixture::distort` from `tests/common`.
fn fixtures() -> Vec<(&'static str, Image<Gray8>)> {
    let gradient = Gray8::base(192, 192);
    let texture = gray8(192, 192, texture_sample);
    vec![
        ("gradient_ref.pgm", Gray8::base(192, 192)),
        ("gradient_lo.pgm", Gray8::distort(&gradient, 10.0)),
        ("gradient_hi.pgm", Gray8::distort(&gradient, 40.0)),
        ("texture_ref.pgm", gray8(192, 192, texture_sample)),
        ("texture_lo.pgm", Gray8::distort(&texture, 10.0)),
        ("texture_hi.pgm", Gray8::distort(&texture, 30.0)),
    ]
}

/// A high-frequency grayscale texture, so the pyramid's coarse scales see
/// structure that the fine scales do — exercising the multi-scale combination
/// and a well-conditioned reference covariance for the GSM weighting.
fn texture_sample(x: u32, y: u32) -> u8 {
    let (fx, fy) = (x as f64, y as f64);
    let v = 128.0 + 70.0 * (fx * 0.42).sin() * (fy * 0.31).cos() + 25.0 * ((x ^ y) % 13) as f64;
    v.round().clamp(8.0, 247.0) as u8
}

/// Rewrites the committed PGM fixtures from the deterministic generators above.
///
/// Ignored by default: it mutates checked-in files. Run it only to regenerate
/// fixtures, and regenerate the goldens afterwards via
/// `scripts/gen-iwssim-goldens.sh` (and the `.py` cross-check).
#[test]
#[ignore = "regenerates committed PGM fixtures; rerun scripts/gen-iwssim-goldens.sh after"]
fn write_iw_ssim_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    for (name, img) in fixtures() {
        // Hand-written binary PGM (P5): uncompressed 8-bit grayscale, read
        // identically by the `image` crate and Octave's `imread`, so neither
        // side can disagree about the pixels.
        let mut bytes = format!("P5\n{} {}\n255\n", img.width(), img.height()).into_bytes();
        bytes.extend_from_slice(img.samples());
        std::fs::write(dir.join(name), bytes).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }
    eprintln!("wrote {} fixtures to {}", fixtures().len(), dir.display());
}
