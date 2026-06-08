//! Shared fixtures for the black-box test suite.
//!
//! This module is included by the integration tests via `mod common;`. It
//! provides typed image builders, the [`Fixture`] trait (per-format
//! reference/distortion generation), the [`Metric`] trait (a uniform
//! description of an IQA metric), and [`run_property_suite`], which exercises
//! a `(format, metric)` pair against the rules every full-reference IQA metric
//! must obey.
//!
//! Because [`Image`] is generic over its pixel format, the property battery is
//! dispatched at compile time: `tests/properties.rs` instantiates
//! [`run_property_suite`] once per `(format, metric)` pair.

// Each test crate that includes this module uses a different subset of it.
#![allow(dead_code)]

use iqa::{Error, PixelFormat};

pub use iqa::{Gray8, Gray16, Image, Rgba8, Rgba16, Srgb8, Srgb16};

// ---------------------------------------------------------------------------
// Sample-type helpers
// ---------------------------------------------------------------------------

/// Bridges the 8-bit "design domain" used by the fixtures to a concrete sample
/// type. 16-bit fixtures are the 8-bit ones scaled by `SCALE`, so a given
/// fixture has the same *relative* content at either bit depth.
trait ScaledSample: Copy + Into<f64> {
    /// Multiplier from an 8-bit value to this sample type's full range.
    const SCALE: f64;
    /// Largest representable value.
    const MAXV: f64;
    /// Converts an 8-bit-domain value into a full-range sample.
    fn from_8bit(value8: f64) -> Self;
    /// Rounds and clamps a full-range value into a sample.
    fn from_full(value: f64) -> Self;
}

impl ScaledSample for u8 {
    const SCALE: f64 = 1.0;
    const MAXV: f64 = 255.0;
    fn from_8bit(value8: f64) -> Self {
        Self::from_full(value8 * Self::SCALE)
    }
    fn from_full(value: f64) -> Self {
        value.round().clamp(0.0, Self::MAXV) as u8
    }
}

impl ScaledSample for u16 {
    const SCALE: f64 = 257.0;
    const MAXV: f64 = 65_535.0;
    fn from_8bit(value8: f64) -> Self {
        Self::from_full(value8 * Self::SCALE)
    }
    fn from_full(value: f64) -> Self {
        value.round().clamp(0.0, Self::MAXV) as u16
    }
}

/// Builds a tightly packed, row-major sample buffer from a generator.
fn grid<S>(
    width: u32,
    height: u32,
    channels: usize,
    mut f: impl FnMut(u32, u32, usize) -> S,
) -> Vec<S> {
    let mut data = Vec::with_capacity(width as usize * height as usize * channels);
    for y in 0..height {
        for x in 0..width {
            for c in 0..channels {
                data.push(f(x, y, c));
            }
        }
    }
    data
}

// ---------------------------------------------------------------------------
// Typed image builders (for the metric-specific test files)
// ---------------------------------------------------------------------------

/// Builds an 8-bit sRGB image from a per-sample generator `f(x, y, c)`.
pub fn srgb8(width: u32, height: u32, f: impl FnMut(u32, u32, usize) -> u8) -> Image<Srgb8> {
    Image::srgb8(width, height, grid(width, height, 3, f)).unwrap()
}

/// Builds a 16-bit sRGB image from a per-sample generator `f(x, y, c)`.
pub fn srgb16(width: u32, height: u32, f: impl FnMut(u32, u32, usize) -> u16) -> Image<Srgb16> {
    Image::srgb16(width, height, grid(width, height, 3, f)).unwrap()
}

/// Builds an 8-bit grayscale image from a per-pixel generator `f(x, y)`.
pub fn gray8(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> u8) -> Image<Gray8> {
    Image::gray8(width, height, grid(width, height, 1, |x, y, _| f(x, y))).unwrap()
}

/// Builds a 16-bit grayscale image from a per-pixel generator `f(x, y)`.
pub fn gray16(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> u16) -> Image<Gray16> {
    Image::gray16(width, height, grid(width, height, 1, |x, y, _| f(x, y))).unwrap()
}

/// Builds an 8-bit RGBA image from a per-sample generator `f(x, y, c)`.
pub fn rgba8(width: u32, height: u32, f: impl FnMut(u32, u32, usize) -> u8) -> Image<Rgba8> {
    Image::rgba8(width, height, grid(width, height, 4, f)).unwrap()
}

