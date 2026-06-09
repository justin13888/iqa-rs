//! Structural similarity index (SSIM).
//!
//! SSIM is a native implementation of the metric introduced by Wang, Bovik,
//! Sheikh, and Simoncelli (2004). It slides an 11x11 Gaussian window (σ = 1.5)
//! over the image, computes a structural similarity value at every fully
//! covered position, and reports the mean of that map (the *mean SSIM*, or
//! MSSIM). Scores lie in `(-1, 1]`, higher is better; pixel-identical images
//! score exactly `1.0`.
//!
//! Each window contributes
//! `((2 μx μy + C1)(2 σxy + C2)) / ((μx² + μy² + C1)(σx² + σy² + C2))`, where the
//! means, variances, and covariance are all weighted by the same Gaussian
//! window and the stabilizing constants are `C1 = (K1·L)²`, `C2 = (K2·L)²` with
//! `K1 = 0.01`, `K2 = 0.03`, and `L` the dynamic range of the bit depth.
//!
//! An alpha channel, if present, is ignored, and each image must be at least
//! 11x11 — the window has to fit at least once.

use crate::error::{Error, Result};
use crate::format::{PixelFormat, Sample};
use crate::image::{Channels, Image};

/// Side length of the (square) Gaussian window.
pub(crate) const WINDOW: usize = 11;
/// Standard deviation of the Gaussian window, in pixels.
const SIGMA: f64 = 1.5;
/// First stabilizing constant coefficient (`C1 = (K1·L)²`).
pub(crate) const K1: f64 = 0.01;
/// Second stabilizing constant coefficient (`C2 = (K2·L)²`).
pub(crate) const K2: f64 = 0.03;

/// Which signal SSIM is computed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SsimMode {
    /// SSIM computed independently for the R, G, and B channels, then averaged.
    ///
    /// For grayscale images this is simply the single-channel SSIM.
    #[default]
    RgbAveraged,
    /// SSIM of Rec.709 luma (`0.2126 R + 0.7152 G + 0.0722 B`).
    ///
    /// For grayscale images the gray channel is treated as luma directly.
    Luma709,
}

/// Options controlling an SSIM computation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SsimOptions {
    /// The signal SSIM is computed over.
    pub mode: SsimMode,
}

/// Computes the mean SSIM between `reference` and `distorted`.
///
/// Both images share the pixel format `F`, so a difference in color space,
/// channel layout, or bit depth is rejected by the compiler rather than at
/// run time. An alpha channel, if present, is ignored. The score ranges over
/// `(-1, 1]`; higher is better, and pixel-identical images score exactly `1.0`.
/// Each image must be at least 11x11 (the size of the Gaussian window).
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 11 pixels.
///
/// # Examples
///
/// ```
/// use iqa::{Image, SsimOptions, ssim};
///
/// let reference = Image::srgb8(11, 11, vec![128; 11 * 11 * 3])?;
/// let distorted = Image::srgb8(11, 11, vec![130; 11 * 11 * 3])?;
/// let score = ssim(&reference, &distorted, SsimOptions::default())?;
/// assert!(score <= 1.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa::{Image, SsimOptions, ssim};
///
/// let rgb = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let gray = Image::gray8(11, 11, vec![0; 11 * 11])?;
/// let _ = ssim(&rgb, &gray, SsimOptions::default()); // channel mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// ```compile_fail
/// use iqa::{Image, SsimOptions, ssim};
///
/// let eight = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let sixteen = Image::srgb16(11, 11, vec![0; 11 * 11 * 3])?;
/// let _ = ssim(&eight, &sixteen, SsimOptions::default()); // bit-depth mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn ssim<F: PixelFormat>(
    reference: &Image<F>,
    distorted: &Image<F>,
    opts: SsimOptions,
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

    let window = gaussian_window();
    let l = <F::Sample as Sample>::MAX;
    let c1 = (K1 * l).powi(2);
    let c2 = (K2 * l).powi(2);

    let score = match opts.mode {
        SsimMode::RgbAveraged => {
            let channels = color_channels(F::CHANNELS);
            let total: f64 = channels
                .iter()
                .map(|&c| {
                    ssim_map_mean(
                        width,
                        height,
                        &window,
                        c1,
                        c2,
                        |x, y| reference.sample_at(x, y, c),
                        |x, y| distorted.sample_at(x, y, c),
                    )
                })
                .sum();
            total / channels.len() as f64
        }
        SsimMode::Luma709 => ssim_map_mean(
            width,
            height,
            &window,
            c1,
            c2,
            |x, y| luma(reference, x, y),
            |x, y| luma(distorted, x, y),
        ),
    };

    Ok(score)
}

