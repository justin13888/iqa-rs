//! Peak signal-to-noise ratio, human-visual-system, with masking (PSNR-HVS-M).
//!
//! PSNR-HVS-M is a native implementation of the metric by Ponomarenko et al.
//! (2007), the contrast-masking refinement of PSNR-HVS. It is the metric
//! Daala's `tools/dump_psnrhvs.c` ports; this implementation follows the
//! original reference (`psnrhvsm.m`) directly.
//!
//! Each image is partitioned into non-overlapping 8x8 blocks. Every block is
//! transformed with the orthonormal 2-D DCT-II, and the per-coefficient error
//! between reference and distorted blocks is weighted by a contrast-sensitivity
//! function (`CSF`). Before weighting, *between-coefficient contrast masking*
//! suppresses error that a human eye would not see against the block's own
//! energy: a masking threshold derived from each block's AC energy and local
//! variance (the larger of the reference's and distorted's) is subtracted from
//! each non-DC coefficient's error, flooring it at zero. The masked, CSF-weighted
//! squared errors are averaged over all coefficients of all blocks into a
//! perceptual MSE, then reported as `10 * log10(MAX^2 / MSE)` in decibels.
//!
//! Higher is better; pixel-identical images yield [`f64::INFINITY`]. The CSF and
//! masking tables are tuned for the 8-bit value range, so 16-bit input is scaled
//! into the 8-bit domain first, making the two bit depths consistent. An alpha
//! channel, if present, is ignored, and each image must be at least 8x8 (one
//! block).

use crate::error::{Error, Result};
use crate::format::{PixelFormat, Sample};
use crate::image::{Channels, Image};

/// Side length of the (square) DCT block.
const BLOCK: usize = 8;
/// Step between block origins. Equal to [`BLOCK`], so blocks do not overlap —
/// the reference's default `wstep`.
const STEP: usize = 8;
/// Smallest width/height accepted: one full block must fit.
const MIN_DIMENSION: u32 = 8;

/// Contrast-sensitivity weights for the 8x8 DCT coefficients, from the reference
/// `psnrhvsm.m` (`CSFCof`). Indexed `[row][col]` matching DCT `[vertical][horizontal]`.
#[rustfmt::skip]
const CSF_COF: [[f64; 8]; 8] = [
    [1.608443, 2.339554, 2.573509, 1.608443, 1.072295, 0.643377, 0.504610, 0.421887],
    [2.144591, 2.144591, 1.838221, 1.354478, 0.989811, 0.443708, 0.428918, 0.467911],
    [1.838221, 1.979622, 1.608443, 1.072295, 0.643377, 0.451493, 0.372972, 0.459555],
    [1.838221, 1.513829, 1.169777, 0.887417, 0.504610, 0.295806, 0.321689, 0.415082],
    [1.429727, 1.169777, 0.695543, 0.459555, 0.378457, 0.236102, 0.249855, 0.334222],
    [1.072295, 0.735288, 0.467911, 0.402111, 0.317717, 0.247453, 0.227744, 0.279729],
    [0.525206, 0.402111, 0.329937, 0.295806, 0.249855, 0.212687, 0.214459, 0.254803],
    [0.357432, 0.279729, 0.270896, 0.262603, 0.229778, 0.257351, 0.249855, 0.259950],
];

/// Masking-weight table for the 8x8 DCT coefficients, from the reference
/// `psnrhvsm.m` (`MaskCof`). The masking threshold for a coefficient is
/// `block_mask / MASK_COF[k][l]`.
#[rustfmt::skip]
const MASK_COF: [[f64; 8]; 8] = [
    [0.390625, 0.826446, 1.000000, 0.390625, 0.173611, 0.062500, 0.038447, 0.026874],
    [0.694444, 0.694444, 0.510204, 0.277008, 0.147929, 0.029727, 0.027778, 0.033058],
    [0.510204, 0.591716, 0.390625, 0.173611, 0.062500, 0.030779, 0.021004, 0.031888],
    [0.510204, 0.346021, 0.206612, 0.118906, 0.038447, 0.013212, 0.015625, 0.026015],
    [0.308642, 0.206612, 0.073046, 0.031888, 0.021626, 0.008417, 0.009426, 0.016866],
    [0.173611, 0.081633, 0.033058, 0.024414, 0.015242, 0.009246, 0.007831, 0.011815],
    [0.041649, 0.024414, 0.016437, 0.013212, 0.009426, 0.006830, 0.006944, 0.009803],
    [0.019290, 0.011815, 0.011080, 0.010412, 0.007972, 0.010000, 0.009426, 0.010203],
];

