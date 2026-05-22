//! Raw FFI bindings to the SSIMULACRA2 C shim.
//!
//! The shim (`third_party/shim/ssimulacra2_shim.{h,cc}`) exposes a clean C ABI
//! around the vendored C++ reference; it is compiled by `build.rs` when the
//! `ssimulacra2` feature is enabled. The FFI surface is a single struct and a
//! single function, so it is declared by hand rather than generated.

use std::os::raw::{c_int, c_uint};

use crate::error::{Error, Result};

/// Mirror of the C `S2Result` struct from `ssimulacra2_shim.h`.
#[repr(C)]
struct S2Result {
    score: f64,
    ok: c_int,
}

unsafe extern "C" {
    /// See `iqa_ssimulacra2_rgb_f32` in `ssimulacra2_shim.h`.
    fn iqa_ssimulacra2_rgb_f32(
        orig_rgb: *const f32,
        dist_rgb: *const f32,
        width: c_uint,
        height: c_uint,
    ) -> S2Result;
}

/// Computes SSIMULACRA2 over two packed RGB `f32` buffers (samples in `0..=1`).
///
/// `orig` and `dist` must each contain exactly `width * height * 3` samples.
pub(super) fn compute(orig: &[f32], dist: &[f32], width: u32, height: u32) -> Result<f64> {
    let expected = width as usize * height as usize * 3;
    assert_eq!(orig.len(), expected, "orig buffer length");
    assert_eq!(dist.len(), expected, "dist buffer length");

    // SAFETY: both pointers reference `expected` initialized `f32`s, matching
    // the `width`/`height` passed alongside them; the shim only reads them.
    let result = unsafe { iqa_ssimulacra2_rgb_f32(orig.as_ptr(), dist.as_ptr(), width, height) };

    if result.ok != 0 {
        Ok(result.score)
    } else {
        Err(Error::Ssimulacra2Failed)
    }
}
