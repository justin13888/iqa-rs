//! Numerical cross-validation of `iqa::ciede2000` against the `colour-science`
//! reference (the de-facto authority for color science in Python).
//!
//! The Sharma–Wu–Dalal test set (reproduced in `src/ciede2000.rs`'s inline
//! tests) pins the ΔE₀₀ *formula* on CIELAB inputs to four decimals. This file
//! pins the rest of the pipeline that those tests can't see: `/255`
//! normalization, the sRGB EOTF, the linear-sRGB→XYZ matrix, the XYZ→CIELAB
//! (D65) conversion, and the per-pixel mean. `scripts/gen-ciede2000-goldens.py`
//! runs `colour.sRGB_to_XYZ → colour.XYZ_to_Lab → colour.difference.
//! delta_E_CIE2000` over the same committed PPM fixtures and averages, so
//! feeding both sides the *same* `.ppm` files isolates exactly what our pipeline
//! computes. Binary PPM (P6) is uncompressed 8-bit sRGB, read identically by the
//! `image` crate and NumPy, so neither side can disagree about the pixels.
//!
//! The committed [`GOLDENS`] were produced by `scripts/gen-ciede2000-goldens.py`.
//! The fixtures are written by the `#[ignore]`d [`write_ciede2000_fixtures`]
//! below; if you regenerate them you must regenerate the goldens too.
#![cfg(feature = "ciede2000")]

mod common;

use std::path::{Path, PathBuf};

use common::{Fixture, Image, Srgb8, srgb8};
use iqa::{Ciede2000Options, ciede2000};

/// Committed PPM fixtures live here, relative to the crate root.
const FIXTURE_DIR: &str = "tests/fixtures/ciede2000";

/// `(case, reference.ppm, distorted.ppm)`. ΔE₀₀ is symmetric, but the
/// reference-first order mirrors the generator for clarity.
const CASES: &[(&str, &str, &str)] = &[
    ("gradient_lo", "gradient_ref.ppm", "gradient_lo.ppm"),
    ("gradient_hi", "gradient_ref.ppm", "gradient_hi.ppm"),
    ("solid_dist", "solid_ref.ppm", "solid_dist.ppm"),
    ("chroma_dist", "chroma_ref.ppm", "chroma_dist.ppm"),
];

/// Mean ΔE₀₀ from `colour-science` over the same fixtures. Regenerate with
/// `python3 scripts/gen-ciede2000-goldens.py`.
const GOLDENS: &[(&str, f64)] = &[
    ("gradient_lo", 4.603349),
    ("gradient_hi", 15.278680),
    ("solid_dist", 3.169049),
    ("chroma_dist", 2.970071),
];

#[test]
fn matches_colour_science_ciede2000() {
    for &(label, ref_ppm, dist_ppm) in CASES {
        let reference = load(&fixture_path(ref_ppm));
        let distorted = load(&fixture_path(dist_ppm));

        let ours = ciede2000(&reference, &distorted, Ciede2000Options::default())
            .unwrap_or_else(|e| panic!("{label}: ciede2000 errored: {e}"));
        let golden = golden(label);

        // 1% relative + a small absolute floor. The slack absorbs cross-target
        // floating-point divergence and the 4th–5th-digit differences between
        // colour-science's primaries-derived sRGB matrix / D65 white and ours. A
        // missing EOTF or a wrong matrix would shift the value by tens of percent
        // — far outside this band.
        let tol = 1e-3 + 0.01 * golden;
        let delta = (ours - golden).abs();
        // Captured by default; surfaces with `--nocapture` (or on failure).
        eprintln!("{label}: ours={ours:.6} colour-science={golden:.6} (Δ={delta:.6})");
        assert!(
            delta <= tol,
            "{label}: ours={ours:.6} colour-science={golden:.6} (|Δ|={delta:.6} > tol={tol:.6})",
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
                 Run `cargo test --features ciede2000 --test ciede2000_reference \
                 write_ciede2000_fixtures -- --ignored` to (re)generate fixtures.",
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
/// and `gen-ciede2000-goldens.py` consume these exact files, so they are the
/// single source of truth. All are 64x64 8-bit RGB; distortions reuse the
/// deterministic `Fixture::distort` from `tests/common`.
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
/// chroma fixture exercises a* / b* (hue and chroma) rather than lightness alone
/// — the part of ΔE₀₀ a color metric most needs to get right.
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
/// `python3 scripts/gen-ciede2000-goldens.py`.
#[test]
#[ignore = "regenerates committed PPM fixtures; rerun scripts/gen-ciede2000-goldens.py after"]
fn write_ciede2000_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    for (name, img) in fixtures() {
        // Hand-written binary PPM (P6): trivial, and read identically by both the
        // `image` crate and NumPy, so neither side can disagree about the pixels.
        let mut bytes = format!("P6\n{} {}\n255\n", img.width(), img.height()).into_bytes();
        bytes.extend_from_slice(img.samples());
        std::fs::write(dir.join(name), bytes).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }
    eprintln!("wrote {} fixtures to {}", fixtures().len(), dir.display());
}