/// Channel indices contributing to a color computation (alpha excluded).
pub(crate) fn color_channels(channels: Channels) -> &'static [usize] {
    match channels {
        Channels::Gray => &[0],
        Channels::Rgb | Channels::Rgba => &[0, 1, 2],
    }
}

/// Rec.709 luma of the pixel at `(x, y)`.
pub(crate) fn luma<F: PixelFormat>(img: &Image<F>, x: u32, y: u32) -> f64 {
    match F::CHANNELS {
        Channels::Gray => img.sample_at(x, y, 0),
        Channels::Rgb | Channels::Rgba => {
            0.2126 * img.sample_at(x, y, 0)
                + 0.7152 * img.sample_at(x, y, 1)
                + 0.0722 * img.sample_at(x, y, 2)
        }
    }
}

/// Builds the normalized 11x11 Gaussian window (weights sum to 1.0).
///
/// The iteration order is fixed, so the weights are bit-identical on every
/// run — the determinism the metric guarantees depends on it.
pub(crate) fn gaussian_window() -> [[f64; WINDOW]; WINDOW] {
    let center = (WINDOW as f64 - 1.0) / 2.0;
    let mut window = [[0.0f64; WINDOW]; WINDOW];
    let mut sum = 0.0;
    for (j, row) in window.iter_mut().enumerate() {
        for (i, w) in row.iter_mut().enumerate() {
            let dx = i as f64 - center;
            let dy = j as f64 - center;
            *w = (-(dx * dx + dy * dy) / (2.0 * SIGMA * SIGMA)).exp();
            sum += *w;
        }
    }
    for row in &mut window {
        for w in row {
            *w /= sum;
        }
    }
    window
}

/// Mean of the SSIM map over every fully covered window position.
///
/// `a` and `b` sample the signal (one channel, or luma) of the reference and
/// distorted images at a pixel. The result is bit-identical to the first
/// component of [`ssim_cs_map_means`], which this delegates to. The
/// minimum-dimension check in [`ssim`] guarantees at least one position.
fn ssim_map_mean(
    width: u32,
    height: u32,
    window: &[[f64; WINDOW]; WINDOW],
    c1: f64,
    c2: f64,
    a: impl Fn(u32, u32) -> f64,
    b: impl Fn(u32, u32) -> f64,
) -> f64 {
    ssim_cs_map_means(width, height, window, c1, c2, a, b).0
}

/// Means of the full-SSIM map and the contrast-structure (`cs`) map over every
/// fully covered window position, returned as `(full_ssim_mean, cs_mean)`.
///
/// MS-SSIM needs the contrast-structure term
/// `cs = (2σxy + C2) / (σx² + σy² + C2)` on its own — separate from the full
/// SSIM, which also folds in the luminance term — so this exposes both in a
/// single pass. The full-SSIM mean is bit-identical to what [`ssim_map_mean`]
/// returns. Positions are visited in a fixed row-major order, so the
/// accumulation — and hence the result — is deterministic.
pub(crate) fn ssim_cs_map_means(
    width: u32,
    height: u32,
    window: &[[f64; WINDOW]; WINDOW],
    c1: f64,
    c2: f64,
    a: impl Fn(u32, u32) -> f64,
    b: impl Fn(u32, u32) -> f64,
) -> (f64, f64) {
    let mut sum_full = 0.0;
    let mut sum_cs = 0.0;
    let mut count = 0usize;
    for y0 in 0..=(height - WINDOW as u32) {
        for x0 in 0..=(width - WINDOW as u32) {
            let (full, cs) = ssim_at(window, c1, c2, x0, y0, &a, &b);
            sum_full += full;
            sum_cs += cs;
            count += 1;
        }
    }
    (sum_full / count as f64, sum_cs / count as f64)
}