/// Which signal PSNR-HVS-M is computed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PsnrHvsMode {
    /// Computed independently for the R, G, and B channels, with the masked
    /// errors pooled across channels into a single perceptual MSE.
    ///
    /// The luma CSF/masking tables are applied to each channel; for grayscale
    /// images this is simply the single-channel metric.
    #[default]
    RgbAveraged,
    /// Computed on Rec.709 luma (`0.2126 R + 0.7152 G + 0.0722 B`).
    ///
    /// For grayscale images the gray channel is treated as luma directly.
    Luma709,
}

/// Options controlling a PSNR-HVS-M computation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PsnrHvsOptions {
    /// The signal PSNR-HVS-M is computed over.
    pub mode: PsnrHvsMode,
}

/// Computes the PSNR-HVS-M (in decibels) between `reference` and `distorted`.
///
/// Both images share the pixel format `F`, so a difference in color space,
/// channel layout, or bit depth is rejected by the compiler rather than at
/// run time. An alpha channel, if present, is ignored. Returns
/// [`f64::INFINITY`] when the two images are pixel-identical. Each image must be
/// at least 8x8 (one DCT block).
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 8 pixels.
///
/// # Examples
///
/// ```
/// use iqa::{Image, PsnrHvsOptions, psnr_hvs_m};
///
/// let reference = Image::srgb8(8, 8, vec![128; 8 * 8 * 3])?;
/// let distorted = Image::srgb8(8, 8, vec![130; 8 * 8 * 3])?;
/// let score = psnr_hvs_m(&reference, &distorted, PsnrHvsOptions::default())?;
/// assert!(score > 0.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa::{Image, PsnrHvsOptions, psnr_hvs_m};
///
/// let rgb = Image::srgb8(8, 8, vec![0; 8 * 8 * 3])?;
/// let gray = Image::gray8(8, 8, vec![0; 8 * 8])?;
/// let _ = psnr_hvs_m(&rgb, &gray, PsnrHvsOptions::default()); // channel mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// ```compile_fail
/// use iqa::{Image, PsnrHvsOptions, psnr_hvs_m};
///
/// let eight = Image::srgb8(8, 8, vec![0; 8 * 8 * 3])?;
/// let sixteen = Image::srgb16(8, 8, vec![0; 8 * 8 * 3])?;
/// let _ = psnr_hvs_m(&eight, &sixteen, PsnrHvsOptions::default()); // bit-depth mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn psnr_hvs_m<F: PixelFormat>(
    reference: &Image<F>,
    distorted: &Image<F>,
    opts: PsnrHvsOptions,
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

    // The CSF/masking tables are tuned for 8-bit amplitudes; scale samples into
    // that domain so 8-bit and 16-bit inputs of the same content score alike.
    let scale = 255.0 / <F::Sample as Sample>::MAX;
    let basis = dct_basis();

    let mut sse = 0.0;
    let mut count = 0u64;
    match opts.mode {
        PsnrHvsMode::RgbAveraged => {
            for &c in color_channels(F::CHANNELS) {
                let r = channel_buffer(reference, c, scale);
                let d = channel_buffer(distorted, c, scale);
                accumulate(&r, &d, width, height, &basis, &mut sse, &mut count);
            }
        }
        PsnrHvsMode::Luma709 => {
            let r = luma_buffer(reference, scale);
            let d = luma_buffer(distorted, scale);
            accumulate(&r, &d, width, height, &basis, &mut sse, &mut count);
        }
    }

    if count == 0 {
        return Ok(f64::INFINITY);
    }
    let mse = sse / count as f64;
    if mse == 0.0 {
        return Ok(f64::INFINITY);
    }
    Ok(10.0 * (255.0 * 255.0 / mse).log10())
}

/// Channel indices contributing to a color computation (alpha excluded).
fn color_channels(channels: Channels) -> &'static [usize] {
    match channels {
        Channels::Gray => &[0],
        Channels::Rgb | Channels::Rgba => &[0, 1, 2],
    }
}

/// Extracts channel `c` of `img` into a row-major `f64` buffer, scaled into the
/// 8-bit value domain.
fn channel_buffer<F: PixelFormat>(img: &Image<F>, c: usize, scale: f64) -> Vec<f64> {
    let (width, height) = img.dimensions();
    let mut buf = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            buf.push(img.sample_at(x, y, c) * scale);
        }
    }
    buf
}

/// Extracts the Rec.709 luma of `img` into a row-major `f64` buffer, scaled into
/// the 8-bit value domain.
fn luma_buffer<F: PixelFormat>(img: &Image<F>, scale: f64) -> Vec<f64> {
    let (width, height) = img.dimensions();
    let mut buf = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            let v = match F::CHANNELS {
                Channels::Gray => img.sample_at(x, y, 0),
                Channels::Rgb | Channels::Rgba => {
                    0.2126 * img.sample_at(x, y, 0)
                        + 0.7152 * img.sample_at(x, y, 1)
                        + 0.0722 * img.sample_at(x, y, 2)
                }
            };
            buf.push(v * scale);
        }
    }
    buf
}

