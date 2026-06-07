//! `iqa` provides a single, ergonomic API over the patchwork of visual
//! quality assessment metrics available in the Rust ecosystem.
//!
//! Every metric consumes the same [`Image<F>`](Image) type, parameterized by a
//! [`PixelFormat`] marker. The format fixes an image's color space, channel
//! layout, and bit depth at the type level, and a metric requires its two
//! inputs to share one format. Comparing images that differ in any of those
//! properties — a classic source of meaningless scores — is therefore a
//! compile error, not a silent surprise. Only a dimension mismatch, which
//! cannot be known at compile time, remains a runtime error.
//!
//! Decoding external image formats into an [`Image`] is out of scope: callers
//! build one directly from a sample buffer via [`Image::srgb8`] and friends.
//!
//! # Features
//!
//! Each metric is gated behind a Cargo feature:
//!
//! - `psnr` *(default)* — peak signal-to-noise ratio, a native implementation.
//! - `ssimulacra2` *(default)* — SSIMULACRA2, bound via FFI to the vendored
//!   C++ reference. Enabling it requires the `third_party/` git submodules and
//!   a system `lcms2`; see the project README.

#![deny(missing_docs)]

mod error;
mod format;
mod image;

pub use error::{Error, Result};
pub use format::{Gray8, Gray16, PixelFormat, Rgba8, Rgba16, Sample, Srgb8, Srgb16};
pub use image::{BitDepth, Channels, ColorSpace, Image};

#[cfg(feature = "psnr")]
mod psnr;
#[cfg(feature = "psnr")]
pub use psnr::{PsnrMode, PsnrOptions, psnr};

#[cfg(feature = "ssimulacra2")]
mod ssimulacra2;
#[cfg(feature = "ssimulacra2")]
pub use ssimulacra2::{Ssimulacra2Input, ssimulacra2};
