//! SSIMULACRA2, a perceptual full-reference metric.
//!
//! This binds the original C++ reference implementation
//! ([cloudinary/ssimulacra2]) via FFI rather than reimplementing it in Rust,
//! keeping results faithful to upstream. The C++ sources are vendored as git
//! submodules under `third_party/` and compiled by `build.rs`.
//!
//! [cloudinary/ssimulacra2]: https://github.com/cloudinary/ssimulacra2

mod ffi;

use crate::error::{Error, Result};
use crate::format::{Gray8, Gray16, PixelFormat, Rgba8, Rgba16, Srgb8, Srgb16};
use crate::image::Image;

/// Minimum width and height the reference implementation accepts.
const MIN_DIMENSION: u32 = 8;

/// A pixel format that [`ssimulacra2`] can score.
///
/// SSIMULACRA2's reference implementation requires sRGB-encoded input, so this
/// trait is implemented only for the sRGB-family formats (grayscale counts: it
/// is treated as sRGB-encoded luma). It is the seam that keeps a non-sRGB
/// image from reaching the metric: were a linear-light format added to the
/// crate, omitting its `Ssimulacra2Input` impl would make passing it to
/// [`ssimulacra2`] a compile error, with no change to the function signature.
pub trait Ssimulacra2Input: PixelFormat {}

impl Ssimulacra2Input for Srgb8 {}
impl Ssimulacra2Input for Srgb16 {}
impl Ssimulacra2Input for Gray8 {}
impl Ssimulacra2Input for Gray16 {}
impl Ssimulacra2Input for Rgba8 {}
impl Ssimulacra2Input for Rgba16 {}

/// Computes the SSIMULACRA2 score between `reference` and `distorted`.
///
/// The score ranges up to `100` (mathematically lossless) and is unbounded
/// below; higher is better. Both images share the format `F`, which the
/// [`Ssimulacra2Input`] bound additionally constrains to an sRGB-family
/// format. Each must be at least 8x8.
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 8 pixels.
/// - [`Error::Ssimulacra2Failed`] if the native implementation reports failure.
///
/// # Examples
///
/// ```no_run
/// use iqa_rs::{Image, ssimulacra2};
///
/// let reference = Image::srgb8(8, 8, vec![128; 192])?;
/// let distorted = Image::srgb8(8, 8, vec![130; 192])?;
/// let score = ssimulacra2(&reference, &distorted)?;
/// assert!(score <= 100.0);
/// # Ok::<(), iqa_rs::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa_rs::{Image, ssimulacra2};
///
/// let rgb = Image::srgb8(8, 8, vec![0; 192])?;
/// let gray = Image::gray8(8, 8, vec![0; 64])?;
/// let _ = ssimulacra2(&rgb, &gray); // mismatched formats
/// # Ok::<(), iqa_rs::Error>(())
/// ```
pub fn ssimulacra2<F: Ssimulacra2Input>(
    reference: &Image<F>,
    distorted: &Image<F>,
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
    ffi::compute(&orig, &dist, width, height)
}