/// Accumulates the masked, CSF-weighted squared error and coefficient count over
/// every non-overlapping 8x8 block of one signal.
fn accumulate(
    reference: &[f64],
    distorted: &[f64],
    width: u32,
    height: u32,
    basis: &[[f64; BLOCK]; BLOCK],
    sse: &mut f64,
    count: &mut u64,
) {
    let stride = width as usize;
    let (w, h) = (width as usize, height as usize);

    let mut y = 0;
    while y + BLOCK <= h {
        let mut x = 0;
        while x + BLOCK <= w {
            let mut a = [[0.0; BLOCK]; BLOCK];
            let mut b = [[0.0; BLOCK]; BLOCK];
            for (r, (arow, brow)) in a.iter_mut().zip(b.iter_mut()).enumerate() {
                let base = (y + r) * stride + x;
                for (cc, (av, bv)) in arow.iter_mut().zip(brow.iter_mut()).enumerate() {
                    *av = reference[base + cc];
                    *bv = distorted[base + cc];
                }
            }

            let a_dct = fdct8x8(&a, basis);
            let b_dct = fdct8x8(&b, basis);
            let mask = maskeff(&a, &a_dct).max(maskeff(&b, &b_dct));

            for k in 0..BLOCK {
                for l in 0..BLOCK {
                    let mut u = (a_dct[k][l] - b_dct[k][l]).abs();
                    if k != 0 || l != 0 {
                        let threshold = mask / MASK_COF[k][l];
                        u = if u < threshold { 0.0 } else { u - threshold };
                    }
                    let e = u * CSF_COF[k][l];
                    *sse += e * e;
                    *count += 1;
                }
            }

            x += STEP;
        }
        y += STEP;
    }
}

/// Builds the 8x8 orthonormal DCT-II basis matrix `C`, so a block's transform is
/// `C · block · Cᵀ` (matching MATLAB `dct2`, which the CSF tables are tuned to).
fn dct_basis() -> [[f64; BLOCK]; BLOCK] {
    let n = BLOCK as f64;
    let mut c = [[0.0; BLOCK]; BLOCK];
    for (k, row) in c.iter_mut().enumerate() {
        let alpha = if k == 0 {
            (1.0 / n).sqrt()
        } else {
            (2.0 / n).sqrt()
        };
        for (x, value) in row.iter_mut().enumerate() {
            *value = alpha
                * (std::f64::consts::PI * (2.0 * x as f64 + 1.0) * k as f64 / (2.0 * n)).cos();
        }
    }
    c
}

/// Forward 8x8 orthonormal DCT-II of `block`, computed as `C · block · Cᵀ`.
fn fdct8x8(block: &[[f64; BLOCK]; BLOCK], c: &[[f64; BLOCK]; BLOCK]) -> [[f64; BLOCK]; BLOCK] {
    // tmp = C · block
    let mut tmp = [[0.0; BLOCK]; BLOCK];
    for k in 0..BLOCK {
        for j in 0..BLOCK {
            let mut s = 0.0;
            for m in 0..BLOCK {
                s += c[k][m] * block[m][j];
            }
            tmp[k][j] = s;
        }
    }
    // out = tmp · Cᵀ  =>  out[k][l] = Σ_j tmp[k][j] · C[l][j]
    let mut out = [[0.0; BLOCK]; BLOCK];
    for k in 0..BLOCK {
        for l in 0..BLOCK {
            let mut s = 0.0;
            for j in 0..BLOCK {
                s += tmp[k][j] * c[l][j];
            }
            out[k][l] = s;
        }
    }
    out
}

/// Between-coefficient contrast-masking energy of one block (reference
/// `maskeff`): AC energy weighted by `MASK_COF`, scaled by the block's
/// quadrant-to-whole variance ratio, normalized by 32.
fn maskeff(spatial: &[[f64; BLOCK]; BLOCK], dct: &[[f64; BLOCK]; BLOCK]) -> f64 {
    let mut m = 0.0;
    for k in 0..BLOCK {
        for l in 0..BLOCK {
            if k != 0 || l != 0 {
                m += dct[k][l] * dct[k][l] * MASK_COF[k][l];
            }
        }
    }

    let whole: Vec<f64> = spatial.iter().flatten().copied().collect();
    let var_whole = vari(&whole);
    // `pop` is the energy spread across the four 4x4 quadrants relative to the
    // whole block; a flat block (zero variance) gets no masking.
    let pop = if var_whole != 0.0 {
        let quadrant = |r0: usize, c0: usize| {
            let mut q = Vec::with_capacity(16);
            for row in spatial.iter().skip(r0).take(4) {
                q.extend_from_slice(&row[c0..c0 + 4]);
            }
            vari(&q)
        };
        (quadrant(0, 0) + quadrant(0, 4) + quadrant(4, 4) + quadrant(4, 0)) / var_whole
    } else {
        0.0
    };

    (m * pop).sqrt() / 32.0
}

