//! Raw FFI bindings to the Butteraugli C shim.
//!
//! The shim (`third_party/shim/butteraugli_shim.{h,cc}`) exposes a clean C ABI
//! around libjxl's Butteraugli comparator; it is compiled by `build.rs` into
//! the shared native archive when the `butteraugli` feature is enabled. The FFI
//! surface is a single struct and a single function, so it is declared by hand
//! rather than generated.

use std::os::raw::{c_int, c_uint};

use crate::error::{Error, Result};

/// Mirror of the C `BaResult` struct from `butteraugli_shim.h`.
#[repr(C)]
struct BaResult {
    score: f64,
    ok: c_int,
}

unsafe extern "C" {
    /// See `iqa_butteraugli_rgb_f32` in `butteraugli_shim.h`.
    fn iqa_butteraugli_rgb_f32(
        orig_rgb: *const f32,
        dist_rgb: *const f32,
        width: c_uint,
        height: c_uint,
        intensity_target: f32,
        hf_asymmetry: f32,
        pnorm: f64,
    ) -> BaResult;
}

/// Computes the Butteraugli distance over two packed RGB `f32` buffers
/// (samples in `0..=1`). Lower is better; `0.0` means identical.
///
/// `orig` and `dist` must each contain exactly `width * height * 3` samples.
pub(super) fn compute(
    orig: &[f32],
    dist: &[f32],
    width: u32,
    height: u32,
    intensity_target: f32,
    hf_asymmetry: f32,
    pnorm: f64,
) -> Result<f64> {
    let expected = width as usize * height as usize * 3;
    assert_eq!(orig.len(), expected, "orig buffer length");
    assert_eq!(dist.len(), expected, "dist buffer length");

    // SAFETY: both pointers reference `expected` initialized `f32`s, matching
    // the `width`/`height` passed alongside them; the shim only reads them.
    let result = unsafe {
        iqa_butteraugli_rgb_f32(
            orig.as_ptr(),
            dist.as_ptr(),
            width,
            height,
            intensity_target,
            hf_asymmetry,
            pnorm,
        )
    };

    if result.ok != 0 {
        Ok(result.score)
    } else {
        Err(Error::ButteraugliFailed)
    }
}