/// Full SSIM and its contrast-structure term `cs` at the single window whose
/// top-left corner is `(x0, y0)`, returned as `(full, cs)`.
///
/// Two passes: the first accumulates the Gaussian-weighted means, the second
/// the weighted variances and covariance from deviations about those means.
/// The deviation form keeps the variance terms at exactly zero for identical
/// input, so an identical pair scores exactly `1.0` even at 16-bit, which the
/// algebraically equivalent `E[x²] − E[x]²` form would not. The full SSIM is
/// computed as one fraction (not `luminance * cs`) to keep its value
/// bit-identical to the original single-term form.
fn ssim_at<A, B>(
    window: &[[f64; WINDOW]; WINDOW],
    c1: f64,
    c2: f64,
    x0: u32,
    y0: u32,
    a: &A,
    b: &B,
) -> (f64, f64)
where
    A: Fn(u32, u32) -> f64,
    B: Fn(u32, u32) -> f64,
{
    let mut mu_x = 0.0;
    let mut mu_y = 0.0;
    for (j, row) in window.iter().enumerate() {
        for (i, &w) in row.iter().enumerate() {
            let x = x0 + i as u32;
            let y = y0 + j as u32;
            mu_x += w * a(x, y);
            mu_y += w * b(x, y);
        }
    }

    let mut var_x = 0.0;
    let mut var_y = 0.0;
    let mut cov = 0.0;
    for (j, row) in window.iter().enumerate() {
        for (i, &w) in row.iter().enumerate() {
            let x = x0 + i as u32;
            let y = y0 + j as u32;
            let dx = a(x, y) - mu_x;
            let dy = b(x, y) - mu_y;
            var_x += w * (dx * dx);
            var_y += w * (dy * dy);
            cov += w * (dx * dy);
        }
    }

    // The products are grouped so that swapping the two images — which swaps
    // `mu_x`/`mu_y` and `dx`/`dy` — leaves every term bit-identical, making the
    // metric exactly symmetric rather than symmetric up to rounding.
    let full = ((2.0 * (mu_x * mu_y) + c1) * (2.0 * cov + c2))
        / ((mu_x * mu_x + mu_y * mu_y + c1) * (var_x + var_y + c2));
    let cs = (2.0 * cov + c2) / (var_x + var_y + c2);
    (full, cs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Image;

    /// An 11x11 solid sRGB image filled with `value` in every color channel.
    fn solid_srgb8(value: u8) -> Image<crate::format::Srgb8> {
        Image::srgb8(11, 11, vec![value; 11 * 11 * 3]).unwrap()
    }

    #[test]
    fn identical_images_are_one() {
        let img = solid_srgb8(100);
        let score = ssim(&img, &img, SsimOptions::default()).unwrap();
        assert!((score - 1.0).abs() < 1e-12, "expected 1.0, got {score}");
    }

    #[test]
    fn gaussian_window_sums_to_one_and_is_symmetric() {
        let window = gaussian_window();
        let sum: f64 = window.iter().flatten().sum();
        assert!((sum - 1.0).abs() < 1e-12, "window sum was {sum}");
        // Symmetric about both axes, with the maximum at the center.
        for j in 0..WINDOW {
            for i in 0..WINDOW {
                assert!((window[j][i] - window[i][j]).abs() < 1e-18);
                assert!(window[5][5] >= window[j][i]);
            }
        }
    }

    #[test]
    fn solid_offset_matches_closed_form() {
        // With both images uniform, every window has zero variance and
        // covariance, so SSIM collapses to (2ab + C1) / (a² + b² + C1).
        let reference = solid_srgb8(100);
        let distorted = solid_srgb8(120);
        let score = ssim(&reference, &distorted, SsimOptions::default()).unwrap();
        let c1 = (K1 * 255.0_f64).powi(2);
        let expected = (2.0 * 100.0 * 120.0 + c1) / (100.0 * 100.0 + 120.0 * 120.0 + c1);
        assert!(
            (score - expected).abs() < 1e-9,
            "got {score}, want {expected}"
        );
    }

    #[test]
    fn distortion_lowers_the_score() {
        let reference = solid_srgb8(128);
        let mut data = vec![128u8; 11 * 11 * 3];
        for (i, sample) in data.iter_mut().enumerate() {
            if i % 7 == 0 {
                *sample = 160;
            }
        }
        let distorted = Image::srgb8(11, 11, data).unwrap();
        let score = ssim(&reference, &distorted, SsimOptions::default()).unwrap();
        assert!(score < 1.0, "distorted image scored {score}");
    }

    #[test]
    fn luma_weights_blue_channel_lightly() {
        // Distort only blue: Luma709 weights it at 0.0722, so its score stays
        // closer to 1.0 than the channel-averaged result.
        let reference = solid_srgb8(128);
        let mut data = vec![128u8; 11 * 11 * 3];
        for px in data.chunks_mut(3) {
            px[2] = 180; // blue channel
        }
        let distorted = Image::srgb8(11, 11, data).unwrap();
        let rgb = ssim(&reference, &distorted, SsimOptions::default()).unwrap();
        let luma = ssim(
            &reference,
            &distorted,
            SsimOptions {
                mode: SsimMode::Luma709,
            },
        )
        .unwrap();
        assert!(luma > rgb, "rgb={rgb}, luma={luma}");
    }

    #[test]
    fn image_below_window_is_rejected() {
        let img = Image::srgb8(10, 10, vec![0; 10 * 10 * 3]).unwrap();
        let err = ssim(&img, &img, SsimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::ImageTooSmall(10, 10, 11)));
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = Image::srgb8(11, 11, vec![0; 11 * 11 * 3]).unwrap();
        let b = Image::srgb8(12, 11, vec![0; 12 * 11 * 3]).unwrap();
        let err = ssim(&a, &b, SsimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }
}
