//! `iqa-rs` provides a single, ergonomic API over the patchwork of visual
//! quality assessment metrics available in the Rust ecosystem.
//!
//! Every metric consumes the same [`Image`] type, so callers do not have to
//! juggle a different pixel representation, color space, and bit depth for
//! each one.
//!
//! # Features
//!
//! Each metric is gated behind a Cargo feature:
//!
//! - `psnr` *(default)* — peak signal-to-noise ratio, a native implementation.
//! - `ssimulacra2` — SSIMULACRA2, bound via FFI to the vendored C++ reference.
//!   Enabling it requires the `third_party/` git submodules and a system
//!   `lcms2`; see the project README.

#![deny(missing_docs)]

mod error;
mod image;

pub use error::{Error, Result};
pub use image::{BitDepth, Channels, ColorSpace, Image};

#[cfg(feature = "psnr")]
mod psnr;
#[cfg(feature = "psnr")]
pub use psnr::{PsnrMode, PsnrOptions, psnr};

#[cfg(feature = "ssimulacra2")]
mod ssimulacra2;
#[cfg(feature = "ssimulacra2")]
pub use ssimulacra2::ssimulacra2;
