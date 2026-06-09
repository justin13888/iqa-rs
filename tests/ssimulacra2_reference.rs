//! Numerical cross-validation of `iqa::ssimulacra2` against the upstream
//! SSIMULACRA2 reference tool.
//!
//! Our FFI shim calls the vendored cloudinary `ComputeSSIMULACRA2`, so the metric
//! is canonical *by construction* — but the Rust wrapper adds steps a
//! property/relative test can't catch if they silently corrupt the number: sRGB
//! color tagging, `/255` normalization, interleaved→planar packing, and the SIMD
//! row-padding fill. This file pins the absolute values against the `ssimulacra2`
//! reference tool built from the **exact cloudinary source we vendor**
//! (`third_party/ssimulacra2/src`), on identical pixels.
//!
//! The version matters: SSIMULACRA2 changed numerically from 2.0 to **2.1** (April
//! 2023 — rescaled XYB and retuned weights), so a libjxl release old enough to
//! build self-contained (≤ v0.8.2, used for the butteraugli oracle) ships 2.0 and
//! scores these fixtures very differently (e.g. gradient_lo: 19.3 under 2.0 vs
//! 53.8 under our 2.1). The vendored submodule is the only oracle guaranteed to be
//! the same algorithm version our shim binds; its `ssimulacra2_main.cc` still
//! loads images through its own libjxl path (independent of our shim's
//! `MakeBundle`), so this remains a real cross-check.
//!
//! `ssimulacra2 original.png distorted.png` prints one line: the score (`-inf..100`,
//! 100 = identical). It loads images via libjxl's built-in PNM decoder, so binary
//! PPM (P6) — uncompressed 8-bit RGB, read as nonlinear sRGB on both sides — lets
//! us feed both the metric and the reference the *same committed `.ppm` files*,
//! isolating exactly what our wrapper adds. (PPM, not PNG, because the reference
//! tool's PNG codec needs system libpng; its PNM reader is built in.)
//!
//! The committed [`GOLDENS`] were produced by `scripts/gen-ssimulacra2-goldens.sh`
//! (which builds the tool and prints this table). The fixtures are written by the
//! `#[ignore]`d [`write_ssimulacra2_fixtures`] below; if you regenerate them you
//! must regenerate the goldens too.
#![cfg(feature = "ssimulacra2")]

mod common;

use std::path::{Path, PathBuf};

use common::{Fixture, Image, Srgb8, srgb8};
use iqa::ssimulacra2;

/// Committed PPM fixtures live here, relative to the crate root.
const FIXTURE_DIR: &str = "tests/fixtures/ssimulacra2";

/// `(case, reference.ppm, distorted.ppm)`. SSIMULACRA2 is asymmetric, so the
/// reference-first order must match the `ssimulacra2 <original> <distorted>`
/// order used to generate the goldens.
const CASES: &[(&str, &str, &str)] = &[
    ("gradient_lo", "gradient_ref.ppm", "gradient_lo.ppm"),
    ("gradient_hi", "gradient_ref.ppm", "gradient_hi.ppm"),
    ("chroma_dist", "chroma_ref.ppm", "chroma_dist.ppm"),
    ("oddwidth_dist", "oddwidth_ref.ppm", "oddwidth_dist.ppm"),
];

/// Reference scores from libjxl v0.8.2 `ssimulacra2` (SSIMULACRA 2.1, no params).
/// Regenerate with `scripts/gen-ssimulacra2-goldens.sh`.
const GOLDENS: &[(&str, f64)] = &[
    ("gradient_lo", 53.78061578),
    ("gradient_hi", -9.72360450),
    ("chroma_dist", 50.83180715),
    ("oddwidth_dist", 19.06314962),
];

