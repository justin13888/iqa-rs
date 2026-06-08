//! Structural dissimilarity (DSSIM).
//!
//! DSSIM is the classic structural *dissimilarity* derived from
//! [`ssim`](crate::ssim): `DSSIM = (1 - SSIM) / 2`. Because SSIM lies in
//! `(-1, 1]`, DSSIM lies in `[0, 1)`; `0.0` means the images are identical and
//! a larger value means more distortion, so — unlike SSIM — **lower is
//! better**. It is computed over the same Gaussian-windowed SSIM map, shares
//! SSIM's [`SsimMode`] knob, and inherits its exact symmetry, determinism, and
//! 11x11 minimum size.
//!
//! This is the textbook structural-dissimilarity definition (Wang et al. 2004;
//! Loza et al.). It is deliberately **not** the multi-scale, L\*a\*b\* DSSIM of
//! [`kornelski/dssim`](https://github.com/kornelski/dssim): that implementation
//! is AGPL-licensed and defined only by its source, so it cannot be reproduced
//! under this crate's permissive license. No numeric parity with it is implied.

use crate::error::Result;
use crate::format::PixelFormat;
use crate::image::Image;
use crate::ssim::{SsimMode, SsimOptions, ssim};

/// Options controlling a DSSIM computation.
///
/// DSSIM's only parameter is the SSIM signal it is built on, so it reuses
/// [`SsimMode`] directly rather than introducing a parallel enum.
#[derive(Debug, Clone, Copy, Default)]
pub struct DssimOptions {
    /// The signal SSIM is computed over before the dissimilarity transform.
    pub mode: SsimMode,
}

/// Computes the structural dissimilarity (DSSIM) between `reference` and
/// `distorted` as `(1 - SSIM) / 2`.
///
/// Both images share the pixel format `F`, so a difference in color space,
/// channel layout, or bit depth is rejected by the compiler rather than at run
/// time. An alpha channel, if present, is ignored. The score ranges over
/// `[0, 1)`; **lower is better**, and pixel-identical images score exactly
/// `0.0`. Each image must be at least 11x11 (the size of SSIM's Gaussian
/// window).
///
/// This is the classic `(1 - SSIM) / 2` dissimilarity, not the multi-scale
/// L\*a\*b\* metric of `kornelski/dssim` (see this module's documentation).
///
/// # Errors
///
/// - [`Error::DimensionMismatch`](crate::Error::DimensionMismatch) if the images
///   differ in size.
/// - [`Error::ImageTooSmall`](crate::Error::ImageTooSmall) if either dimension is
///   below 11 pixels.
///
/// # Examples
///
/// ```
/// use iqa::{Image, DssimOptions, dssim};
///
/// let reference = Image::srgb8(11, 11, vec![128; 11 * 11 * 3])?;
/// let distorted = Image::srgb8(11, 11, vec![130; 11 * 11 * 3])?;
/// let score = dssim(&reference, &distorted, DssimOptions::default())?;
/// assert!(score >= 0.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Identical images score exactly zero:
///
/// ```
/// use iqa::{Image, DssimOptions, dssim};
///
/// let img = Image::srgb8(11, 11, vec![64; 11 * 11 * 3])?;
/// assert_eq!(dssim(&img, &img, DssimOptions::default())?, 0.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa::{Image, DssimOptions, dssim};
///
/// let rgb = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let gray = Image::gray8(11, 11, vec![0; 11 * 11])?;
/// let _ = dssim(&rgb, &gray, DssimOptions::default()); // channel mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// ```compile_fail
/// use iqa::{Image, DssimOptions, dssim};
///
/// let eight = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let sixteen = Image::srgb16(11, 11, vec![0; 11 * 11 * 3])?;
/// let _ = dssim(&eight, &sixteen, DssimOptions::default()); // bit-depth mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn dssim<F: PixelFormat>(
    reference: &Image<F>,
    distorted: &Image<F>,
    opts: DssimOptions,
) -> Result<f64> {
    let s = ssim(reference, distorted, SsimOptions { mode: opts.mode })?;
    Ok((1.0 - s) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    /// An 11x11 solid sRGB image filled with `value` in every color channel.
    fn solid_srgb8(value: u8) -> Image<crate::format::Srgb8> {
        Image::srgb8(11, 11, vec![value; 11 * 11 * 3]).unwrap()
    }

    #[test]
    fn identical_images_are_zero() {
        let img = solid_srgb8(100);
        let score = dssim(&img, &img, DssimOptions::default()).unwrap();
        // SSIM is exactly 1.0 for identical input, so the transform is exact 0.
        assert_eq!(score, 0.0, "expected exactly 0.0, got {score}");
    }

    #[test]
    fn equals_one_minus_ssim_over_two() {
        let reference = solid_srgb8(128);
        let mut data = vec![128u8; 11 * 11 * 3];
        for (i, sample) in data.iter_mut().enumerate() {
            if i % 5 == 0 {
                *sample = 170;
            }
        }
        let distorted = Image::srgb8(11, 11, data).unwrap();
        let opts = DssimOptions::default();
        let d = dssim(&reference, &distorted, opts).unwrap();
        let s = ssim(&reference, &distorted, SsimOptions { mode: opts.mode }).unwrap();
        // Bit-exact: DSSIM is a pure transform of SSIM, nothing else.
        assert_eq!(d.to_bits(), ((1.0 - s) / 2.0).to_bits());
    }

    #[test]
    fn solid_offset_matches_closed_form() {
        // Both images uniform: every window has zero variance/covariance, so
        // SSIM collapses to (2ab + C1)/(a² + b² + C1) and DSSIM to (1 - that)/2.
        // Computed here independently of `ssim()` so a shared bug can't cancel.
        let reference = solid_srgb8(100);
        let distorted = solid_srgb8(120);
        let score = dssim(&reference, &distorted, DssimOptions::default()).unwrap();
        let c1 = (0.01 * 255.0_f64).powi(2);
        let ssim_expected = (2.0 * 100.0 * 120.0 + c1) / (100.0 * 100.0 + 120.0 * 120.0 + c1);
        let expected = (1.0 - ssim_expected) / 2.0;
        assert!(
            (score - expected).abs() < 1e-9,
            "got {score}, want {expected}"
        );
    }

    #[test]
    fn distortion_raises_the_score() {
        let reference = solid_srgb8(128);
        let mut data = vec![128u8; 11 * 11 * 3];
        for (i, sample) in data.iter_mut().enumerate() {
            if i % 7 == 0 {
                *sample = 160;
            }
        }
        let distorted = Image::srgb8(11, 11, data).unwrap();
        let score = dssim(&reference, &distorted, DssimOptions::default()).unwrap();
        assert!(score > 0.0, "distorted image scored {score}");
    }

    #[test]
    fn image_below_window_is_rejected() {
        let img = Image::srgb8(10, 10, vec![0; 10 * 10 * 3]).unwrap();
        let err = dssim(&img, &img, DssimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::ImageTooSmall(10, 10, 11)));
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = Image::srgb8(11, 11, vec![0; 11 * 11 * 3]).unwrap();
        let b = Image::srgb8(12, 11, vec![0; 12 * 11 * 3]).unwrap();
        let err = dssim(&a, &b, DssimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }
}
