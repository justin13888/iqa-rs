//! Numerical cross-validation of `iqa::msssim` against an independent reference.
//!
//! Unlike the property battery (which checks algorithm-agnostic invariants),
//! this pins absolute MS-SSIM values on fixed inputs, so a transcription or
//! algebra slip in the port — wrong weight, misaligned downsample, a swapped
//! contrast-structure term — moves the number out of band and fails here.
//!
//! The committed [`GOLDENS`] are produced by `scripts/gen-msssim-goldens.py`, an
//! **independent NumPy reimplementation** of the published five-scale MS-SSIM
//! (Wang/Simoncelli/Bovik 2003, the same algorithm Daala's `tools/dump_msssim.c`
//! implements). It is deliberately written from the paper — a different language,
//! a different convolution, and the textbook `E[x²]−E[x]²` variance form rather
//! than this crate's deviation form — so agreement is real evidence, not a
//! restatement of our own arithmetic. The fixtures are **grayscale**, so the
//! luma path has no color-conversion freedom and a single plane is compared.
//!
//! TODO(authoritative): re-pin these values against the Daala `dump_msssim`
//! binary itself. That build is autotools-based (needs `autoconf`/`automake`,
//! plus `ffmpeg` to wrap the grayscale fixtures as luma Y4M); the recipe lives
//! in the header of `scripts/gen-msssim-goldens.py`. Until then the MS-SSIM row
//! in the README stays "Require testing".
//!
//! Fixtures are written by the `#[ignore]`d [`write_ms_ssim_fixtures`] below; if
//! you regenerate them you must regenerate the goldens too.
#![cfg(feature = "ms-ssim")]

mod common;

use std::path::{Path, PathBuf};

use common::{Fixture, Gray8, Image, gray8};
use iqa::{MsssimOptions, SsimMode, msssim};

/// Committed PGM fixtures live here, relative to the crate root.
const FIXTURE_DIR: &str = "tests/fixtures/ms_ssim";

/// `(case, reference.pgm, distorted.pgm)`, kept in lockstep with the generator.
/// All fixtures are 192x192 — the smallest size at which all five MS-SSIM scales
/// run (the coarsest is 11x11), so the goldens exercise the full pyramid.
const CASES: &[(&str, &str, &str)] = &[
    ("gradient_lo", "gradient_ref.pgm", "gradient_lo.pgm"),
    ("gradient_hi", "gradient_ref.pgm", "gradient_hi.pgm"),
    ("solid_dist", "solid_ref.pgm", "solid_dist.pgm"),
    ("texture_dist", "texture_ref.pgm", "texture_dist.pgm"),
];

/// Independent five-scale MS-SSIM from `scripts/gen-msssim-goldens.py`.
/// Regenerate with that script after changing the fixtures.
const GOLDENS: &[(&str, f64)] = &[
    ("gradient_lo", 0.985502),
    ("gradient_hi", 0.859923),
    ("solid_dist", 0.981648),
    ("texture_dist", 0.992655),
];

#[test]
fn matches_reference_msssim() {
    for &(label, ref_pgm, dist_pgm) in CASES {
        let reference = load(&fixture_path(ref_pgm));
        let distorted = load(&fixture_path(dist_pgm));

        // Grayscale input: the two modes coincide, so either pins the luma path.
        let ours = msssim(
            &reference,
            &distorted,
            MsssimOptions {
                mode: SsimMode::Luma709,
            },
        )
        .unwrap_or_else(|e| panic!("{label}: msssim errored: {e}"));
        let golden = golden(label);

        // 1% relative + a small absolute floor, matching the butteraugli
        // cross-check: enough slack for cross-implementation float divergence
        // (and a future re-pin against the Daala binary), far tighter than the
        // tens-of-percent a real algorithm bug would cause.
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
                 Run `cargo test --features ms-ssim --test ms_ssim_reference \
                 write_ms_ssim_fixtures -- --ignored` to (re)generate fixtures.",
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
    let solid = Gray8::solid(192, 192, 128.0);
    let texture = gray8(192, 192, texture_sample);
    vec![
        ("gradient_ref.pgm", Gray8::base(192, 192)),
        ("gradient_lo.pgm", Gray8::distort(&gradient, 10.0)),
        ("gradient_hi.pgm", Gray8::distort(&gradient, 40.0)),
        ("solid_ref.pgm", Gray8::solid(192, 192, 128.0)),
        ("solid_dist.pgm", Gray8::distort(&solid, 5.0)),
        ("texture_ref.pgm", gray8(192, 192, texture_sample)),
        ("texture_dist.pgm", Gray8::distort(&texture, 20.0)),
    ]
}

/// A high-frequency grayscale texture, so the pyramid's coarse scales see
/// structure that the fine scales do — exercising the multi-scale combination
/// rather than a single dominant scale.
fn texture_sample(x: u32, y: u32) -> u8 {
    let (fx, fy) = (x as f64, y as f64);
    let v = 128.0 + 70.0 * (fx * 0.42).sin() * (fy * 0.31).cos() + 25.0 * ((x ^ y) % 13) as f64;
    v.round().clamp(8.0, 247.0) as u8
}

/// Rewrites the committed PGM fixtures from the deterministic generators above.
///
/// Ignored by default: it mutates checked-in files. Run it only to regenerate
/// fixtures, and regenerate the goldens afterwards via
/// `scripts/gen-msssim-goldens.py`.
#[test]
#[ignore = "regenerates committed PGM fixtures; rerun scripts/gen-msssim-goldens.py after"]
fn write_ms_ssim_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    for (name, img) in fixtures() {
        // Hand-written binary PGM (P5): uncompressed 8-bit grayscale, read
        // identically by the `image` crate and any reference tool, so neither
        // side can disagree about the pixels.
        let mut bytes = format!("P5\n{} {}\n255\n", img.width(), img.height()).into_bytes();
        bytes.extend_from_slice(img.samples());
        std::fs::write(dir.join(name), bytes).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }
    eprintln!("wrote {} fixtures to {}", fixtures().len(), dir.display());
}
