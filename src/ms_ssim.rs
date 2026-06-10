//! Multi-scale structural similarity index (MS-SSIM).
//!
//! MS-SSIM is a native implementation of the metric introduced by Wang,
//! Simoncelli, and Bovik (2003). It evaluates SSIM over an image pyramid: at
//! each scale the same 11x11 Gaussian window (σ = 1.5) used by [`ssim`] slides
//! over the image, the image is then low-pass filtered and downsampled 2x, and
//! the process repeats. Coarse scales contribute only their *contrast-structure*
//! term, the single coarsest scale contributes the full SSIM (which folds in the
//! luminance term), and the per-scale results are combined as a weighted product
//! using the original paper's weights:
//!
//! ```text
//! MS-SSIM = (∏ cs[i]^w[i] for i in 0..n-1) · ssim[n-1]^w[n-1]
//! w = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333]
//! ```
//!
//! Scores lie in `(-1, 1]`, higher is better, and pixel-identical images score
//! exactly `1.0`. An alpha channel, if present, is ignored, and each image must
//! be at least 11x11 — the window has to fit at least once at the finest scale.
//!
//! # Scale count
//!
//! The canonical metric uses five scales, which needs the coarsest scale to
//! still be at least 11x11 and therefore an image of at least 176x176. To stay
//! well-defined for ordinary images, this implementation uses *up to* five
//! scales, stopping before any scale would fall below the window, and
//! renormalizes the weights of the scales it does use so they sum to one (a
//! one-scale evaluation is then exactly [`ssim`]). At 176x176 and larger all
//! five scales run, matching the reference.
//!
//! [`ssim`]: crate::ssim

use crate::error::{Error, Result};
use crate::format::{PixelFormat, Sample};
use crate::image::Image;
use crate::ssim::{
    K1, K2, SsimMode, WINDOW, color_channels, gaussian_window, luma, ssim_cs_map_means,
};

/// Maximum number of pyramid scales (the canonical MS-SSIM uses five).
const MAX_SCALES: usize = 5;

/// Per-scale weights from Wang, Simoncelli & Bovik (2003), finest scale first.
const WEIGHTS: [f64; MAX_SCALES] = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333];

/// Lower bound applied to each per-scale factor before exponentiation.
///
/// The contrast-structure term can go slightly negative for pathologically
/// distorted content; clamping it to a tiny positive value keeps the weighted
/// product finite (a fractional power of a negative base is `NaN`) and drives
/// the score toward zero, as a near-zero factor should.
const TERM_FLOOR: f64 = 1e-12;

/// Options controlling an MS-SSIM computation.
///
/// MS-SSIM reuses [`SsimMode`], so the color handling matches [`ssim`](crate::ssim)
/// exactly.
#[derive(Debug, Clone, Copy, Default)]
pub struct MsssimOptions {
    /// The signal MS-SSIM is computed over.
    pub mode: SsimMode,
}

