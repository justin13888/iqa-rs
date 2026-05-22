//! Shared fixtures for the black-box test suite.
//!
//! This module is included by the integration tests via `mod common;`. It
//! provides image builders, deterministic distortion helpers, and — most
//! importantly — [`MetricSpec`], a uniform description of an IQA metric.
//!
//! Every metric is registered in [`metrics`], and the property battery in
//! `tests/properties.rs` exercises each registered metric against the rules
//! that *every* full-reference IQA metric must obey. Adding a new metric is
//! therefore a one-line change that automatically inherits the whole battery.

// Each test crate that includes this module uses a different subset of it.
#![allow(dead_code)]

use iqa_rs::{BitDepth, Channels, ColorSpace, Image};

// ---------------------------------------------------------------------------
// Image builders
// ---------------------------------------------------------------------------

/// Builds an 8-bit sRGB RGB image from a per-sample generator `f(x, y, c)`.
pub fn rgb8(width: u32, height: u32, mut f: impl FnMut(u32, u32, usize) -> u8) -> Image {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                data.push(f(x, y, c));
            }
        }
    }
    Image::new(
        width,
        height,
        Channels::Rgb,
        BitDepth::Eight,
        ColorSpace::Srgb,
        data,
    )
    .unwrap()
}

/// Builds an 8-bit RGBA image from a per-sample generator `f(x, y, c)`.
pub fn rgba8(width: u32, height: u32, mut f: impl FnMut(u32, u32, usize) -> u8) -> Image {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            for c in 0..4 {
                data.push(f(x, y, c));
            }
        }
    }
    Image::new(
        width,
        height,
        Channels::Rgba,
        BitDepth::Eight,
        ColorSpace::Srgb,
        data,
    )
    .unwrap()
}

/// Builds an 8-bit grayscale image from a per-pixel generator `f(x, y)`.
pub fn gray8(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> u8) -> Image {
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            data.push(f(x, y));
        }
    }
    Image::new(
        width,
        height,
        Channels::Gray,
        BitDepth::Eight,
        ColorSpace::Grayscale,
        data,
    )
    .unwrap()
}