/// `var(vals) * len(vals)`, matching the reference's `vari`. MATLAB `var` uses
/// the `N-1` (sample) denominator, so this is `Σ(v − mean)² · N / (N − 1)`.
fn vari(vals: &[f64]) -> f64 {
    let n = vals.len() as f64;
    let mean = vals.iter().sum::<f64>() / n;
    let ssd: f64 = vals.iter().map(|v| (v - mean) * (v - mean)).sum();
    ssd * n / (n - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Image;

    fn opts() -> PsnrHvsOptions {
        PsnrHvsOptions::default()
    }

    /// A structured 8-bit gray image (a couple of frequencies), so blocks have
    /// real AC energy for masking to act on.
    fn structured(width: u32, height: u32) -> Image<crate::format::Gray8> {
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let v = 128.0 + 60.0 * (x as f64 * 0.5).sin() + 40.0 * (y as f64 * 0.33).cos();
                data.push(v.round().clamp(0.0, 255.0) as u8);
            }
        }
        Image::gray8(width, height, data).unwrap()
    }

    #[test]
    fn identical_images_are_infinite() {
        let img = structured(16, 16);
        assert_eq!(psnr_hvs_m(&img, &img, opts()).unwrap(), f64::INFINITY);
    }

    #[test]
    fn black_versus_white_is_finite_and_very_low() {
        let black = Image::gray8(16, 16, vec![0; 16 * 16]).unwrap();
        let white = Image::gray8(16, 16, vec![255; 16 * 16]).unwrap();
        let score = psnr_hvs_m(&black, &white, opts()).unwrap();
        // The maximal DC error is never masked and the CSF weights it above 1, so
        // the perceptual MSE exceeds the 255² reference and the score goes
        // negative — PSNR-HVS-M, unlike plain PSNR, is not floored at 0 dB. (The
        // reference psnrhvsm.m produces the same.)
        assert!(score.is_finite() && score < 0.0, "got {score}");
    }

    #[test]
    fn distortion_lowers_the_score() {
        let reference = structured(32, 32);
        let mut scores = Vec::new();
        for amp in [4i32, 16, 48] {
            let mut data: Vec<u8> = reference.samples().to_vec();
            for (i, s) in data.iter_mut().enumerate() {
                let d = if i % 2 == 0 { amp } else { -amp };
                *s = (*s as i32 + d).clamp(0, 255) as u8;
            }
            let distorted = Image::gray8(32, 32, data).unwrap();
            scores.push(psnr_hvs_m(&reference, &distorted, opts()).unwrap());
        }
        assert!(
            scores[0] > scores[1] && scores[1] > scores[2],
            "not monotonic: {scores:?}",
        );
    }

    #[test]
    fn masking_hides_error_in_busy_blocks() {
        // The same error added to a flat block is more visible (lower score)
        // than added to a high-energy block, which masks it.
        let flat = Image::gray8(8, 8, vec![128; 64]).unwrap();
        let busy = structured(8, 8);

        let perturb = |img: &Image<crate::format::Gray8>| {
            let mut data = img.samples().to_vec();
            for (i, s) in data.iter_mut().enumerate() {
                let d = if i % 2 == 0 { 12 } else { -12 };
                *s = (*s as i32 + d).clamp(0, 255) as u8;
            }
            Image::gray8(8, 8, data).unwrap()
        };

        let flat_score = psnr_hvs_m(&flat, &perturb(&flat), opts()).unwrap();
        let busy_score = psnr_hvs_m(&busy, &perturb(&busy), opts()).unwrap();
        assert!(
            busy_score > flat_score,
            "masking failed: busy={busy_score}, flat={flat_score}",
        );
    }

    #[test]
    fn image_below_block_is_rejected() {
        let img = Image::gray8(7, 7, vec![0; 49]).unwrap();
        let err = psnr_hvs_m(&img, &img, opts()).unwrap_err();
        assert!(matches!(err, Error::ImageTooSmall(7, 7, 8)));
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = Image::gray8(8, 8, vec![0; 64]).unwrap();
        let b = Image::gray8(8, 16, vec![0; 128]).unwrap();
        let err = psnr_hvs_m(&a, &b, opts()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }
}