/// Computes the MS-SSIM between `reference` and `distorted`.
///
/// Both images share the pixel format `F`, so a difference in color space,
/// channel layout, or bit depth is rejected by the compiler rather than at
/// run time. An alpha channel, if present, is ignored. The score ranges over
/// `(-1, 1]`; higher is better, and pixel-identical images score exactly `1.0`.
/// Each image must be at least 11x11 (the size of the Gaussian window). The
/// metric uses up to five pyramid scales, dropping coarser scales (and
/// renormalizing their weights) for images too small to hold them; all five run
/// at 176x176 and larger, matching the reference.
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 11 pixels.
///
/// # Examples
///
/// ```
/// use iqa::{Image, MsssimOptions, msssim};
///
/// let reference = Image::srgb8(32, 32, vec![128; 32 * 32 * 3])?;
/// let distorted = Image::srgb8(32, 32, vec![130; 32 * 32 * 3])?;
/// let score = msssim(&reference, &distorted, MsssimOptions::default())?;
/// assert!(score <= 1.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa::{Image, MsssimOptions, msssim};
///
/// let rgb = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let gray = Image::gray8(11, 11, vec![0; 11 * 11])?;
/// let _ = msssim(&rgb, &gray, MsssimOptions::default()); // channel mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// ```compile_fail
/// use iqa::{Image, MsssimOptions, msssim};
///
/// let eight = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let sixteen = Image::srgb16(11, 11, vec![0; 11 * 11 * 3])?;
/// let _ = msssim(&eight, &sixteen, MsssimOptions::default()); // bit-depth mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn msssim<F: PixelFormat>(
    reference: &Image<F>,
    distorted: &Image<F>,
    opts: MsssimOptions,
) -> Result<f64> {
    if reference.dimensions() != distorted.dimensions() {
        return Err(Error::DimensionMismatch {
            a: reference.dimensions(),
            b: distorted.dimensions(),
        });
    }

    let (width, height) = reference.dimensions();
    if width < WINDOW as u32 || height < WINDOW as u32 {
        return Err(Error::ImageTooSmall(width, height, WINDOW as u32));
    }

    let l = <F::Sample as Sample>::MAX;
    let c1 = (K1 * l).powi(2);
    let c2 = (K2 * l).powi(2);

    let score = match opts.mode {
        SsimMode::RgbAveraged => {
            let channels = color_channels(F::CHANNELS);
            let total: f64 = channels
                .iter()
                .map(|&c| {
                    let r = channel_buffer(reference, c);
                    let d = channel_buffer(distorted, c);
                    msssim_of_signal(&r, &d, width, height, c1, c2)
                })
                .sum();
            total / channels.len() as f64
        }
        SsimMode::Luma709 => {
            let r = luma_buffer(reference);
            let d = luma_buffer(distorted);
            msssim_of_signal(&r, &d, width, height, c1, c2)
        }
    };

    Ok(score)
}

/// Extracts channel `c` of `img` into a row-major `f64` buffer.
fn channel_buffer<F: PixelFormat>(img: &Image<F>, c: usize) -> Vec<f64> {
    let (width, height) = img.dimensions();
    let mut buf = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            buf.push(img.sample_at(x, y, c));
        }
    }
    buf
}

/// Extracts the Rec.709 luma of `img` into a row-major `f64` buffer.
fn luma_buffer<F: PixelFormat>(img: &Image<F>) -> Vec<f64> {
    let (width, height) = img.dimensions();
    let mut buf = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            buf.push(luma(img, x, y));
        }
    }
    buf
}

/// MS-SSIM of one signal (a single channel, or luma), given as row-major `f64`
/// buffers of size `width * height` and the SSIM stabilizing constants.
fn msssim_of_signal(
    reference: &[f64],
    distorted: &[f64],
    width: u32,
    height: u32,
    c1: f64,
    c2: f64,
) -> f64 {
    let window = gaussian_window();
    let scales = num_scales(width, height);
    let weights = scale_weights(scales);

    let mut r = reference.to_vec();
    let mut d = distorted.to_vec();
    let (mut w, mut h) = (width, height);

    let mut product = 1.0;
    for (scale, &weight) in weights.iter().enumerate() {
        let stride = w as usize;
        let (full, cs) = ssim_cs_map_means(
            w,
            h,
            &window,
            c1,
            c2,
            |x, y| r[y as usize * stride + x as usize],
            |x, y| d[y as usize * stride + x as usize],
        );
        // Intermediate scales contribute only the contrast-structure term; the
        // coarsest scale contributes the full SSIM (which folds in luminance).
        let term = if scale == scales - 1 { full } else { cs };
        product *= term.max(TERM_FLOOR).powf(weight);

        if scale + 1 < scales {
            let (rn, nw, nh) = downsample(&r, w, h);
            let (dn, _, _) = downsample(&d, w, h);
            r = rn;
            d = dn;
            w = nw;
            h = nh;
        }
    }
    product
}

/// Number of pyramid scales to use for a `width`×`height` signal: up to
/// [`MAX_SCALES`], stopping before any scale would fall below the window.
fn num_scales(mut width: u32, mut height: u32) -> usize {
    let mut scales = 1;
    while scales < MAX_SCALES {
        let (nw, nh) = (width / 2, height / 2);
        if nw < WINDOW as u32 || nh < WINDOW as u32 {
            break;
        }
        width = nw;
        height = nh;
        scales += 1;
    }
    scales
}

