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
use crate::image::Image;

/// Minimum width and height the reference implementation accepts.
const MIN_DIMENSION: u32 = 8;

/// Computes the SSIMULACRA2 score between `reference` and `distorted`.
///
/// The score ranges up to `100` (mathematically lossless) and is unbounded
/// below; higher is better. Both images are converted internally to sRGB and
/// must be at least 8x8.
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 8 pixels.
/// - [`Error::Ssimulacra2Failed`] if the native implementation reports failure.
pub fn ssimulacra2(reference: &Image, distorted: &Image) -> Result<f64> {
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