/// Builds a 16-bit RGBA image from a per-sample generator `f(x, y, c)`.
pub fn rgba16(width: u32, height: u32, f: impl FnMut(u32, u32, usize) -> u16) -> Image<Rgba16> {
    Image::rgba16(width, height, grid(width, height, 4, f)).unwrap()
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

/// A structured 8-bit-domain color sample kept in `96..=159`, so even the
/// largest distortion the property suite applies never needs clamping.
fn gradient_8bit(x: u32, y: u32, c: usize) -> f64 {
    (96 + ((x * 2 + y * 3 + c as u32 * 13) % 64)) as f64
}

// ---------------------------------------------------------------------------
// Fixture: per-format reference content and distortion
// ---------------------------------------------------------------------------

/// Generates reference images and deterministic distortions for one pixel
/// format, so the property suite can be run generically over any format.
pub trait Fixture: PixelFormat + Sized {
    /// A structured, non-trivial reference image.
    fn base(width: u32, height: u32) -> Image<Self>;
    /// A uniform image whose color channels all hold `value8` (8-bit domain).
    fn solid(width: u32, height: u32, value8: f64) -> Image<Self>;
    /// Deterministically distorts `base`; the mean squared error grows with
    /// `amplitude` (an 8-bit-domain figure), so a larger amplitude is
    /// unambiguously a worse image. An alpha channel, if present, is untouched.
    fn distort(base: &Image<Self>, amplitude: f64) -> Image<Self>;
    /// A copy of `base` with *only* the alpha channel changed, or `None` for
    /// formats without alpha. Metrics must score this identical to `base`.
    fn alpha_variant(base: &Image<Self>) -> Option<Image<Self>>;
}

/// Implements [`Fixture`] for one pixel format.
macro_rules! impl_fixture {
    ($fmt:ty, $ctor:ident, $sample:ty) => {
        impl Fixture for $fmt {
            fn base(width: u32, height: u32) -> Image<$fmt> {
                let ch = <$fmt as PixelFormat>::CHANNELS.count();
                let data = grid::<$sample>(width, height, ch, |x, y, c| {
                    if c == 3 {
                        <$sample as ScaledSample>::from_8bit(255.0)
                    } else {
                        <$sample as ScaledSample>::from_8bit(gradient_8bit(x, y, c))
                    }
                });
                Image::$ctor(width, height, data).unwrap()
            }

            fn solid(width: u32, height: u32, value8: f64) -> Image<$fmt> {
                let ch = <$fmt as PixelFormat>::CHANNELS.count();
                let data = grid::<$sample>(width, height, ch, |_, _, c| {
                    let v = if c == 3 { 255.0 } else { value8 };
                    <$sample as ScaledSample>::from_8bit(v)
                });
                Image::$ctor(width, height, data).unwrap()
            }

            fn distort(base: &Image<$fmt>, amplitude: f64) -> Image<$fmt> {
                let ch = <$fmt as PixelFormat>::CHANNELS.count();
                let w = base.width();
                let src = base.samples();
                let scale = <$sample as ScaledSample>::SCALE;
                let data = grid::<$sample>(base.width(), base.height(), ch, |x, y, c| {
                    let idx = (y as usize * w as usize + x as usize) * ch + c;
                    if c == 3 {
                        src[idx]
                    } else {
                        let orig: f64 = src[idx].into();
                        <$sample as ScaledSample>::from_full(
                            orig + amplitude * scale * delta(x, y, c),
                        )
                    }
                });
                Image::$ctor(base.width(), base.height(), data).unwrap()
            }

            fn alpha_variant(base: &Image<$fmt>) -> Option<Image<$fmt>> {
                let ch = <$fmt as PixelFormat>::CHANNELS.count();
                if ch < 4 {
                    return None;
                }
                let w = base.width();
                let src = base.samples();
                let data = grid::<$sample>(base.width(), base.height(), ch, |x, y, c| {
                    let idx = (y as usize * w as usize + x as usize) * ch + c;
                    if c == 3 {
                        <$sample as ScaledSample>::from_8bit(((x * 7 + y) % 256) as f64)
                    } else {
                        src[idx]
                    }
                });
                Some(Image::$ctor(base.width(), base.height(), data).unwrap())
            }
        }
    };
}

impl_fixture!(Srgb8, srgb8, u8);
impl_fixture!(Srgb16, srgb16, u16);
impl_fixture!(Gray8, gray8, u8);
impl_fixture!(Gray16, gray16, u16);
impl_fixture!(Rgba8, rgba8, u8);
impl_fixture!(Rgba16, rgba16, u16);

// ---------------------------------------------------------------------------
// Metric description
// ---------------------------------------------------------------------------

/// A uniform, compile-time description of an IQA metric specialized to a
/// pixel format `F`. Implementors are zero-sized tokens ([`PsnrRgbAvg`], ...).
pub trait Metric<F: PixelFormat> {
    /// Human-readable name, used in assertion messages.
    const NAME: &'static str;
    /// Score returned for two pixel-identical images.
    const IDENTITY_SCORE: f64;
    /// Whether a numerically higher score means better quality.
    const HIGHER_IS_BETTER: bool;
    /// Whether `compute(a, b) == compute(b, a)` for all inputs.
    const SYMMETRIC: bool;
    /// Smallest width/height the metric accepts.
    const MIN_DIM: u32;
    /// Runs the metric.
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64>;
}

/// PSNR in channel-pooled (`RgbAveraged`) mode.
#[cfg(feature = "psnr")]
pub struct PsnrRgbAvg;

#[cfg(feature = "psnr")]
impl<F: PixelFormat> Metric<F> for PsnrRgbAvg {
    const NAME: &'static str = "psnr (rgb-averaged)";
    const IDENTITY_SCORE: f64 = f64::INFINITY;
    const HIGHER_IS_BETTER: bool = true;
    const SYMMETRIC: bool = true;
    const MIN_DIM: u32 = 1;
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64> {
        iqa::psnr(
            reference,
            distorted,
            iqa::PsnrOptions {
                mode: iqa::PsnrMode::RgbAveraged,
            },
        )
    }
}

/// PSNR in Rec.709 luma (`Luma709`) mode.
#[cfg(feature = "psnr")]
pub struct PsnrLuma;

#[cfg(feature = "psnr")]
impl<F: PixelFormat> Metric<F> for PsnrLuma {
    const NAME: &'static str = "psnr (luma709)";
    const IDENTITY_SCORE: f64 = f64::INFINITY;
    const HIGHER_IS_BETTER: bool = true;
    const SYMMETRIC: bool = true;
    const MIN_DIM: u32 = 1;
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64> {
        iqa::psnr(
            reference,
            distorted,
            iqa::PsnrOptions {
                mode: iqa::PsnrMode::Luma709,
            },
        )
    }
}

/// SSIM in channel-averaged (`RgbAveraged`) mode.
#[cfg(feature = "ssim")]
pub struct SsimRgbAvg;

#[cfg(feature = "ssim")]
impl<F: PixelFormat> Metric<F> for SsimRgbAvg {
    const NAME: &'static str = "ssim (rgb-averaged)";
    const IDENTITY_SCORE: f64 = 1.0;
    const HIGHER_IS_BETTER: bool = true;
    const SYMMETRIC: bool = true;
    const MIN_DIM: u32 = 11;
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64> {
        iqa::ssim(
            reference,
            distorted,
            iqa::SsimOptions {
                mode: iqa::SsimMode::RgbAveraged,
            },
        )
    }
}

/// SSIM in Rec.709 luma (`Luma709`) mode.
#[cfg(feature = "ssim")]
pub struct SsimLuma;

#[cfg(feature = "ssim")]
impl<F: PixelFormat> Metric<F> for SsimLuma {
    const NAME: &'static str = "ssim (luma709)";
    const IDENTITY_SCORE: f64 = 1.0;
    const HIGHER_IS_BETTER: bool = true;
    const SYMMETRIC: bool = true;
    const MIN_DIM: u32 = 11;
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64> {
        iqa::ssim(
            reference,
            distorted,
            iqa::SsimOptions {
                mode: iqa::SsimMode::Luma709,
            },
        )
    }
}

/// SSIMULACRA2.
#[cfg(feature = "ssimulacra2")]
pub struct Ssim2;

#[cfg(feature = "ssimulacra2")]
impl<F: iqa::Ssimulacra2Input> Metric<F> for Ssim2 {
    const NAME: &'static str = "ssimulacra2";
    const IDENTITY_SCORE: f64 = 100.0;
    const HIGHER_IS_BETTER: bool = true;
    // SSIMULACRA2 is deliberately asymmetric in reference vs distorted.
    const SYMMETRIC: bool = false;
    const MIN_DIM: u32 = 8;
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64> {
        iqa::ssimulacra2(reference, distorted)
    }
}

/// Butteraugli (default options: 3-norm pooling).
#[cfg(feature = "butteraugli")]
pub struct Butteraugli;

#[cfg(feature = "butteraugli")]
impl<F: iqa::ButteraugliInput> Metric<F> for Butteraugli {
    const NAME: &'static str = "butteraugli";
    // A perfect match is distance zero, and lower is better.
    const IDENTITY_SCORE: f64 = 0.0;
    const HIGHER_IS_BETTER: bool = false;
    // Butteraugli's masking is computed from the reference, so it is asymmetric.
    const SYMMETRIC: bool = false;
    const MIN_DIM: u32 = 8;
    fn compute(reference: &Image<F>, distorted: &Image<F>) -> iqa::Result<f64> {
        iqa::butteraugli(reference, distorted, iqa::ButteraugliOptions::default())
    }
}

// ---------------------------------------------------------------------------
// Score comparison helpers
// ---------------------------------------------------------------------------

/// Whether `score` is a metric's perfect score (within float tolerance).
pub fn matches_identity(identity: f64, score: f64) -> bool {
    if identity.is_infinite() {
        score.is_infinite() && score.is_sign_positive() == identity.is_sign_positive()
    } else {
        (score - identity).abs() <= 1e-9
    }
}

/// Whether score `a` represents strictly better quality than score `b`.
pub fn strictly_better(higher_is_better: bool, a: f64, b: f64) -> bool {
    if higher_is_better { a > b } else { a < b }
}

/// Whether score `a` is *not* strictly better than `b` (worse or equal).
pub fn not_better(higher_is_better: bool, a: f64, b: f64) -> bool {
    !strictly_better(higher_is_better, a, b)
}

/// Short type name of a format, for assertion messages.
fn fmt_name<F: 'static>() -> &'static str {
    std::any::type_name::<F>()
}

// ---------------------------------------------------------------------------
// The universal property battery
// ---------------------------------------------------------------------------

/// Runs every universal full-reference property for metric `M` on format `F`.
///
/// `tests/properties.rs` calls this once per `(format, metric)` pair.
pub fn run_property_suite<F, M>()
where
    F: Fixture,
    M: Metric<F>,
{
    check_identity::<F, M>();
    check_determinism::<F, M>();
    check_dimension_mismatch::<F, M>();
    check_min_dimension::<F, M>();
    check_monotonicity::<F, M>();
    check_finite_and_bounded::<F, M>();
    check_alpha_ignored::<F, M>();
    if M::SYMMETRIC {
        check_symmetry::<F, M>();
    }
}

/// Identical inputs must score the metric's perfect score.
fn check_identity<F: Fixture, M: Metric<F>>() {
    for (label, img) in [
        ("gradient", F::base(24, 24)),
        ("solid", F::solid(24, 24, 128.0)),
    ] {
        let score = M::compute(&img, &img).unwrap_or_else(|e| {
            panic!(
                "{} [{}/{label}]: identical image errored: {e}",
                M::NAME,
                fmt_name::<F>()
            )
        });
        assert!(
            matches_identity(M::IDENTITY_SCORE, score),
            "{} [{}/{label}]: identical inputs scored {score}, expected {}",
            M::NAME,
            fmt_name::<F>(),
            M::IDENTITY_SCORE,
        );
    }
}

/// Repeated computation on the same inputs must be bit-identical.
fn check_determinism<F: Fixture, M: Metric<F>>() {
    let reference = F::base(32, 32);
    let distorted = F::distort(&reference, 20.0);
    let first = M::compute(&reference, &distorted).unwrap();
    let second = M::compute(&reference, &distorted).unwrap();
    assert_eq!(
        first.to_bits(),
        second.to_bits(),
        "{} [{}]: nondeterministic: {first} then {second}",
        M::NAME,
        fmt_name::<F>(),
    );
}

/// Images of different dimensions must be rejected.
fn check_dimension_mismatch<F: Fixture, M: Metric<F>>() {
    let a = F::base(32, 32);
    let b = F::base(32, 24);
    assert!(
        matches!(M::compute(&a, &b), Err(Error::DimensionMismatch { .. })),
        "{} [{}]: 32x32 vs 32x24 should fail with DimensionMismatch",
        M::NAME,
        fmt_name::<F>(),
    );
}

/// Images below the metric's minimum dimension must be rejected.
fn check_min_dimension<F: Fixture, M: Metric<F>>() {
    if M::MIN_DIM <= 1 {
        return;
    }
    let d = M::MIN_DIM - 1;
    let tiny = F::base(d, d);
    assert!(
        M::compute(&tiny, &tiny).is_err(),
        "{} [{}]: accepted a {d}x{d} image below the {min}x{min} minimum",
        M::NAME,
        fmt_name::<F>(),
        min = M::MIN_DIM,
    );
}

/// More distortion must never improve the score.
fn check_monotonicity<F: Fixture, M: Metric<F>>() {
    // Amplitudes are well separated so the ordering is unambiguous.
    let amplitudes = [0.0, 8.0, 22.0, 55.0];
    let base = F::base(32, 32);
    let scores: Vec<f64> = amplitudes
        .iter()
        .map(|&amp| M::compute(&base, &F::distort(&base, amp)).unwrap())
        .collect();

    assert!(
        matches_identity(M::IDENTITY_SCORE, scores[0]),
        "{} [{}]: zero distortion scored {}, expected the perfect score",
        M::NAME,
        fmt_name::<F>(),
        scores[0],
    );
    for pair in scores.windows(2) {
        assert!(
            not_better(M::HIGHER_IS_BETTER, pair[1], pair[0]),
            "{} [{}]: increasing distortion improved the score: {scores:?}",
            M::NAME,
            fmt_name::<F>(),
        );
    }
    assert!(
        strictly_better(M::HIGHER_IS_BETTER, scores[1], scores[3]),
        "{} [{}]: light distortion ({}) did not beat heavy distortion ({})",
        M::NAME,
        fmt_name::<F>(),
        scores[1],
        scores[3],
    );
}

/// Distorted scores must be finite and never beat the perfect score.
fn check_finite_and_bounded<F: Fixture, M: Metric<F>>() {
    let base = F::base(24, 24);
    for amplitude in [5.0, 25.0, 70.0] {
        let score = M::compute(&base, &F::distort(&base, amplitude)).unwrap();
        assert!(
            score.is_finite(),
            "{} [{}]: amplitude {amplitude} produced a non-finite score {score}",
            M::NAME,
            fmt_name::<F>(),
        );
        assert!(
            not_better(M::HIGHER_IS_BETTER, score, M::IDENTITY_SCORE),
            "{} [{}]: distorted score {score} beat the perfect score {}",
            M::NAME,
            fmt_name::<F>(),
            M::IDENTITY_SCORE,
        );
    }
}

/// For RGBA formats, changing only the alpha channel must not change the score.
fn check_alpha_ignored<F: Fixture, M: Metric<F>>() {
    let base = F::base(24, 24);
    if let Some(variant) = F::alpha_variant(&base) {
        let score = M::compute(&base, &variant).unwrap();
        assert!(
            matches_identity(M::IDENTITY_SCORE, score),
            "{} [{}]: changing only the alpha channel changed the score to {score}",
            M::NAME,
            fmt_name::<F>(),
        );
    }
}

/// A metric declared symmetric must ignore argument order.
fn check_symmetry<F: Fixture, M: Metric<F>>() {
    let a = F::base(24, 24);
    let b = F::distort(&a, 18.0);
    let forward = M::compute(&a, &b).unwrap();
    let backward = M::compute(&b, &a).unwrap();
    assert_eq!(
        forward.to_bits(),
        backward.to_bits(),
        "{} [{}]: declared symmetric but compute(a,b)={forward} != compute(b,a)={backward}",
        M::NAME,
        fmt_name::<F>(),
    );
}