/// The first `scales` of [`WEIGHTS`], renormalized to sum to one.
fn scale_weights(scales: usize) -> Vec<f64> {
    let used = &WEIGHTS[..scales];
    let sum: f64 = used.iter().sum();
    used.iter().map(|w| w / sum).collect()
}

/// Low-pass-and-decimate one signal by a factor of two: each output sample is
/// the average of a 2x2 block of inputs. Odd trailing rows/columns are dropped,
/// matching the reference's `width >>= 1` decimation.
fn downsample(src: &[f64], width: u32, height: u32) -> (Vec<f64>, u32, u32) {
    let (nw, nh) = (width / 2, height / 2);
    let stride = width as usize;
    let out_stride = nw as usize;
    let mut out = vec![0.0; out_stride * nh as usize];
    for j in 0..nh as usize {
        for i in 0..nw as usize {
            let (x0, y0) = (2 * i, 2 * j);
            let sum = src[y0 * stride + x0]
                + src[y0 * stride + x0 + 1]
                + src[(y0 + 1) * stride + x0]
                + src[(y0 + 1) * stride + x0 + 1];
            out[j * out_stride + i] = sum / 4.0;
        }
    }
    (out, nw, nh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Image;

    /// A solid sRGB image filled with `value` in every color channel.
    fn solid_srgb8(width: u32, height: u32, value: u8) -> Image<crate::format::Srgb8> {
        Image::srgb8(width, height, vec![value; (width * height * 3) as usize]).unwrap()
    }

    #[test]
    fn identical_images_are_one() {
        let img = solid_srgb8(64, 64, 100);
        let score = msssim(&img, &img, MsssimOptions::default()).unwrap();
        assert!((score - 1.0).abs() < 1e-12, "expected 1.0, got {score}");
    }

    #[test]
    fn uses_five_scales_only_once_large_enough() {
        // 175 is one short of the 176 needed for the coarsest of five 11x11
        // scales; 176 reaches it.
        assert_eq!(num_scales(175, 175), 4);
        assert_eq!(num_scales(176, 176), 5);
        assert_eq!(num_scales(11, 11), 1);
    }

    #[test]
    fn single_scale_matches_plain_ssim() {
        // An image too small to downsample collapses MS-SSIM to one full-SSIM
        // scale, which must equal `ssim` exactly.
        let reference = solid_srgb8(16, 16, 100);
        let distorted = solid_srgb8(16, 16, 120);
        assert_eq!(num_scales(16, 16), 1);
        let ms = msssim(&reference, &distorted, MsssimOptions::default()).unwrap();
        let plain =
            crate::ssim::ssim(&reference, &distorted, crate::ssim::SsimOptions::default()).unwrap();
        assert!((ms - plain).abs() < 1e-12, "ms={ms}, plain={plain}");
    }

    #[test]
    fn distortion_lowers_the_score() {
        let reference = solid_srgb8(64, 64, 128);
        let mut data = vec![128u8; 64 * 64 * 3];
        for (i, sample) in data.iter_mut().enumerate() {
            if i % 7 == 0 {
                *sample = 170;
            }
        }
        let distorted = Image::srgb8(64, 64, data).unwrap();
        let score = msssim(&reference, &distorted, MsssimOptions::default()).unwrap();
        assert!(score < 1.0, "distorted image scored {score}");
    }

    #[test]
    fn image_below_window_is_rejected() {
        let img = Image::srgb8(10, 10, vec![0; 10 * 10 * 3]).unwrap();
        let err = msssim(&img, &img, MsssimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::ImageTooSmall(10, 10, 11)));
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = Image::srgb8(16, 16, vec![0; 16 * 16 * 3]).unwrap();
        let b = Image::srgb8(16, 12, vec![0; 16 * 12 * 3]).unwrap();
        let err = msssim(&a, &b, MsssimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }
}
