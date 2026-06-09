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
//! - `ssim` *(default)* — structural similarity index (SSIM), a native
//!   implementation.
//! - `dssim` *(default)* — structural dissimilarity, `(1 - SSIM) / 2`, a native
//!   implementation layered on `ssim` (which it enables).
//! - `ms-ssim` *(default)* — multi-scale SSIM (MS-SSIM), a native implementation
//!   that evaluates `ssim` over an image pyramid (and so enables `ssim`).
//! - `ssimulacra2` *(default)* — SSIMULACRA2, bound via FFI to the vendored
//!   C++ reference. Enabling it requires the `third_party/` git submodules and
//!   a C++ toolchain; see the project README.
//! - `butteraugli` *(default)* — Butteraugli, bound via FFI to libjxl's
//!   vendored C++. Shares the same native build as `ssimulacra2`.

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

#[cfg(feature = "ssim")]
mod ssim;
#[cfg(feature = "ssim")]
pub use ssim::{SsimMode, SsimOptions, ssim};

#[cfg(feature = "dssim")]
mod dssim;
#[cfg(feature = "dssim")]
pub use dssim::{DssimOptions, dssim};

#[cfg(feature = "ms-ssim")]
mod ms_ssim;
#[cfg(feature = "ms-ssim")]
pub use ms_ssim::{MsssimOptions, msssim};

#[cfg(feature = "ssimulacra2")]
mod ssimulacra2;
#[cfg(feature = "ssimulacra2")]
pub use ssimulacra2::{Ssimulacra2Input, ssimulacra2};

#[cfg(feature = "butteraugli")]
mod butteraugli;
#[cfg(feature = "butteraugli")]
pub use butteraugli::{ButteraugliInput, ButteraugliOptions, butteraugli};