/// Builds a 16-bit sRGB RGB image (samples stored little-endian).
pub fn rgb16(width: u32, height: u32, mut f: impl FnMut(u32, u32, usize) -> u16) -> Image {
    let mut data = Vec::with_capacity((width * height * 3 * 2) as usize);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                data.extend_from_slice(&f(x, y, c).to_le_bytes());
            }
        }
    }
    Image::new(
        width,
        height,
        Channels::Rgb,
        BitDepth::Sixteen,
        ColorSpace::Srgb,
        data,
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// Deterministic content & distortion
// ---------------------------------------------------------------------------

/// Deterministic, well-mixed hash of three integers.
fn hash3(x: u32, y: u32, c: u32) -> u32 {
    let mut h =
        x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

/// Deterministic pseudo-random offset in `-1.0..=1.0` for one sample.
fn delta(x: u32, y: u32, c: usize) -> f64 {
    (hash3(x, y, c as u32) % 20_001) as f64 / 10_000.0 - 1.0
}

/// A structured 8-bit RGB sample value kept in `96..=159` so distortion never
/// needs clamping.
pub fn gradient_sample(x: u32, y: u32, c: usize) -> u8 {
    (96 + ((x * 2 + y * 3 + c as u32 * 13) % 64)) as u8
}

/// A non-trivial 8-bit RGB reference image.
pub fn base_rgb8(width: u32, height: u32) -> Image {
    rgb8(width, height, gradient_sample)
}

/// A non-trivial 16-bit RGB reference image (samples in `20_000..=28_000`).
pub fn base_rgb16(width: u32, height: u32) -> Image {
    rgb16(width, height, |x, y, c| {
        20_000 + ((x * 211 + y * 307 + c as u32 * 1009) % 8_001) as u16
    })
}

/// Applies deterministic distortion of the given `amplitude` to an 8-bit RGB
/// image. The mean squared error grows as `amplitude^2`, so a larger amplitude
/// is unambiguously a worse image.
pub fn distort_rgb8(base: &Image, amplitude: f64) -> Image {
    assert_eq!(base.channels, Channels::Rgb);
    assert_eq!(base.bit_depth, BitDepth::Eight);
    rgb8(base.width, base.height, |x, y, c| {
        let i = ((y * base.width + x) as usize) * 3 + c;
        let v = base.data[i] as f64 + amplitude * delta(x, y, c);
        v.round().clamp(0.0, 255.0) as u8
    })
}

/// Applies deterministic distortion of the given `amplitude` to a 16-bit RGB
/// image.
pub fn distort_rgb16(base: &Image, amplitude: f64) -> Image {
    assert_eq!(base.channels, Channels::Rgb);
    assert_eq!(base.bit_depth, BitDepth::Sixteen);
    rgb16(base.width, base.height, |x, y, c| {
        let i = (((y * base.width + x) as usize) * 3 + c) * 2;
        let orig = u16::from_le_bytes([base.data[i], base.data[i + 1]]) as f64;
        let v = orig + amplitude * delta(x, y, c);
        v.round().clamp(0.0, 65_535.0) as u16
    })
}

// ---------------------------------------------------------------------------
// Metric description & registry
// ---------------------------------------------------------------------------

/// A uniform description of an IQA metric, used to drive the generic property
/// battery. Every full-reference metric in the crate is registered in
/// [`metrics`].
#[derive(Clone, Copy)]
pub struct MetricSpec {
    /// Human-readable name, used in assertion messages.
    pub name: &'static str,
    /// The metric function under test.
    pub compute: fn(&Image, &Image) -> iqa_rs::Result<f64>,
    /// Score returned for two pixel-identical images.
    pub identity_score: f64,
    /// Whether a numerically higher score means better quality.
    pub higher_is_better: bool,
    /// Whether `compute(a, b) == compute(b, a)` for all inputs.
    pub symmetric: bool,
    /// Smallest width/height the metric accepts.
    pub min_dim: u32,
}

impl MetricSpec {
    /// Whether `score` is the metric's perfect score (within float tolerance).
    pub fn matches_identity(self, score: f64) -> bool {
        if self.identity_score.is_infinite() {
            score.is_infinite()
                && score.is_sign_positive() == self.identity_score.is_sign_positive()
        } else {
            (score - self.identity_score).abs() <= 1e-9
        }
    }

    /// Whether score `a` represents strictly better quality than score `b`.
    pub fn strictly_better(self, a: f64, b: f64) -> bool {
        if self.higher_is_better { a > b } else { a < b }
    }

    /// Whether score `a` is *not* strictly better than `b` (worse or equal).
    pub fn not_better(self, a: f64, b: f64) -> bool {
        !self.strictly_better(a, b)
    }
}

/// All IQA metrics currently implemented, gated by their Cargo features.
///
/// Registering a metric here automatically subjects it to the full property
/// battery in `tests/properties.rs`.
pub fn metrics() -> Vec<MetricSpec> {
    #[allow(unused_mut)]
    let mut specs: Vec<MetricSpec> = Vec::new();

    #[cfg(feature = "psnr")]
    {
        specs.push(MetricSpec {
            name: "psnr (rgb-averaged)",
            compute: |a, b| {
                iqa_rs::psnr(
                    a,
                    b,
                    iqa_rs::PsnrOptions {
                        mode: iqa_rs::PsnrMode::RgbAveraged,
                    },
                )
            },
            identity_score: f64::INFINITY,
            higher_is_better: true,
            symmetric: true,
            min_dim: 1,
        });
        specs.push(MetricSpec {
            name: "psnr (luma709)",
            compute: |a, b| {
                iqa_rs::psnr(
                    a,
                    b,
                    iqa_rs::PsnrOptions {
                        mode: iqa_rs::PsnrMode::Luma709,
                    },
                )
            },
            identity_score: f64::INFINITY,
            higher_is_better: true,
            symmetric: true,
            min_dim: 1,
        });
    }

    #[cfg(feature = "ssimulacra2")]
    {
        specs.push(MetricSpec {
            name: "ssimulacra2",
            compute: |a, b| iqa_rs::ssimulacra2(a, b),
            identity_score: 100.0,
            higher_is_better: true,
            // SSIMULACRA2 is deliberately asymmetric in reference vs distorted.
            symmetric: false,
            min_dim: 8,
        });
    }

    specs
}
