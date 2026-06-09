//! Independent-reference tests for SSIM.
//!
//! `tests/ssim.rs` pins SSIM's identity score, its *uniform* closed form, and a
//! set of orderings. None of those exercise the windowed path on structured
//! content: with a uniform image every window has zero variance and covariance,
//! so the Gaussian shape, the variance/covariance accumulation, and the `C2`
//! constant all drop out (`C2` cancels as `c2 / c2`). A wrong window σ, a
//! sign-flipped covariance, or a perturbed `K2` therefore passes the rest of the
//! suite untouched.
//!
//! This file closes that gap with a **self-contained second implementation** of
//! mean SSIM, written independently of `src/ssim.rs` — it derives the Gaussian
//! window from `σ = 1.5` and takes `K1 = 0.01`, `K2 = 0.03`, and the 11×11 window
//! straight from the spec — and asserts `iqa::ssim` matches it to a tight
//! tolerance on structured fixtures, at a single-window size (11×11) and a
//! multi-window size, in both modes and at both bit depths. It is the
//! no-external-dependency oracle for the windowed path.
#![cfg(feature = "ssim")]

use iqa::{Gray8, Image, Srgb8, Srgb16, SsimMode, SsimOptions, ssim};

const RGB_AVERAGED: SsimOptions = SsimOptions {
    mode: SsimMode::RgbAveraged,
};
const LUMA709: SsimOptions = SsimOptions {
    mode: SsimMode::Luma709,
};

// --- The independent reference --------------------------------------------
//
// Spec constants, transcribed here so a mutated copy in `src/ssim.rs` diverges.
const WINDOW: usize = 11;
const SIGMA: f64 = 1.5;
const K1: f64 = 0.01;
const K2: f64 = 0.03;

/// Normalized 11×11 Gaussian window, derived from `σ = 1.5`.
fn gaussian() -> [[f64; WINDOW]; WINDOW] {
    let center = (WINDOW as f64 - 1.0) / 2.0;
    let mut w = [[0.0f64; WINDOW]; WINDOW];
    let mut sum = 0.0;
    for (j, row) in w.iter_mut().enumerate() {
        for (i, cell) in row.iter_mut().enumerate() {
            let dx = i as f64 - center;
            let dy = j as f64 - center;
            *cell = (-(dx * dx + dy * dy) / (2.0 * SIGMA * SIGMA)).exp();
            sum += *cell;
        }
    }
    for row in &mut w {
        for cell in row {
            *cell /= sum;
        }
    }
    w
}

/// Mean SSIM over every fully covered window of two single-channel planes,
/// computed from the textbook definition. `l` is the dynamic range.
fn mean_ssim(width: usize, height: usize, a: &[f64], b: &[f64], l: f64) -> f64 {
    let w = gaussian();
    let c1 = (K1 * l).powi(2);
    let c2 = (K2 * l).powi(2);
    let mut total = 0.0;
    let mut count = 0usize;
    for y0 in 0..=(height - WINDOW) {
        for x0 in 0..=(width - WINDOW) {
            let mut mu_a = 0.0;
            let mut mu_b = 0.0;
            for (j, row) in w.iter().enumerate() {
                for (i, &wij) in row.iter().enumerate() {
                    let idx = (y0 + j) * width + (x0 + i);
                    mu_a += wij * a[idx];
                    mu_b += wij * b[idx];
                }
            }
            let mut var_a = 0.0;
            let mut var_b = 0.0;
            let mut cov = 0.0;
            for (j, row) in w.iter().enumerate() {
                for (i, &wij) in row.iter().enumerate() {
                    let idx = (y0 + j) * width + (x0 + i);
                    let da = a[idx] - mu_a;
                    let db = b[idx] - mu_b;
                    var_a += wij * da * da;
                    var_b += wij * db * db;
                    cov += wij * da * db;
                }
            }
            let s = ((2.0 * mu_a * mu_b + c1) * (2.0 * cov + c2))
                / ((mu_a * mu_a + mu_b * mu_b + c1) * (var_a + var_b + c2));
            total += s;
            count += 1;
        }
    }
    total / count as f64
}

// --- Fixtures: structured content where every channel varies independently --
//
// All three channels vary in space *and* between reference and distorted, so the
// means, variances, and covariance are all non-trivial and distinct — the
// conditions under which the windowed path and the luma weighting actually
// matter.
fn ref_sample(x: u32, y: u32, c: usize) -> u8 {
    let v = match c {
        0 => 40 + (x * 7 + y * 3) % 120,
        1 => 30 + (x * 3 + y * 11) % 140,
        _ => 50 + (x * 13 + y * 5) % 100,
    };
    v as u8
}

fn dist_sample(x: u32, y: u32, c: usize) -> u8 {
    let perturb = match c {
        0 => ((x + 2 * y) % 5) as i32 * 6 - 12,
        1 => ((3 * x + y) % 7) as i32 * 4 - 12,
        _ => ((x + y) % 3) as i32 * 9 - 9,
    };
    (ref_sample(x, y, c) as i32 + perturb).clamp(0, 255) as u8
}

