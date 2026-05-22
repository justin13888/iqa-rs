//! Peak signal-to-noise ratio (PSNR).
//!
//! PSNR is a native implementation. It reports `10 * log10(MAX^2 / MSE)` in
//! decibels, where `MAX` is derived from the image bit depth and `MSE` is the
//! mean squared error. Identical images yield [`f64::INFINITY`].

use crate::error::{Error, Result};
use crate::image::{Channels, Image};

/// Which signal PSNR is computed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PsnrMode {
    /// Mean squared error pooled across the R, G, and B channels.
    ///
    /// For grayscale images this is simply the single-channel error.
    #[default]
    RgbAveraged,
    /// Mean squared error of Rec.709 luma (`0.2126 R + 0.7152 G + 0.0722 B`).
    ///
    /// For grayscale images the gray channel is treated as luma directly.
    Luma709,
}

/// Options controlling a PSNR computation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PsnrOptions {
    /// The signal PSNR is computed over.
    pub mode: PsnrMode,
}

/// Computes the PSNR (in decibels) between `reference` and `distorted`.
///
/// An alpha channel, if present, is ignored. Returns [`f64::INFINITY`] when the
/// two images are pixel-identical.
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::Incompatible`] if they differ in channel layout or bit depth.
pub fn psnr(reference: &Image, distorted: &Image, opts: PsnrOptions) -> Result<f64> {
    if reference.dimensions() != distorted.dimensions() {
        return Err(Error::DimensionMismatch {
            a: reference.dimensions(),
            b: distorted.dimensions(),
        });
    }
    if reference.channels != distorted.channels {
        return Err(Error::Incompatible(format!(
            "channel layout {:?} vs {:?}",
            reference.channels, distorted.channels
        )));
    }
    if reference.bit_depth != distorted.bit_depth {
        return Err(Error::Incompatible(format!(
            "bit depth {:?} vs {:?}",
            reference.bit_depth, distorted.bit_depth
        )));
    }

    let mse = match opts.mode {
        PsnrMode::RgbAveraged => mse_rgb(reference, distorted),
        PsnrMode::Luma709 => mse_luma(reference, distorted),
    };

    if mse == 0.0 {
        return Ok(f64::INFINITY);
    }
    let max = reference.bit_depth.max_value();
    Ok(10.0 * (max * max / mse).log10())
}

/// Channel indices contributing to a color computation (alpha excluded).
fn color_channels(channels: Channels) -> &'static [usize] {
    match channels {
        Channels::Gray => &[0],
        Channels::Rgb | Channels::Rgba => &[0, 1, 2],
    }
}

/// Mean squared error pooled across all color channels.
fn mse_rgb(a: &Image, b: &Image) -> f64 {
    let channels = color_channels(a.channels);
    let mut sum = 0.0;
    let mut count = 0usize;
    for y in 0..a.height {
        for x in 0..a.width {
            for &c in channels {
                let d = a.sample_at(x, y, c) - b.sample_at(x, y, c);
                sum += d * d;
                count += 1;
            }
        }
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

/// Rec.709 luma of the pixel at `(x, y)`.
fn luma(img: &Image, x: u32, y: u32) -> f64 {
    match img.channels {
        Channels::Gray => img.sample_at(x, y, 0),
        Channels::Rgb | Channels::Rgba => {
            0.2126 * img.sample_at(x, y, 0)
                + 0.7152 * img.sample_at(x, y, 1)
                + 0.0722 * img.sample_at(x, y, 2)
        }
    }
}

/// Mean squared error of Rec.709 luma.
fn mse_luma(a: &Image, b: &Image) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for y in 0..a.height {
        for x in 0..a.width {
            let d = luma(a, x, y) - luma(b, x, y);
            sum += d * d;
            count += 1;
        }
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{BitDepth, ColorSpace};

    fn rgb8(width: u32, height: u32, data: Vec<u8>) -> Image {
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

    #[test]
    fn identical_images_are_infinite() {
        let img = rgb8(2, 2, vec![10; 12]);
        let result = psnr(&img, &img, PsnrOptions::default()).unwrap();
        assert_eq!(result, f64::INFINITY);
    }

    #[test]
    fn black_versus_white_is_zero_db() {
        // MSE = 255^2 = 65025, so PSNR = 10*log10(65025/65025) = 0 dB exactly.
        let black = rgb8(2, 2, vec![0; 12]);
        let white = rgb8(2, 2, vec![255; 12]);
        let result = psnr(&black, &white, PsnrOptions::default()).unwrap();
        assert!(result.abs() < 1e-12, "expected 0 dB, got {result}");
    }

    #[test]
    fn single_sample_off_by_one() {
        // 2x2 RGB = 12 samples; one sample off by 1 -> MSE = 1/12.
        let reference = rgb8(2, 2, vec![100; 12]);
        let mut data = vec![100; 12];
        data[0] = 101;
        let distorted = rgb8(2, 2, data);
        let result = psnr(&reference, &distorted, PsnrOptions::default()).unwrap();
        let expected = 10.0 * (255.0_f64 * 255.0 / (1.0 / 12.0)).log10();
        assert!((result - expected).abs() < 1e-9, "got {result}");
    }

    #[test]
    fn higher_bit_depth_raises_psnr() {
        // The same off-by-1 error scores far higher at 16-bit (larger MAX).
        let ref8 = rgb8(1, 1, vec![100, 100, 100]);
        let mut d8 = vec![100, 100, 100];
        d8[0] = 101;
        let dist8 = rgb8(1, 1, d8);
        let psnr8 = psnr(&ref8, &dist8, PsnrOptions::default()).unwrap();

        let ref16 = Image::new(
            1,
            1,
            Channels::Rgb,
            BitDepth::Sixteen,
            ColorSpace::Srgb,
            vec![100, 0, 100, 0, 100, 0],
        )
        .unwrap();
        let dist16 = Image::new(
            1,
            1,
            Channels::Rgb,
            BitDepth::Sixteen,
            ColorSpace::Srgb,
            vec![101, 0, 100, 0, 100, 0],
        )
        .unwrap();
        let psnr16 = psnr(&ref16, &dist16, PsnrOptions::default()).unwrap();

        assert!(psnr16 > psnr8 + 40.0, "psnr8={psnr8}, psnr16={psnr16}");
    }

    #[test]
    fn luma_weights_blue_channel_lightly() {
        // Distort only the blue channel: Luma709 weights B at 0.0722, so its
        // PSNR is much higher than the channel-pooled RgbAveraged result.
        let reference = rgb8(1, 1, vec![100, 100, 100]);
        let distorted = rgb8(1, 1, vec![100, 100, 150]);
        let rgb = psnr(&reference, &distorted, PsnrOptions::default()).unwrap();
        let luma = psnr(
            &reference,
            &distorted,
            PsnrOptions {
                mode: PsnrMode::Luma709,
            },
        )
        .unwrap();
        assert!(luma > rgb + 10.0, "rgb={rgb}, luma={luma}");
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = rgb8(2, 2, vec![0; 12]);
        let b = rgb8(1, 1, vec![0; 3]);
        let err = psnr(&a, &b, PsnrOptions::default()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }

    #[test]
    fn channel_mismatch_is_an_error() {
        let a = rgb8(1, 1, vec![0; 3]);
        let b = Image::new(
            1,
            1,
            Channels::Gray,
            BitDepth::Eight,
            ColorSpace::Grayscale,
            vec![0],
        )
        .unwrap();
        let err = psnr(&a, &b, PsnrOptions::default()).unwrap_err();
        assert!(matches!(err, Error::Incompatible(_)));
    }

    #[test]
    fn bit_depth_mismatch_is_an_error() {
        let a = rgb8(1, 1, vec![0; 3]);
        let b = Image::new(
            1,
            1,
            Channels::Rgb,
            BitDepth::Sixteen,
            ColorSpace::Srgb,
            vec![0; 6],
        )
        .unwrap();
        let err = psnr(&a, &b, PsnrOptions::default()).unwrap_err();
        assert!(matches!(err, Error::Incompatible(_)));
    }
}
