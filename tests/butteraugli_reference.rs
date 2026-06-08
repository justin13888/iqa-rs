//! Numerical cross-validation of `iqa::butteraugli` against libjxl's own
//! reference tool.
//!
//! Our FFI shim calls libjxl's `ButteraugliDistance` → `ComputeDistanceP`, so
//! the metric is canonical *by construction* — but the Rust wrapper adds four
//! steps that a property/relative test can't catch if they silently corrupt the
//! number: sRGB color tagging, `/255` normalization, interleaved→planar packing,
//! and parameter wiring. This file pins the absolute values against libjxl
//! **v0.8.2** (the version vendored under `third_party/`), on identical pixels.
//!
//! `butteraugli_main ref.ppm dist.ppm` from v0.8.2 prints two lines: the
//! max-norm `ButteraugliDistance`, then `3-norm: <value>` from
//! `ComputeDistanceP(distmap, params, 3.0)`. With its defaults
//! (`intensity_target = 80`, `hf_asymmetry = 1.0`, `--pnorm` default `3`) — which
//! are exactly [`ButteraugliOptions::default`] — that `3-norm:` line is the value
//! we reproduce. Binary PPM (P6) is uncompressed 8-bit RGB and is read as
//! nonlinear sRGB on both sides, so feeding both the *same committed `.ppm`
//! files* isolates exactly what our wrapper adds. (PPM, not PNG, because the
//! reference tool's PNG codec needs system libpng; its PNM reader is built in.)
//!
//! The committed [`GOLDENS`] were produced by `scripts/gen-butteraugli-goldens.sh`
//! (which builds the tool and prints this table). The fixtures are written by the
//! `#[ignore]`d [`write_butteraugli_fixtures`] below; if you regenerate them you
//! must regenerate the goldens too.
#![cfg(feature = "butteraugli")]

mod common;

use std::path::{Path, PathBuf};

use common::{Fixture, Image, Srgb8, srgb8};
use iqa::{ButteraugliOptions, butteraugli};

/// Committed PPM fixtures live here, relative to the crate root.
const FIXTURE_DIR: &str = "tests/fixtures/butteraugli";

/// `(case, reference.ppm, distorted.ppm)`. Butteraugli is asymmetric, so the
/// reference-first order must match the `butteraugli_main <ref> <dist>` order
/// used to generate the goldens.
const CASES: &[(&str, &str, &str)] = &[
    ("gradient_lo", "gradient_ref.ppm", "gradient_lo.ppm"),
    ("gradient_hi", "gradient_ref.ppm", "gradient_hi.ppm"),
    ("solid_dist", "solid_ref.ppm", "solid_dist.ppm"),
    ("chroma_dist", "chroma_ref.ppm", "chroma_dist.ppm"),
];

/// Reference 3-norm values from libjxl v0.8.2 `butteraugli_main`, default params
/// (`intensity_target = 80`, `hf_asymmetry = 1.0`, `pnorm = 3`). Regenerate with
/// `scripts/gen-butteraugli-goldens.sh`.
const GOLDENS: &[(&str, f64)] = &[
    ("gradient_lo", 1.997707),
    ("gradient_hi", 6.692956),
    ("solid_dist", 1.739092),
    ("chroma_dist", 2.272068),
];

#[test]
fn matches_libjxl_butteraugli_main() {
    for &(label, ref_ppm, dist_ppm) in CASES {
        let reference = load(&fixture_path(ref_ppm));
        let distorted = load(&fixture_path(dist_ppm));

        let ours = butteraugli(&reference, &distorted, ButteraugliOptions::default())
            .unwrap_or_else(|e| panic!("{label}: butteraugli errored: {e}"));
        let golden = golden(label);

        // 1% relative + a small absolute floor. The slack is for cross-target
        // floating-point / SIMD divergence (goldens were generated on one CPU;
        // CI runs another, and libjxl dispatches SIMD per target via Highway).
        // A wrong color tag / missing gamma would shift the value by tens of
        // percent — far outside this band.
        let tol = 1e-3 + 0.01 * golden;
        let delta = (ours - golden).abs();
        // Captured by default; surfaces with `--nocapture` (or on failure).
        eprintln!("{label}: ours={ours:.6} libjxl-3-norm={golden:.6} (Δ={delta:.6})");
        assert!(
            delta <= tol,
            "{label}: ours={ours:.6} libjxl 3-norm={golden:.6} (|Δ|={delta:.6} > tol={tol:.6})",
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
                 Run `cargo test --features butteraugli --test butteraugli_reference \
                 write_butteraugli_fixtures -- --ignored` to (re)generate fixtures.",
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
/// and `butteraugli_main` consume these exact files, so they are the single
/// source of truth. All are 64x64 8-bit RGB; distortions reuse the deterministic
/// `Fixture::distort` from `tests/common`.
fn fixtures() -> Vec<(&'static str, Image<Srgb8>)> {
    let gradient = Srgb8::base(64, 64);
    let solid = Srgb8::solid(64, 64, 128.0);
    let chroma = srgb8(64, 64, chroma_sample);
    vec![
        ("gradient_ref.ppm", Srgb8::base(64, 64)),
        ("gradient_lo.ppm", Srgb8::distort(&gradient, 10.0)),
        ("gradient_hi.ppm", Srgb8::distort(&gradient, 40.0)),
        ("solid_ref.ppm", Srgb8::solid(64, 64, 128.0)),
        ("solid_dist.ppm", Srgb8::distort(&solid, 5.0)),
        ("chroma_ref.ppm", srgb8(64, 64, chroma_sample)),
        ("chroma_dist.ppm", Srgb8::distort(&chroma, 20.0)),
    ]
}

/// Saturated vertical color bands with a mild vertical brightness ramp, so the
/// chroma fixture exercises the opponent (X / B-Y) channels rather than luma
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
/// `scripts/gen-butteraugli-goldens.sh`.
#[test]
#[ignore = "regenerates committed PPM fixtures; rerun scripts/gen-butteraugli-goldens.sh after"]
fn write_butteraugli_fixtures() {
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