fn build_srgb8(width: u32, height: u32, f: impl Fn(u32, u32, usize) -> u8) -> Image<Srgb8> {
    let mut data = Vec::with_capacity((width * height) as usize * 3);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                data.push(f(x, y, c));
            }
        }
    }
    Image::srgb8(width, height, data).unwrap()
}

fn build_srgb16(width: u32, height: u32, f: impl Fn(u32, u32, usize) -> u8) -> Image<Srgb16> {
    let mut data = Vec::with_capacity((width * height) as usize * 3);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                // Mirror the 8-bit content scaled into the 16-bit range, so the
                // reference exercises `l = 65535` and the c1/c2 scaling.
                data.push(f(x, y, c) as u16 * 257);
            }
        }
    }
    Image::srgb16(width, height, data).unwrap()
}

fn build_gray8(width: u32, height: u32, f: impl Fn(u32, u32) -> u8) -> Image<Gray8> {
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            data.push(f(x, y));
        }
    }
    Image::gray8(width, height, data).unwrap()
}

/// One color channel of an sRGB-8 image as an `f64` plane.
fn channel_plane(img: &Image<Srgb8>, c: usize) -> Vec<f64> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let s = img.samples();
    let mut p = Vec::with_capacity(w * h);
    for px in 0..w * h {
        p.push(s[px * 3 + c] as f64);
    }
    p
}

/// Rec.709 luma plane of an sRGB-8 image.
fn luma_plane(img: &Image<Srgb8>) -> Vec<f64> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let s = img.samples();
    let mut p = Vec::with_capacity(w * h);
    for px in 0..w * h {
        let i = px * 3;
        p.push(0.2126 * s[i] as f64 + 0.7152 * s[i + 1] as f64 + 0.0722 * s[i + 2] as f64);
    }
    p
}

fn channel_plane16(img: &Image<Srgb16>, c: usize) -> Vec<f64> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let s = img.samples();
    let mut p = Vec::with_capacity(w * h);
    for px in 0..w * h {
        p.push(s[px * 3 + c] as f64);
    }
    p
}

fn gray_plane(img: &Image<Gray8>) -> Vec<f64> {
    img.samples().iter().map(|&v| v as f64).collect()
}

fn assert_close(got: f64, want: f64, context: &str) {
    assert!(
        (got - want).abs() <= 1e-9,
        "{context}: iqa::ssim = {got}, independent reference = {want} (delta {})",
        (got - want).abs(),
    );
}

// --- The tests -------------------------------------------------------------

#[test]
fn rgb_averaged_matches_independent_reference() {
    for (w, h) in [(11, 11), (16, 18)] {
        let reference = build_srgb8(w, h, ref_sample);
        let distorted = build_srgb8(w, h, dist_sample);
        let got = ssim(&reference, &distorted, RGB_AVERAGED).unwrap();
        let want = (0..3)
            .map(|c| {
                mean_ssim(
                    w as usize,
                    h as usize,
                    &channel_plane(&reference, c),
                    &channel_plane(&distorted, c),
                    255.0,
                )
            })
            .sum::<f64>()
            / 3.0;
        assert_close(got, want, &format!("rgb-averaged {w}x{h}"));
        // Sanity: the structured pair is a genuine, non-degenerate comparison.
        assert!(got < 1.0 && got > -1.0, "{w}x{h}: score {got} out of range");
    }
}

#[test]
fn luma_matches_independent_reference() {
    for (w, h) in [(11, 11), (16, 18)] {
        let reference = build_srgb8(w, h, ref_sample);
        let distorted = build_srgb8(w, h, dist_sample);
        let got = ssim(&reference, &distorted, LUMA709).unwrap();
        let want = mean_ssim(
            w as usize,
            h as usize,
            &luma_plane(&reference),
            &luma_plane(&distorted),
            255.0,
        );
        assert_close(got, want, &format!("luma709 {w}x{h}"));
    }
}

#[test]
fn grayscale_matches_independent_reference() {
    let (w, h) = (14, 14);
    let reference = build_gray8(w, h, |x, y| ref_sample(x, y, 0));
    let distorted = build_gray8(w, h, |x, y| dist_sample(x, y, 0));
    let got = ssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let want = mean_ssim(
        w as usize,
        h as usize,
        &gray_plane(&reference),
        &gray_plane(&distorted),
        255.0,
    );
    assert_close(got, want, "grayscale 14x14");
}

#[test]
fn sixteen_bit_matches_independent_reference() {
    let (w, h) = (13, 15);
    let reference = build_srgb16(w, h, ref_sample);
    let distorted = build_srgb16(w, h, dist_sample);
    let got = ssim(&reference, &distorted, RGB_AVERAGED).unwrap();
    let want = (0..3)
        .map(|c| {
            mean_ssim(
                w as usize,
                h as usize,
                &channel_plane16(&reference, c),
                &channel_plane16(&distorted, c),
                65535.0,
            )
        })
        .sum::<f64>()
        / 3.0;
    assert_close(got, want, "rgb-averaged 16-bit 13x15");
}
