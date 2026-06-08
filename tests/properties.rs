//! Universal property battery for full-reference IQA metrics.
//!
//! [`run_property_suite`] encodes the rules any correct full-reference metric
//! must satisfy regardless of its algorithm. Because [`Image`] is generic over
//! its pixel format, the battery is dispatched at compile time: the
//! [`property_matrix!`] macro instantiates the suite once per pixel format for
//! each metric. A new metric inherits the whole battery by adding one module.

// With no metric features enabled there is nothing to instantiate the suite
// for, leaving the import and macro below genuinely unused.
#![cfg_attr(
    not(any(
        feature = "psnr",
        feature = "ssim",
        feature = "dssim",
        feature = "ms-ssim",
        feature = "ssimulacra2",
        feature = "butteraugli"
    )),
    allow(unused_imports, unused_macros)
)]

mod common;

use common::*;

/// Generates one `#[test]` per pixel format, each running the full property
/// suite for the given metric on that format.
macro_rules! property_matrix {
    ($metric:ty, $($fmt:ident),+ $(,)?) => {
        $(
            #[test]
            #[allow(non_snake_case)]
            fn $fmt() {
                run_property_suite::<$fmt, $metric>();
            }
        )+
    };
}

#[cfg(feature = "psnr")]
mod psnr_rgb_averaged {
    use super::*;
    property_matrix!(PsnrRgbAvg, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "psnr")]
mod psnr_luma709 {
    use super::*;
    property_matrix!(PsnrLuma, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "ssim")]
mod ssim_rgb_averaged {
    use super::*;
    property_matrix!(SsimRgbAvg, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "ssim")]
mod ssim_luma709 {
    use super::*;
    property_matrix!(SsimLuma, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "dssim")]
mod dssim_rgb_averaged {
    use super::*;
    property_matrix!(DssimRgbAvg, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "dssim")]
mod dssim_luma709 {
    use super::*;
    property_matrix!(DssimLuma, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "ms-ssim")]
mod ms_ssim_rgb_averaged {
    use super::*;
    property_matrix!(MsssimRgbAvg, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

#[cfg(feature = "ms-ssim")]
mod ms_ssim_luma709 {
    use super::*;
    property_matrix!(MsssimLuma, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

// Every line below compiles only because the named format implements
// `Ssimulacra2Input`; a non-sRGB format would be rejected here by the type
// checker, which is exactly the guarantee the metric's signature provides.
#[cfg(feature = "ssimulacra2")]
mod ssimulacra2 {
    use super::*;
    property_matrix!(Ssim2, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}

// Like the ssimulacra2 block, every line compiles only because the named format
// implements `ButteraugliInput`.
#[cfg(feature = "butteraugli")]
mod butteraugli {
    use super::*;
    property_matrix!(Butteraugli, Srgb8, Srgb16, Gray8, Gray16, Rgba8, Rgba16);
}
