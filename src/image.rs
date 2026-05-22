//! The unified image type accepted by every metric.
//!
//! All metrics in this crate consume the same [`Image`] type so callers do not
//! have to juggle a different pixel representation for each one. An [`Image`]
//! holds tightly packed, row-major pixel data together with the metadata a
//! metric needs to interpret it: channel layout, bit depth, and color space.

use crate::error::{Error, Result};

/// Bits per sample of an [`Image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// 8-bit samples; one byte per sample.
    Eight,
    /// 16-bit samples; two little-endian bytes per sample.
    Sixteen,
}

impl BitDepth {
    /// Number of bytes used to store a single sample.
    pub fn bytes_per_sample(self) -> usize {
        match self {
            BitDepth::Eight => 1,
            BitDepth::Sixteen => 2,
        }
    }

    /// The maximum representable sample value (the `MAX` term in PSNR).
    pub fn max_value(self) -> f64 {
        match self {
            BitDepth::Eight => 255.0,
            BitDepth::Sixteen => 65535.0,
        }
    }
}

/// Channel layout of an [`Image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channels {
    /// A single grayscale channel.
    Gray,
    /// Three channels: red, green, blue.
    Rgb,
    /// Four channels: red, green, blue, alpha.
    Rgba,
}

impl Channels {
    /// Number of channels stored per pixel.
    pub fn count(self) -> usize {
        match self {
            Channels::Gray => 1,
            Channels::Rgb => 3,
            Channels::Rgba => 4,
        }
    }
}

/// Color space the pixel values are encoded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorSpace {
    /// Standard sRGB with the sRGB transfer function applied.
    Srgb,
    /// Linear-light RGB (sRGB primaries, no transfer function).
    LinearSrgb,
    /// Grayscale intensity.
    Grayscale,
}

/// A decoded image: pixel data plus the metadata needed to interpret it.
///
/// `data` is row-major and tightly packed. Each pixel stores
/// [`Channels::count`] samples, and each sample occupies
/// [`BitDepth::bytes_per_sample`] bytes (16-bit samples are little-endian).
#[derive(Debug, Clone)]
pub struct Image {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Channel layout.
    pub channels: Channels,
    /// Bits per sample.
    pub bit_depth: BitDepth,
    /// Color space the samples are encoded in.
    pub color_space: ColorSpace,
    /// Raw, row-major, tightly packed pixel bytes.
    pub data: Vec<u8>,
}

impl Image {
    /// Constructs an image, validating that `data` is exactly the size the
    /// dimensions and format require.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] if `data.len()` does not match
    /// `width * height * channels * bytes_per_sample`.
    pub fn new(
        width: u32,
        height: u32,
        channels: Channels,
        bit_depth: BitDepth,
        color_space: ColorSpace,
        data: Vec<u8>,
    ) -> Result<Self> {
        let expected =
            width as usize * height as usize * channels.count() * bit_depth.bytes_per_sample();
        if data.len() != expected {
            return Err(Error::BufferSize {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            channels,
            bit_depth,
            color_space,
            data,
        })
    }

    /// Total number of samples (`width * height * channels`).
    pub fn sample_count(&self) -> usize {
        self.width as usize * self.height as usize * self.channels.count()
    }

    /// Dimensions as a `(width, height)` pair.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Reads the raw sample at `(x, y)` for channel `c`, in `0..=max_value`.
    ///
    /// Callers must ensure `x < width`, `y < height`, and `c < channels`.
    pub(crate) fn sample_at(&self, x: u32, y: u32, c: usize) -> f64 {
        let channels = self.channels.count();
        let index = (y as usize * self.width as usize + x as usize) * channels + c;
        match self.bit_depth {
            BitDepth::Eight => self.data[index] as f64,
            BitDepth::Sixteen => {
                let lo = self.data[index * 2] as u16;
                let hi = self.data[index * 2 + 1] as u16;
                (lo | (hi << 8)) as f64
            }
        }
    }

    /// Converts the image to tightly packed RGB `f32` samples normalized to
    /// `0.0..=1.0`, suitable for the SSIMULACRA2 FFI shim.
    ///
    /// Grayscale input is expanded to RGB by replicating the gray value; an
    /// alpha channel, if present, is dropped. The returned buffer has length
    /// `width * height * 3`.
    #[cfg_attr(not(feature = "ssimulacra2"), allow(dead_code))]
    pub(crate) fn to_rgb_f32_normalized(&self) -> Vec<f32> {
        let max = self.bit_depth.max_value();
        let pixels = self.width as usize * self.height as usize;
        let mut out = Vec::with_capacity(pixels * 3);
        for y in 0..self.height {
            for x in 0..self.width {
                let (r, g, b) = match self.channels {
                    Channels::Gray => {
                        let v = self.sample_at(x, y, 0);
                        (v, v, v)
                    }
                    Channels::Rgb | Channels::Rgba => (
                        self.sample_at(x, y, 0),
                        self.sample_at(x, y, 1),
                        self.sample_at(x, y, 2),
                    ),
                };
                out.push((r / max) as f32);
                out.push((g / max) as f32);
                out.push((b / max) as f32);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_wrong_buffer_size() {
        let err = Image::new(
            2,
            2,
            Channels::Rgb,
            BitDepth::Eight,
            ColorSpace::Srgb,
            vec![0; 10],
        )
        .unwrap_err();
        assert!(matches!(
            err,
            Error::BufferSize {
                expected: 12,
                actual: 10
            }
        ));
    }

    #[test]
    fn sixteen_bit_samples_are_little_endian() {
        // One 1x1 grayscale pixel with value 0x0102 = 258.
        let img = Image::new(
            1,
            1,
            Channels::Gray,
            BitDepth::Sixteen,
            ColorSpace::Grayscale,
            vec![0x02, 0x01],
        )
        .unwrap();
        assert_eq!(img.sample_at(0, 0, 0), 258.0);
    }

    #[test]
    fn to_rgb_f32_replicates_gray_and_normalizes() {
        let img = Image::new(
            1,
            1,
            Channels::Gray,
            BitDepth::Eight,
            ColorSpace::Grayscale,
            vec![255],
        )
        .unwrap();
        assert_eq!(img.to_rgb_f32_normalized(), vec![1.0, 1.0, 1.0]);
    }
}