#[test]
fn matches_cloudinary_ssimulacra2() {
    for &(label, ref_ppm, dist_ppm) in CASES {
        let reference = load(&fixture_path(ref_ppm));
        let distorted = load(&fixture_path(dist_ppm));

        let ours = ssimulacra2(&reference, &distorted)
            .unwrap_or_else(|e| panic!("{label}: ssimulacra2 errored: {e}"));
        let golden = golden(label);

        // 1% relative + a small absolute floor. The slack is for cross-target
        // floating-point / SIMD divergence (goldens were generated on one CPU;
        // CI runs another, and libjxl dispatches SIMD per target via Highway).
        // A wrong color tag / missing gamma / transposed packing would shift the
        // value by many points — far outside this band.
        let tol = 1e-3 + 0.01 * golden.abs();
        let delta = (ours - golden).abs();
        // Captured by default; surfaces with `--nocapture` (or on failure).
        eprintln!("{label}: ours={ours:.6} cloudinary={golden:.6} (Δ={delta:.6})");
        assert!(
            delta <= tol,
            "{label}: ours={ours:.6} cloudinary={golden:.6} (|Δ|={delta:.6} > tol={tol:.6})",
        );
    }
}

/// Decodes a committed fixture into the `iqa` input type, exactly as a caller
/// would (mirrors `examples/compare.rs`, here for PPM).
fn load(path: &Path) -> Image<Srgb8> {
    let decoded = image::open(path)
        .unwrap_or_else(|e| {
            panic!(
                "open {}: {e}\n\
                 Run `cargo test --features ssimulacra2 --test ssimulacra2_reference \
                 write_ssimulacra2_fixtures -- --ignored` to (re)generate fixtures.",
                path.display(),
            )
        })
        .to_rgb8();
    let (width, height) = decoded.dimensions();
    Image::srgb8(width, height, decoded.into_raw()).expect("fixture dimensions match its buffer")
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
/// and the reference `ssimulacra2` tool consume these exact files, so they are
/// the single source of truth. Distortions reuse the deterministic
/// `Fixture::distort` from `tests/common`. The 64x64 cases are vector-aligned
/// (no SIMD row padding); the 65x65 `oddwidth` pair is deliberately non-aligned
/// to cross-validate the shim's padding fill against the reference.
fn fixtures() -> Vec<(&'static str, Image<Srgb8>)> {
    let gradient = Srgb8::base(64, 64);
    let chroma = srgb8(64, 64, chroma_sample);
    let oddwidth = Srgb8::base(65, 65);
    vec![
        ("gradient_ref.ppm", Srgb8::base(64, 64)),
        ("gradient_lo.ppm", Srgb8::distort(&gradient, 10.0)),
        ("gradient_hi.ppm", Srgb8::distort(&gradient, 40.0)),
        ("chroma_ref.ppm", srgb8(64, 64, chroma_sample)),
        ("chroma_dist.ppm", Srgb8::distort(&chroma, 20.0)),
        ("oddwidth_ref.ppm", Srgb8::base(65, 65)),
        ("oddwidth_dist.ppm", Srgb8::distort(&oddwidth, 20.0)),
    ]
}

/// Saturated vertical color bands with a mild vertical brightness ramp, so the
/// chroma fixture exercises the chromatic (X / B-Y) channels rather than luma
/// alone.
fn chroma_sample(x: u32, y: u32, c: usize) -> u8 {
    const PALETTE: [[u8; 3]; 8] = [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [0, 255, 255],
        [255, 0, 255],
        [255, 128, 0],
        [128, 0, 255],
    ];
    let band = ((x / 8) % 8) as usize;
    let base = PALETTE[band][c] as f64;
    let scale = 0.7 + 0.3 * (y % 16) as f64 / 15.0;
    (base * scale).round().clamp(0.0, 255.0) as u8
}

/// Rewrites the committed PPM fixtures from the deterministic generators above.
///
/// Ignored by default: it mutates checked-in files. Run it only to regenerate
/// fixtures, and regenerate the goldens afterwards via
/// `scripts/gen-ssimulacra2-goldens.sh`.
#[test]
#[ignore = "regenerates committed PPM fixtures; rerun scripts/gen-ssimulacra2-goldens.sh after"]
fn write_ssimulacra2_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    for (name, img) in fixtures() {
        // Hand-written binary PPM (P6): trivial, and read identically by both
        // libjxl's built-in PNM decoder and the `image` crate, so neither side
        // can disagree about the pixels.
        let mut bytes = format!("P6\n{} {}\n255\n", img.width(), img.height()).into_bytes();
        bytes.extend_from_slice(img.samples());
        std::fs::write(dir.join(name), bytes).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }
    eprintln!("wrote {} fixtures to {}", fixtures().len(), dir.display());
}
