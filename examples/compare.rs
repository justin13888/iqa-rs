//! Compares two images with every metric `iqa` provides.
//!
//! Run it with two image files — PNG or JPEG, any size, color or grayscale:
//!
//! ```text
//! cargo run --release --example compare -- reference.png distorted.jpg
//! ```
//!
//! `iqa` deliberately does not decode image formats; that one job is left
//! to the caller. This example shows the whole pipeline: the `image` crate
//! decodes the files, and a few lines turn each decoded image into the
//! `iqa::Image` type the metrics consume.

use std::error::Error;
use std::ffi::OsString;
use std::path::Path;

use iqa::{Image, PsnrMode, PsnrOptions, Srgb8};

/// Decodes an image file and converts it into the `iqa` input type.
///
/// This is the one conversion step `iqa` leaves to the caller. The `image`
/// crate decodes the file; `to_rgb8()` normalizes whatever it found —
/// grayscale, RGBA, 16-bit, palette — into 8-bit RGB; and `into_raw()` yields
/// exactly the tightly packed, row-major sample buffer `Image::srgb8` expects.
///
/// The result is an `Image<Srgb8>`: its color space, channel layout, and bit
/// depth are now fixed in its type. (For a 16-bit pipeline the same shape
/// works with `image::DynamicImage::to_rgb16()` and `Image::srgb16`.)
fn load(path: &Path) -> Result<Image<Srgb8>, Box<dyn Error>> {
    let decoded = image::open(path)?.to_rgb8();
    let (width, height) = decoded.dimensions();
    Ok(Image::srgb8(width, height, decoded.into_raw())?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let (Some(reference_path), Some(distorted_path)) = (args.next(), args.next()) else {
        eprintln!("usage: compare <reference-image> <distorted-image>");
        eprintln!("       both may be any PNG or JPEG file");
        std::process::exit(2);
    };

    let reference = load(Path::new(&reference_path))?;
    let distorted = load(Path::new(&distorted_path))?;

    describe("reference", &reference_path, &reference);
    describe("distorted", &distorted_path, &distorted);
    println!();

    // Every metric requires its two inputs to share one pixel format. Both
    // images are `Image<Srgb8>`, so these calls type-check. Passing, say, an
    // `Image<Gray8>` as one argument would not compile at all — a mismatch
    // can never reach run time as a wrong score.
    //
    // Differing *dimensions* are the one mismatch left to run time, since an
    // image's size is not known to the compiler; the metric reports it as an
    // error, which `report` prints below.
    report(
        "PSNR (RGB-averaged)",
        iqa::psnr(&reference, &distorted, PsnrOptions::default()),
        "dB  (higher is better)",
    );
    report(
        "PSNR (Rec.709 luma)",
        iqa::psnr(
            &reference,
            &distorted,
            PsnrOptions {
                mode: PsnrMode::Luma709,
            },
        ),
        "dB  (higher is better)",
    );
    report(
        "SSIM (RGB-averaged)",
        iqa::ssim(&reference, &distorted, iqa::SsimOptions::default()),
        "    (1.0 = identical)",
    );
    report(
        "SSIMULACRA2",
        iqa::ssimulacra2(&reference, &distorted),
        "    (100 = identical)",
    );
    report(
        "Butteraugli",
        iqa::butteraugli(&reference, &distorted, iqa::ButteraugliOptions::default()),
        "    (0 = identical, lower is better)",
    );

    Ok(())
}

/// Prints a one-line summary of a loaded image.
fn describe(role: &str, path: &OsString, image: &Image<Srgb8>) {
    println!(
        "{role}: {}  ({}x{})",
        Path::new(path).display(),
        image.width(),
        image.height(),
    );
}

/// Prints one metric result, or the error explaining why it could not run.
fn report(name: &str, result: iqa::Result<f64>, unit: &str) {
    match result {
        Ok(score) => println!("  {name:<22} {score:>9.3}  {unit}"),
        Err(error) => println!("  {name:<22}     error: {error}"),
    }
}
