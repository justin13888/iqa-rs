//! Butteraugli, a perceptual full-reference metric.
//!
//! This binds Google libjxl's Butteraugli comparator via FFI rather than
//! reimplementing it in Rust, keeping results faithful to upstream. The C++
//! sources are vendored under `third_party/` and compiled by `build.rs`,
//! sharing the libjxl subset that the [`ssimulacra2`](crate::ssimulacra2)
//! metric also uses.

mod ffi;

use crate::error::{Error, Result};
use crate::format::{Gray8, Gray16, PixelFormat, Rgba8, Rgba16, Srgb8, Srgb16};
use crate::image::Image;

/// Minimum width and height the reference implementation accepts.
const MIN_DIMENSION: u32 = 8;

/// A pixel format that [`butteraugli`] can score.
///
/// Butteraugli expects sRGB-encoded input (it gamma-expands internally), so
/// this trait is implemented only for the sRGB-family formats (grayscale
/// counts: it is treated as sRGB-encoded luma). It is the seam that keeps a
/// non-sRGB image from reaching the metric: were a linear-light format added to
/// the crate, omitting its `ButteraugliInput` impl would make passing it to
/// [`butteraugli`] a compile error, with no change to the function signature.
pub trait ButteraugliInput: PixelFormat {}

impl ButteraugliInput for Srgb8 {}
impl ButteraugliInput for Srgb16 {}
impl ButteraugliInput for Gray8 {}
impl ButteraugliInput for Gray16 {}
impl ButteraugliInput for Rgba8 {}
impl ButteraugliInput for Rgba16 {}

/// Tuning parameters for [`butteraugli`].
///
/// The defaults match libjxl's own (`intensity_target = 80`,
/// `hf_asymmetry = 1.0`) together with Butteraugli's canonical `pnorm = 3.0`,
/// so [`ButteraugliOptions::default`] reproduces a standard Butteraugli score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButteraugliOptions {
    /// Display luminance, in nits, that an input value of `1.0` maps to. libjxl
    /// uses `80.0`; raising it models a brighter viewing environment and makes
    /// the metric more sensitive to differences in dark regions.
    pub intensity_target: f32,

    /// Multiplier penalizing newly introduced high-frequency artifacts over
    /// blurred-away detail. `1.0` is neutral (symmetric).
    pub hf_asymmetry: f32,

    /// Exponent used to pool the per-pixel difference map into a single score.
    /// `3.0` is the conventional "Butteraugli 3-norm"; larger values weight the
    /// worst regions more heavily (approaching the max-norm). The default `3.0`
    /// uses libjxl's optimized pooling path; other values fall back to a slower
    /// scalar one.
    pub pnorm: f64,
}

impl Default for ButteraugliOptions {
    fn default() -> Self {
        Self {
            intensity_target: 80.0,
            hf_asymmetry: 1.0,
            pnorm: 3.0,
        }
    }
}

/// Computes the Butteraugli distance between `reference` and `distorted`.
///
/// The distance is `0.0` for identical images and grows with perceived
/// difference (**lower is better**); a value near `1.0` corresponds roughly to
/// a just-noticeable difference. Both images share the format `F`, which the
/// [`ButteraugliInput`] bound additionally constrains to an sRGB-family format.
/// Each must be at least 8x8. See [`ButteraugliOptions`] for tuning.
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 8 pixels.
/// - [`Error::ButteraugliFailed`] if the native implementation reports failure.
///
/// # Examples
///
/// ```no_run
/// use iqa::{ButteraugliOptions, Image, butteraugli};
///
/// let reference = Image::srgb8(8, 8, vec![128; 192])?;
/// let distorted = Image::srgb8(8, 8, vec![130; 192])?;
/// let distance = butteraugli(&reference, &distorted, ButteraugliOptions::default())?;
/// assert!(distance >= 0.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa::{ButteraugliOptions, Image, butteraugli};
///
/// let rgb = Image::srgb8(8, 8, vec![0; 192])?;
/// let gray = Image::gray8(8, 8, vec![0; 64])?;
/// let _ = butteraugli(&rgb, &gray, ButteraugliOptions::default()); // mismatched formats
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn butteraugli<F: ButteraugliInput>(
    reference: &Image<F>,
    distorted: &Image<F>,
    opts: ButteraugliOptions,
) -> Result<f64> {
    if reference.dimensions() != distorted.dimensions() {
        return Err(Error::DimensionMismatch {
            a: reference.dimensions(),
            b: distorted.dimensions(),
        });
    }

    let (width, height) = reference.dimensions();
    if width < MIN_DIMENSION || height < MIN_DIMENSION {
        return Err(Error::ImageTooSmall(width, height, MIN_DIMENSION));
    }

    let orig = reference.to_rgb_f32_normalized();
    let dist = distorted.to_rgb_f32_normalized();
    ffi::compute(
        &orig,
        &dist,
        width,
        height,
        opts.intensity_target,
        opts.hf_asymmetry,
        opts.pnorm,
    )
}
