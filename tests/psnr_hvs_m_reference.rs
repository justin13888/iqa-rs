//! Numerical cross-validation of `iqa::psnr_hvs_m` against an independent
//! reference.
//!
//! Unlike the property battery (which checks algorithm-agnostic invariants),
//! this pins absolute PSNR-HVS-M values on fixed inputs, so a transcription or
//! algebra slip in the port — a wrong CSF/mask entry, a misnormalized DCT, the
//! masking threshold applied to the wrong coefficients — moves the number out of
//! band and fails here.
//!
//! The committed [`GOLDENS`] are produced by `scripts/gen-psnrhvs-goldens.sh`,
//! which runs the **original reference implementation** — Ponomarenko's
//! `psnrhvsm.m` — under Octave on these exact fixtures. `iqa::psnr_hvs_m` is a
//! direct port of that script, so this pins our output to the canonical source
//! of truth, not a paraphrase of it. `scripts/gen-psnrhvs-goldens.py` is an
//! independent NumPy reimplementation kept as a no-Octave cross-check; it
//! reproduces the same values. The fixtures are **grayscale**, so the luma path
//! has no color-conversion freedom and a single plane is compared.
//!
//! Fixtures are written by the `#[ignore]`d [`write_psnr_hvs_m_fixtures`] below;
//! if you regenerate them you must regenerate the goldens too.
#![cfg(feature = "psnr-hvs-m")]

mod common;

use std::path::{Path, PathBuf};

use common::{Fixture, Gray8, Image, gray8};
use iqa::{PsnrHvsMode, PsnrHvsOptions, psnr_hvs_m};

/// Committed PGM fixtures live here, relative to the crate root.
const FIXTURE_DIR: &str = "tests/fixtures/psnr_hvs_m";

/// `(case, reference.pgm, distorted.pgm)`, kept in lockstep with the generator.
/// All fixtures are 64x64 (64 non-overlapping 8x8 blocks).
const CASES: &[(&str, &str, &str)] = &[
    ("gradient_lo", "gradient_ref.pgm", "gradient_lo.pgm"),
    ("gradient_hi", "gradient_ref.pgm", "gradient_hi.pgm"),
    ("solid_dist", "solid_ref.pgm", "solid_dist.pgm"),
    ("texture_dist", "texture_ref.pgm", "texture_dist.pgm"),
];

/// PSNR-HVS-M (dB) from the reference `psnrhvsm.m`, via
/// `scripts/gen-psnrhvs-goldens.sh`. Regenerate after changing the fixtures.
const GOLDENS: &[(&str, f64)] = &[
    ("gradient_lo", 38.018849),
    ("gradient_hi", 23.581020),
    ("solid_dist", 40.924111),
    ("texture_dist", 33.602163),
];

#[test]
fn matches_reference_psnr_hvs_m() {
    for &(label, ref_pgm, dist_pgm) in CASES {
        let reference = load(&fixture_path(ref_pgm));
        let distorted = load(&fixture_path(dist_pgm));

        // Grayscale input: the two modes coincide, so either pins the luma path.
        let ours = psnr_hvs_m(
            &reference,
            &distorted,
            PsnrHvsOptions {
                mode: PsnrHvsMode::Luma709,
            },
        )
        .unwrap_or_else(|e| panic!("{label}: psnr_hvs_m errored: {e}"));
        let golden = golden(label);

        // 1% relative + a small absolute floor, matching the butteraugli
        // cross-check: enough slack for cross-target float divergence, far
        // tighter than the tens-of-percent a real algorithm bug would cause.
        // (On the generating host our output matches the reference exactly.)
        let tol = 1e-3 + 0.01 * golden.abs();
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
                 Run `cargo test --features psnr-hvs-m --test psnr_hvs_m_reference \
                 write_psnr_hvs_m_fixtures -- --ignored` to (re)generate fixtures.",
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
/// source of truth. All are 64x64 8-bit grayscale; distortions reuse the
/// deterministic `Fixture::distort` from `tests/common`.
fn fixtures() -> Vec<(&'static str, Image<Gray8>)> {
    let gradient = Gray8::base(64, 64);
    let solid = Gray8::solid(64, 64, 128.0);
    let texture = gray8(64, 64, texture_sample);
    vec![
        ("gradient_ref.pgm", Gray8::base(64, 64)),
        ("gradient_lo.pgm", Gray8::distort(&gradient, 10.0)),
        ("gradient_hi.pgm", Gray8::distort(&gradient, 40.0)),
        ("solid_ref.pgm", Gray8::solid(64, 64, 128.0)),
        ("solid_dist.pgm", Gray8::distort(&solid, 5.0)),
        ("texture_ref.pgm", gray8(64, 64, texture_sample)),
        ("texture_dist.pgm", Gray8::distort(&texture, 20.0)),
    ]
}

/// A high-frequency grayscale texture, so blocks carry the AC energy that the
/// contrast-masking step acts on.
fn texture_sample(x: u32, y: u32) -> u8 {
    let (fx, fy) = (x as f64, y as f64);
    let v = 128.0 + 70.0 * (fx * 0.42).sin() * (fy * 0.31).cos() + 25.0 * ((x ^ y) % 13) as f64;
    v.round().clamp(8.0, 247.0) as u8
}

/// Rewrites the committed PGM fixtures from the deterministic generators above.
///
/// Ignored by default: it mutates checked-in files. Run it only to regenerate
/// fixtures, and regenerate the goldens afterwards via
/// `scripts/gen-psnrhvs-goldens.py`.
#[test]
#[ignore = "regenerates committed PGM fixtures; rerun scripts/gen-psnrhvs-goldens.py after"]
fn write_psnr_hvs_m_fixtures() {
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
