//! The image type accepted by every metric.
//!
//! [`Image`] is generic over a [`PixelFormat`] marker, so its color space,
//! channel layout, and bit depth are part of its type. Every metric requires
//! its two inputs to share one format, which turns a whole class of mistakes —
//! comparing images in different color spaces, bit depths, or channel layouts
//! — into compile errors. See the [`format`](crate::format) module.
//!
//! Converting external image formats (PNG, decoded video frames, ...) into an
//! [`Image`] is deliberately out of scope: a caller hands over an already
//! correct, tightly packed sample buffer through one of the named
//! constructors ([`Image::srgb8`], [`Image::gray16`], ...).

use std::marker::PhantomData;

use crate::error::{Error, Result};
use crate::format::{Gray8, Gray16, PixelFormat, Rgba8, Rgba16, Sample, Srgb8, Srgb16};

/// Bits per sample of an [`Image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// 8-bit samples.
    Eight,
    /// 16-bit samples.
    Sixteen,
}

impl BitDepth {
    /// The maximum representable sample value (the `MAX` term in PSNR).
    pub fn max_value(self) -> f64 {
        match self {
            BitDepth::Eight => 255.0,
            BitDepth::Sixteen => 65_535.0,
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
    /// Standard sRGB, with the sRGB transfer function applied.
    Srgb,
    /// Grayscale intensity.
    Grayscale,
}

/// A decoded image of pixel format `F`.
///
/// Sample data is row-major and tightly packed: each pixel stores
/// `F::CHANNELS` samples of type `F::Sample`, with no padding. The format `F`
/// fixes the color space, channel layout, and bit depth at the type level.
///
/// Construct an image through one of the named constructors — [`Image::srgb8`],
/// [`Image::srgb16`], [`Image::gray8`], [`Image::gray16`], [`Image::rgba8`],
/// [`Image::rgba16`] — which infer `F` for you.
#[derive(Debug, Clone)]
pub struct Image<F: PixelFormat> {
    width: u32,
    height: u32,
    data: Vec<F::Sample>,
    _format: PhantomData<F>,
}

impl<F: PixelFormat> Image<F> {
    /// Constructs an image from typed samples, validating the buffer length.
    ///
    /// This is the single shared constructor behind every named one.
    fn from_samples(width: u32, height: u32, data: Vec<F::Sample>) -> Result<Self> {
        let expected = width as usize * height as usize * F::CHANNELS.count();
        if data.len() != expected {
            return Err(Error::BufferSize {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
            _format: PhantomData,
        })
    }

    /// Image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Dimensions as a `(width, height)` pair.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Channel layout, as fixed by the format `F`.
    pub fn channels(&self) -> Channels {
        F::CHANNELS
    }

    /// Bit depth, as fixed by the format `F`.
    pub fn bit_depth(&self) -> BitDepth {
        F::BIT_DEPTH
    }

    /// Color space, as fixed by the format `F`.
    pub fn color_space(&self) -> ColorSpace {
        F::COLOR_SPACE
    }

    /// The raw, row-major, tightly packed sample buffer.
    pub fn samples(&self) -> &[F::Sample] {
        &self.data
    }

    /// Total number of samples stored (`width * height * channels`).
    pub fn sample_count(&self) -> usize {
        self.data.len()
    }

    /// Reads the sample at `(x, y)` for channel `c`, widened to `f64`.
    ///
    /// Callers must ensure `x < width`, `y < height`, and `c < channels`.
    pub(crate) fn sample_at(&self, x: u32, y: u32, c: usize) -> f64 {
        let channels = F::CHANNELS.count();
        let index = (y as usize * self.width as usize + x as usize) * channels + c;
        self.data[index].into()
    }

    /// Converts the image to tightly packed RGB `f32` samples normalized to
    /// `0.0..=1.0`, suitable for the SSIMULACRA2 FFI shim.
    ///
    /// Grayscale input is expanded to RGB by replicating the gray value; an
    /// alpha channel, if present, is dropped. The returned buffer has length
    /// `width * height * 3`.
    #[cfg_attr(not(feature = "ssimulacra2"), allow(dead_code))]
    pub(crate) fn to_rgb_f32_normalized(&self) -> Vec<f32> {
        let max = <F::Sample as Sample>::MAX;
        let pixels = self.width as usize * self.height as usize;
        let mut out = Vec::with_capacity(pixels * 3);
        for y in 0..self.height {
            for x in 0..self.width {
                let (r, g, b) = match F::CHANNELS {
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

impl Image<Srgb8> {
    /// Builds an 8-bit sRGB image: row-major, three samples per pixel (R, G, B).
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] unless `data.len() == width * height * 3`.
    pub fn srgb8(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        Self::from_samples(width, height, data)
    }
}

impl Image<Srgb16> {
    /// Builds a 16-bit sRGB image: row-major, three samples per pixel (R, G, B).
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] unless `data.len() == width * height * 3`.
    pub fn srgb16(width: u32, height: u32, data: Vec<u16>) -> Result<Self> {
        Self::from_samples(width, height, data)
    }
}

impl Image<Gray8> {
    /// Builds an 8-bit grayscale image: row-major, one sample per pixel.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] unless `data.len() == width * height`.
    pub fn gray8(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        Self::from_samples(width, height, data)
    }
}

impl Image<Gray16> {
    /// Builds a 16-bit grayscale image: row-major, one sample per pixel.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] unless `data.len() == width * height`.
    pub fn gray16(width: u32, height: u32, data: Vec<u16>) -> Result<Self> {
        Self::from_samples(width, height, data)
    }
}

impl Image<Rgba8> {
    /// Builds an 8-bit sRGB image with alpha: row-major, four samples per pixel
    /// (R, G, B, A).
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] unless `data.len() == width * height * 4`.
    pub fn rgba8(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        Self::from_samples(width, height, data)
    }
}

impl Image<Rgba16> {
    /// Builds a 16-bit sRGB image with alpha: row-major, four samples per pixel
    /// (R, G, B, A).
    ///
    /// # Errors
    ///
    /// Returns [`Error::BufferSize`] unless `data.len() == width * height * 4`.
    pub fn rgba16(width: u32, height: u32, data: Vec<u16>) -> Result<Self> {
        Self::from_samples(width, height, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_buffer_size() {
        let err = Image::srgb8(2, 2, vec![0; 10]).unwrap_err();
        assert!(matches!(
            err,
            Error::BufferSize {
                expected: 12,
                actual: 10
            }
        ));
    }

    #[test]
    fn sixteen_bit_samples_round_trip() {
        // No endianness handling: a `u16` goes in and comes back unchanged.
        let img = Image::gray16(1, 1, vec![258]).unwrap();
        assert_eq!(img.samples(), &[258]);
        assert_eq!(img.sample_at(0, 0, 0), 258.0);
    }

    #[test]
    fn to_rgb_f32_replicates_gray_and_normalizes() {
        let img = Image::gray8(1, 1, vec![255]).unwrap();
        assert_eq!(img.to_rgb_f32_normalized(), vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn accessors_reflect_the_format() {
        let img = Image::rgba16(2, 3, vec![0; 24]).unwrap();
        assert_eq!(img.dimensions(), (2, 3));
        assert_eq!(img.channels(), Channels::Rgba);
        assert_eq!(img.bit_depth(), BitDepth::Sixteen);
        assert_eq!(img.color_space(), ColorSpace::Srgb);
        assert_eq!(img.sample_count(), 24);
    }
}
