//! Pixel-format marker types and the [`PixelFormat`] trait.
//!
//! A *pixel format* bundles three properties — color space, channel layout,
//! and bit depth — into a single zero-sized marker type ([`Srgb8`], [`Gray16`],
//! ...). [`Image`](crate::Image) is generic over that marker, so those three
//! properties are part of an image's type.
//!
//! This is what makes the metrics dummy-proof: a metric requires its two
//! inputs to share one format, so comparing an sRGB image with a linear one,
//! an 8-bit image with a 16-bit one, or a color image with a grayscale one is
//! a *compile* error rather than a silent wrong answer. The only image
//! property left to a runtime check is its pixel dimensions, which cannot be
//! known at compile time.

use crate::image::{BitDepth, Channels, ColorSpace};

mod private {
    /// Seals [`super::Sample`] and [`super::PixelFormat`] so that the set of
    /// valid sample types and pixel formats stays closed and curated.
    pub trait Sealed {}
}

/// A raw image sample type: either [`u8`] or [`u16`].
///
/// This trait is sealed; the crate defines every implementor.
pub trait Sample: Copy + Into<f64> + private::Sealed + 'static {
    /// The largest representable value, as `f64` (the `MAX` term in PSNR).
    const MAX: f64;
    /// The bit depth corresponding to this sample type.
    const BIT_DEPTH: BitDepth;
}

impl private::Sealed for u8 {}
impl Sample for u8 {
    const MAX: f64 = 255.0;
    const BIT_DEPTH: BitDepth = BitDepth::Eight;
}

impl private::Sealed for u16 {}
impl Sample for u16 {
    const MAX: f64 = 65_535.0;
    const BIT_DEPTH: BitDepth = BitDepth::Sixteen;
}

/// A compile-time description of how an [`Image`](crate::Image)'s pixels are
/// laid out and interpreted.
///
/// Every implementor is a zero-sized marker type. Because
/// [`Image`](crate::Image) is generic over this trait, the color space,
/// channel layout, and bit depth of an image all live in its type, so a
/// metric can reject mismatched inputs at compile time.
///
/// This trait is sealed; the crate defines every valid format, so downstream
/// code cannot introduce nonsensical combinations.
pub trait PixelFormat: private::Sealed + Copy + 'static {
    /// The storage sample type, [`u8`] or [`u16`].
    type Sample: Sample;
    /// Channel layout.
    const CHANNELS: Channels;
    /// Color space the samples are encoded in.
    const COLOR_SPACE: ColorSpace;
    /// Bits per sample (mirrors [`Sample::BIT_DEPTH`] for convenience).
    const BIT_DEPTH: BitDepth;
}

/// Defines a zero-sized pixel-format marker and its [`PixelFormat`] impl.
macro_rules! pixel_format {
    ($(#[$doc:meta])* $name:ident, $sample:ty, $channels:expr, $color:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name;

        impl private::Sealed for $name {}
        impl PixelFormat for $name {
            type Sample = $sample;
            const CHANNELS: Channels = $channels;
            const COLOR_SPACE: ColorSpace = $color;
            const BIT_DEPTH: BitDepth = <$sample as Sample>::BIT_DEPTH;
        }
    };
}

pixel_format!(
    /// 8-bit sRGB, three channels per pixel (R, G, B).
    Srgb8, u8, Channels::Rgb, ColorSpace::Srgb
);
pixel_format!(
    /// 16-bit sRGB, three channels per pixel (R, G, B).
    Srgb16, u16, Channels::Rgb, ColorSpace::Srgb
);
pixel_format!(
    /// 8-bit grayscale, one channel per pixel.
    Gray8, u8, Channels::Gray, ColorSpace::Grayscale
);
pixel_format!(
    /// 16-bit grayscale, one channel per pixel.
    Gray16, u16, Channels::Gray, ColorSpace::Grayscale
);
pixel_format!(
    /// 8-bit sRGB with an alpha channel, four channels per pixel (R, G, B, A).
    Rgba8, u8, Channels::Rgba, ColorSpace::Srgb
);
pixel_format!(
    /// 16-bit sRGB with an alpha channel, four channels per pixel (R, G, B, A).
    Rgba16, u16, Channels::Rgba, ColorSpace::Srgb
);
