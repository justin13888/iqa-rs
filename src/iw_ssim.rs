//! Information content weighted structural similarity index (IW-SSIM).
//!
//! IW-SSIM is a native implementation of the metric introduced by Wang and Li
//! (*"Information Content Weighting for Perceptual Image Quality Assessment"*,
//! IEEE TIP 2011). It is a direct extension of [MS-SSIM](crate::ms_ssim): the
//! same per-scale structural-similarity machinery, but the uniform spatial
//! averaging is replaced by an *information-content* weighting derived from a
//! Gaussian Scale Mixture (GSM) model of the wavelet coefficients plus an HVS
//! internal-noise term.
//!
//! Concretely, both images are decomposed into a five-level **Laplacian
//! pyramid** (a sqrt(2)-scaled binomial-5 filter with reflect-about-the-edge
//! boundaries). At each scale the 11x11 Gaussian window (σ = 1.5) from [`ssim`]
//! slides over the *band* and yields a contrast-structure (`cs`) map; the
//! coarsest scale (the low-pass residual) additionally folds in the luminance
//! term to form a full-SSIM map. For every scale but the coarsest, a per-pixel
//! information-content weight is estimated from the reference/distorted band
//! coefficients, and the band's map is pooled as a weighted mean
//! `Σ(map · iw) / Σ(iw)` instead of a flat average. The coarsest scale is pooled
//! uniformly. The per-scale results are combined as a weighted product with the
//! MS-SSIM weights:
//!
//! ```text
//! IW-SSIM = ∏ pool[i]^w[i],  w = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333]
//! ```
//!
//! Scores lie in `(-1, 1]`, higher is better, and pixel-identical images score
//! exactly `1.0`. An alpha channel, if present, is ignored.
//!
//! # Scale count
//!
//! The canonical metric uses five scales, which the reference requires an image
//! of at least 176x176 for (the coarsest band must still admit the 11x11
//! window). To stay well-defined for ordinary images, this implementation uses
//! *up to* five scales — the most that keep every scale at least 11 pixels —
//! and renormalizes the weights of the scales it uses so they sum to one. A
//! single-scale evaluation reduces to plain [`ssim`]. At 176x176 and larger all
//! five scales run, matching the reference.
//!
//! [`ssim`]: crate::ssim

use crate::error::{Error, Result};
use crate::format::{PixelFormat, Sample};
use crate::image::Image;
use crate::ssim::{K1, K2, SsimMode, WINDOW, color_channels, gaussian_window, luma, ssim_cs_maps};

/// Maximum number of pyramid scales (the canonical IW-SSIM uses five).
const MAX_SCALES: usize = 5;

/// Per-scale weights from Wang, Simoncelli & Bovik (2003), finest scale first —
/// shared with MS-SSIM, as IW-SSIM only changes the *pooling*, not the weights.
const WEIGHTS: [f64; MAX_SCALES] = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333];

/// Side length of the spatial neighborhood used by the GSM model (`blk_size`).
const BLK: usize = 3;

/// HVS internal-noise variance for an 8-bit signal (`sigma_nsq`), the default
/// from Sheikh & Bovik (2006). Scaled by `(L / 255)²` for other bit depths so
/// the noise model tracks the coefficient magnitudes, which grow with `L`.
const SIGMA_NSQ_8BIT: f64 = 0.4;

/// Lower bound applied to each per-scale pooled factor before exponentiation, so
/// the weighted product stays finite even if a factor is non-positive (a
/// fractional power of a negative base is `NaN`). Mirrors MS-SSIM's floor.
const TERM_FLOOR: f64 = 1e-12;

/// Numerical tolerance used by the reference for clamping near-zero quantities.
const TOL: f64 = 1e-15;

/// Options controlling an IW-SSIM computation.
///
/// IW-SSIM reuses [`SsimMode`], so the color handling matches [`ssim`](crate::ssim)
/// exactly.
#[derive(Debug, Clone, Copy, Default)]
pub struct IwssimOptions {
    /// The signal IW-SSIM is computed over.
    pub mode: SsimMode,
}

/// Computes the IW-SSIM between `reference` and `distorted`.
///
/// Both images share the pixel format `F`, so a difference in color space,
/// channel layout, or bit depth is rejected by the compiler rather than at
/// run time. An alpha channel, if present, is ignored. The score ranges over
/// `(-1, 1]`; higher is better, and pixel-identical images score exactly `1.0`.
/// Each image must be at least 11x11 (the size of the Gaussian window); see the
/// [module docs](self) for how the number of pyramid scales adapts to the image
/// size.
///
/// Unlike SSIM and MS-SSIM, IW-SSIM is **not symmetric**: its weights come from
/// a model of the *reference*, so swapping the arguments can change the score.
///
/// # Errors
///
/// - [`Error::DimensionMismatch`] if the images differ in size.
/// - [`Error::ImageTooSmall`] if either dimension is below 11 pixels.
///
/// # Examples
///
/// ```
/// use iqa::{Image, IwssimOptions, iwssim};
///
/// let reference = Image::srgb8(32, 32, vec![128; 32 * 32 * 3])?;
/// let distorted = Image::srgb8(32, 32, vec![130; 32 * 32 * 3])?;
/// let score = iwssim(&reference, &distorted, IwssimOptions::default())?;
/// assert!(score <= 1.0);
/// # Ok::<(), iqa::Error>(())
/// ```
///
/// Comparing two different pixel formats does not type-check:
///
/// ```compile_fail
/// use iqa::{Image, IwssimOptions, iwssim};
///
/// let rgb = Image::srgb8(11, 11, vec![0; 11 * 11 * 3])?;
/// let gray = Image::gray8(11, 11, vec![0; 11 * 11])?;
/// let _ = iwssim(&rgb, &gray, IwssimOptions::default()); // channel mismatch
/// # Ok::<(), iqa::Error>(())
/// ```
pub fn iwssim<F: PixelFormat>(
    reference: &Image<F>,
    distorted: &Image<F>,
    opts: IwssimOptions,
) -> Result<f64> {
    if reference.dimensions() != distorted.dimensions() {
        return Err(Error::DimensionMismatch {
            a: reference.dimensions(),
            b: distorted.dimensions(),
        });
    }

    let (width, height) = reference.dimensions();
    if width < WINDOW as u32 || height < WINDOW as u32 {
        return Err(Error::ImageTooSmall(width, height, WINDOW as u32));
    }

    let l = <F::Sample as Sample>::MAX;
    let c1 = (K1 * l).powi(2);
    let c2 = (K2 * l).powi(2);
    let sigma_nsq = SIGMA_NSQ_8BIT * (l / 255.0).powi(2);

    let score = match opts.mode {
        SsimMode::RgbAveraged => {
            let channels = color_channels(F::CHANNELS);
            let total: f64 = channels
                .iter()
                .map(|&c| {
                    let r = channel_plane(reference, c);
                    let d = channel_plane(distorted, c);
                    iwssim_of_signal(&r, &d, c1, c2, sigma_nsq)
                })
                .sum();
            total / channels.len() as f64
        }
        SsimMode::Luma709 => {
            let r = luma_plane(reference);
            let d = luma_plane(distorted);
            iwssim_of_signal(&r, &d, c1, c2, sigma_nsq)
        }
    };

    Ok(score)
}

// ---------------------------------------------------------------------------
// A simple owned, row-major plane of `f64` samples.
// ---------------------------------------------------------------------------

/// A row-major plane of `f64` samples (`data[y * w + x]`).
struct Plane {
    w: usize,
    h: usize,
    data: Vec<f64>,
}

impl Plane {
    fn new(w: usize, h: usize) -> Self {
        Plane {
            w,
            h,
            data: vec![0.0; w * h],
        }
    }

    #[inline]
    fn at(&self, x: usize, y: usize) -> f64 {
        self.data[y * self.w + x]
    }
}

/// Extracts channel `c` of `img` into a [`Plane`].
fn channel_plane<F: PixelFormat>(img: &Image<F>, c: usize) -> Plane {
    let (width, height) = img.dimensions();
    let (w, h) = (width as usize, height as usize);
    let mut p = Plane::new(w, h);
    for y in 0..height {
        for x in 0..width {
            p.data[y as usize * w + x as usize] = img.sample_at(x, y, c);
        }
    }
    p
}

/// Extracts the Rec.709 luma of `img` into a [`Plane`].
fn luma_plane<F: PixelFormat>(img: &Image<F>) -> Plane {
    let (width, height) = img.dimensions();
    let (w, h) = (width as usize, height as usize);
    let mut p = Plane::new(w, h);
    for y in 0..height {
        for x in 0..width {
            p.data[y as usize * w + x as usize] = luma(img, x, y);
        }
    }
    p
}

// ---------------------------------------------------------------------------
// Core: IW-SSIM of a single signal plane.
// ---------------------------------------------------------------------------

/// IW-SSIM of one signal (a single channel, or luma), given as a [`Plane`] and
/// the SSIM stabilizing constants plus the (bit-depth-scaled) GSM noise term.
fn iwssim_of_signal(reference: &Plane, distorted: &Plane, c1: f64, c2: f64, sigma_nsq: f64) -> f64 {
    let nsc = num_scales(reference.w, reference.h);
    let weights = scale_weights(nsc);
    let window = gaussian_window();

    let pyr_ref = build_lpyr(reference, nsc);
    let pyr_dist = build_lpyr(distorted, nsc);

    let mut product = 1.0;
    for (s, &weight) in weights.iter().enumerate() {
        let band_r = &pyr_ref[s];
        let band_d = &pyr_dist[s];
        let (full, cs, map_w, map_h) = ssim_cs_maps(
            band_r.w as u32,
            band_r.h as u32,
            &window,
            c1,
            c2,
            |x, y| band_r.at(x as usize, y as usize),
            |x, y| band_d.at(x as usize, y as usize),
        );
        let (map_w, map_h) = (map_w as usize, map_h as usize);

        // The coarsest scale (the low-pass residual) contributes the full SSIM
        // (which folds in luminance) pooled uniformly; the finer (band-pass)
        // scales contribute the contrast-structure term pooled by information
        // content.
        let pooled = if s == nsc - 1 {
            let term = &full;
            term.iter().sum::<f64>() / (map_w * map_h) as f64
        } else {
            let term = &cs;
            let parent = parent_scale(s, nsc).map(|p| &pyr_ref[p]);
            let iw = info_content_weight_map(band_r, band_d, parent, sigma_nsq);
            pool_weighted(term, map_w, map_h, &iw)
        };

        product *= pooled.max(TERM_FLOOR).powf(weight);
    }
    product
}

/// Number of pyramid scales to use for a `w`×`h` signal: up to [`MAX_SCALES`],
/// stopping before any scale would fall below the 11-pixel window. Equivalent to
/// the reference's `min(size) >= win_size * 2^(Nsc-1)` requirement, but adaptive.
fn num_scales(w: usize, h: usize) -> usize {
    let min_dim = w.min(h);
    let mut nsc = 1;
    while nsc < MAX_SCALES && min_dim >= WINDOW * (1 << nsc) {
        nsc += 1;
    }
    nsc
}

/// The index of the parent (next-coarser) reference band to fold into band-pass
/// scale `s`'s GSM neighborhood, or `None` when no parent applies.
///
/// The reference includes the parent for every band-pass scale *except* the last
/// one (scale `nsc - 2`), whose parent would be the low-pass residual rather than
/// another band-pass band — matching `parent & (nband < Nsc-1)` in `iwssim.m`.
/// Returned as an index (not a borrow) so the gating *and* the `s + 1` offset are
/// pinned by a direct unit test, independent of the per-scale numerics.
fn parent_scale(s: usize, nsc: usize) -> Option<usize> {
    if s + 1 < nsc - 1 { Some(s + 1) } else { None }
}

/// The first `scales` of [`WEIGHTS`], renormalized to sum to one.
fn scale_weights(scales: usize) -> Vec<f64> {
    let used = &WEIGHTS[..scales];
    let sum: f64 = used.iter().sum();
    used.iter().map(|w| w / sum).collect()
}

/// Pools `term` (a row-major `map_w`×`map_h` map) by the information-content map
/// `iw`, after cropping `iw` to the same region: `Σ(term · iw) / Σ(iw)`.
///
/// `iw` is the `(band_h - 2)`×`(band_w - 2)` block grid; the `cs` map is the
/// `(band_h - 10)`×`(band_w - 10)` valid-window region. They are aligned by
/// trimming `bound1 = 4` rows/columns from each edge of `iw` (the reference's
/// `iw(bound1+1:end-bound1, ...)`), which lines its centers up with the window
/// centers. If the weights sum to zero (e.g. a flat band whose information
/// content is everywhere zero), this falls back to the uniform mean so the
/// result stays finite — the reference would divide by zero there.
fn pool_weighted(term: &[f64], map_w: usize, map_h: usize, iw: &Plane) -> f64 {
    // The iw block grid is wider/taller than the map by exactly `2 * bound1`.
    let bound1 = (iw.w - map_w) / 2;
    debug_assert_eq!(iw.w - map_w, iw.h - map_h);
    debug_assert_eq!(iw.w - 2 * bound1, map_w);

    let mut num = 0.0;
    let mut den = 0.0;
    for j in 0..map_h {
        for i in 0..map_w {
            let w = iw.at(i + bound1, j + bound1);
            num += term[j * map_w + i] * w;
            den += w;
        }
    }
    if den > 0.0 {
        num / den
    } else {
        term.iter().sum::<f64>() / (map_w * map_h) as f64
    }
}

// ---------------------------------------------------------------------------
// Laplacian pyramid (port of matlabPyrTools `buildLpyr`, default `binom5`
// filter and `reflect1` edges).
// ---------------------------------------------------------------------------

/// The sqrt(2)-scaled binomial-5 low-pass filter `namedFilter('binom5')`.
fn binom5() -> [f64; 5] {
    let s = 2.0_f64.sqrt();
    [0.0625 * s, 0.25 * s, 0.375 * s, 0.25 * s, 0.0625 * s]
}

/// Reflects index `i` into `[0, n)` about the edge *pixels* (whole-sample
/// symmetric), the `reflect1` rule matlabPyrTools subsamples with.
#[inline]
fn reflect1(i: i64, n: i64) -> usize {
    if n == 1 {
        return 0;
    }
    let period = 2 * (n - 1);
    let mut k = i % period;
    if k < 0 {
        k += period;
    }
    if k >= n {
        k = period - k;
    }
    k as usize
}

/// Builds a Laplacian pyramid of `nsc` levels, finest band first. Bands `0..nsc-1`
/// are the band-pass detail levels; band `nsc-1` is the low-pass residual.
fn build_lpyr(img: &Plane, nsc: usize) -> Vec<Plane> {
    let filt = binom5();
    let mut bands = Vec::with_capacity(nsc);
    let mut current = clone_plane(img);
    for _ in 0..nsc - 1 {
        // Reduce: low-pass and decimate by two (columns, then rows).
        let lo = down_w(&current, &filt);
        let lo2 = down_h(&lo, &filt);
        // Expand the residual back to the current size (rows, then columns) and
        // subtract to form the band-pass detail.
        let hi = up_h(&lo2, &filt, lo.h);
        let hi2 = up_w(&hi, &filt, current.w);
        let mut band = Plane::new(current.w, current.h);
        for k in 0..band.data.len() {
            band.data[k] = current.data[k] - hi2.data[k];
        }
        bands.push(band);
        current = lo2;
    }
    bands.push(current);
    bands
}

fn clone_plane(p: &Plane) -> Plane {
    Plane {
        w: p.w,
        h: p.h,
        data: p.data.clone(),
    }
}

/// Correlate with `filt` along the width and decimate by two (`corrDn` with a
/// row filter and step `[1 2]`). Output width is `ceil(w / 2)`.
fn down_w(p: &Plane, filt: &[f64; 5]) -> Plane {
    let nw = p.w.div_ceil(2);
    let mut out = Plane::new(nw, p.h);
    for y in 0..p.h {
        for k in 0..nw {
            let center = 2 * k;
            let mut sum = 0.0;
            for (t, &f) in filt.iter().enumerate() {
                let x = reflect1(center as i64 + t as i64 - 2, p.w as i64);
                sum += f * p.at(x, y);
            }
            out.data[y * nw + k] = sum;
        }
    }
    out
}

/// Correlate with `filt` along the height and decimate by two (`corrDn` with a
/// column filter and step `[2 1]`). Output height is `ceil(h / 2)`.
fn down_h(p: &Plane, filt: &[f64; 5]) -> Plane {
    let nh = p.h.div_ceil(2);
    let mut out = Plane::new(p.w, nh);
    for k in 0..nh {
        let center = 2 * k;
        for x in 0..p.w {
            let mut sum = 0.0;
            for (t, &f) in filt.iter().enumerate() {
                let y = reflect1(center as i64 + t as i64 - 2, p.h as i64);
                sum += f * p.at(x, y);
            }
            out.data[k * p.w + x] = sum;
        }
    }
    out
}

/// Upsample by two along the width to `target_w`, then convolve with `filt`
/// (`upConv` with a row filter and step `[1 2]`).
fn up_w(p: &Plane, filt: &[f64; 5], target_w: usize) -> Plane {
    // Zero-interleave the samples into a row of width `target_w`.
    let mut tmp = Plane::new(target_w, p.h);
    for y in 0..p.h {
        for k in 0..p.w {
            tmp.data[y * target_w + 2 * k] = p.at(k, y);
        }
    }
    let mut out = Plane::new(target_w, p.h);
    for y in 0..p.h {
        for x in 0..target_w {
            let mut sum = 0.0;
            for (t, &f) in filt.iter().enumerate() {
                let xx = reflect1(x as i64 + t as i64 - 2, target_w as i64);
                sum += f * tmp.at(xx, y);
            }
            out.data[y * target_w + x] = sum;
        }
    }
    out
}

/// Upsample by two along the height to `target_h`, then convolve with `filt`
/// (`upConv` with a column filter and step `[2 1]`).
fn up_h(p: &Plane, filt: &[f64; 5], target_h: usize) -> Plane {
    let mut tmp = Plane::new(p.w, target_h);
    for k in 0..p.h {
        for x in 0..p.w {
            tmp.data[2 * k * p.w + x] = p.at(x, k);
        }
    }
    let mut out = Plane::new(p.w, target_h);
    for y in 0..target_h {
        for x in 0..p.w {
            let mut sum = 0.0;
            for (t, &f) in filt.iter().enumerate() {
                let yy = reflect1(y as i64 + t as i64 - 2, target_h as i64);
                sum += f * tmp.at(x, yy);
            }
            out.data[y * p.w + x] = sum;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Information-content weight map (port of `info_content_weight_map.m`).
// ---------------------------------------------------------------------------

/// Information-content weight map for one band-pass scale, returned as the
/// `(band_h - 2)`×`(band_w - 2)` block grid.
///
/// `parent`, if present, is the next-coarser *reference* band; it is upsampled
/// (via [`imenlarge2`]) and appended to the spatial neighborhood, exactly as the
/// reference does when the parent is available.
fn info_content_weight_map(
    band_r: &Plane,
    band_d: &Plane,
    parent: Option<&Plane>,
    sigma_nsq: f64,
) -> Plane {
    let (bw, bh) = (band_r.w, band_r.h);

    // Local 3x3 gain (g) and estimation-error variance (vv) of the distorted
    // band given the reference band.
    let mean_x = box3_mean(band_r);
    let mean_y = box3_mean(band_d);
    let mut g = vec![0.0; bw * bh];
    let mut vv = vec![0.0; bw * bh];
    let prod_rd = mul_planes(band_r, band_d);
    let sq_r = mul_planes(band_r, band_r);
    let sq_d = mul_planes(band_d, band_d);
    let mean_xy = box3_mean(&prod_rd);
    let mean_xx = box3_mean(&sq_r);
    let mean_yy = box3_mean(&sq_d);
    for k in 0..bw * bh {
        let cov_xy = mean_xy.data[k] - mean_x.data[k] * mean_y.data[k];
        let mut ss_x = (mean_xx.data[k] - mean_x.data[k] * mean_x.data[k]).max(0.0);
        let ss_y = (mean_yy.data[k] - mean_y.data[k] * mean_y.data[k]).max(0.0);
        let mut gk = cov_xy / (ss_x + TOL);
        let mut vvk = ss_y - gk * cov_xy;
        if ss_x < TOL {
            gk = 0.0;
            vvk = ss_y;
            ss_x = 0.0;
        }
        if ss_y < TOL {
            gk = 0.0;
            vvk = 0.0;
        }
        g[k] = gk;
        vv[k] = vvk;
    }

    // Build the neighborhood matrix Y over every interior 3x3 block of the
    // reference band (plus the parent coefficient, if any). Column order is
    // irrelevant: the covariance and the quadratic form it feeds are invariant
    // to a consistent permutation of the neighbors.
    let nblv = bh - (BLK - 1); // number of block rows (band_h - 2)
    let nblh = bw - (BLK - 1); // number of block cols (band_w - 2)
    let nexp = nblv * nblh;
    let parent_up = parent.map(|p| {
        let e = imenlarge2(p);
        crop(&e, bw, bh)
    });
    let n = BLK * BLK + usize::from(parent_up.is_some());

    let mut ymat = vec![0.0; nexp * n];
    for a in 0..nblv {
        for b in 0..nblh {
            let row = a * nblh + b;
            let mut col = 0;
            for dy in 0..BLK {
                for dx in 0..BLK {
                    ymat[row * n + col] = band_r.at(b + dx, a + dy);
                    col += 1;
                }
            }
            if let Some(pu) = &parent_up {
                // Parent coefficient at the block center (a+1, b+1).
                ymat[row * n + col] = pu.at(b + 1, a + 1);
            }
        }
    }

    // C_u = Y' * Y / nexp, then eigen-correct (zero negative eigenvalues,
    // rescale to preserve total variance).
    let mut cu = vec![0.0; n * n];
    for row in 0..nexp {
        for i in 0..n {
            let yi = ymat[row * n + i];
            for j in i..n {
                cu[i * n + j] += yi * ymat[row * n + j];
            }
        }
    }
    for i in 0..n {
        for j in i..n {
            let v = cu[i * n + j] / nexp as f64;
            cu[i * n + j] = v;
            cu[j * n + i] = v;
        }
    }
    let (eig, evec) = jacobi_symmetric(&cu, n);
    let sum_all: f64 = eig.iter().sum();
    let sum_pos: f64 = eig.iter().map(|&l| l.max(0.0)).sum();
    let scale = sum_all / if sum_pos > 0.0 { sum_pos } else { 1.0 };
    let lam: Vec<f64> = eig.iter().map(|&l| l.max(0.0) * scale).collect();

    // Per-block signal strength ss = (y' inv(C_u) y) / N, using the eigen form
    // inv(C_u) = Σ_j v_j v_j' / λ_j.
    let inv_lam: Vec<f64> = lam
        .iter()
        .map(|&l| if l > 0.0 { 1.0 / l } else { 0.0 })
        .collect();

    let mut infow = Plane::new(nblh, nblv);
    let sigma_sq = sigma_nsq * sigma_nsq;
    for a in 0..nblv {
        for b in 0..nblh {
            let row = a * nblh + b;
            let yb = &ymat[row * n..row * n + n];
            // ss = (1/N) Σ_j (v_j · y)² / λ_j
            let mut ss = 0.0;
            for j in 0..n {
                let mut proj = 0.0;
                for k in 0..n {
                    proj += evec[k * n + j] * yb[k];
                }
                ss += proj * proj * inv_lam[j];
            }
            ss /= n as f64;

            // g, vv at the block center (a+1, b+1) of the band.
            let gc = g[(a + 1) * bw + (b + 1)];
            let vvc = vv[(a + 1) * bw + (b + 1)];

            let mut info = 0.0;
            for &lj in &lam {
                let numer = (vvc + (1.0 + gc * gc) * sigma_nsq) * ss * lj + sigma_nsq * vvc;
                info += (1.0 + numer / sigma_sq).log2();
            }
            if info < TOL {
                info = 0.0;
            }
            infow.data[a * nblh + b] = info;
        }
    }
    infow
}

/// Element-wise product of two equally sized planes.
fn mul_planes(a: &Plane, b: &Plane) -> Plane {
    let mut out = Plane::new(a.w, a.h);
    for k in 0..a.data.len() {
        out.data[k] = a.data[k] * b.data[k];
    }
    out
}

/// 3x3 box mean with zero padding outside the plane (`filter2(ones(3)/9, ., 'same')`).
fn box3_mean(p: &Plane) -> Plane {
    let mut out = Plane::new(p.w, p.h);
    for y in 0..p.h as i64 {
        for x in 0..p.w as i64 {
            let mut sum = 0.0;
            for dy in -1..=1i64 {
                for dx in -1..=1i64 {
                    let yy = y + dy;
                    let xx = x + dx;
                    if yy >= 0 && yy < p.h as i64 && xx >= 0 && xx < p.w as i64 {
                        sum += p.at(xx as usize, yy as usize);
                    }
                }
            }
            out.data[y as usize * p.w + x as usize] = sum / 9.0;
        }
    }
    out
}

/// Crops the top-left `w`×`h` corner of `p` (`p(1:h, 1:w)`).
fn crop(p: &Plane, w: usize, h: usize) -> Plane {
    let mut out = Plane::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.data[y * w + x] = p.at(x, y);
        }
    }
    out
}

/// Doubles the resolution of a band (`imenlarge2.m`): a bilinear resize to
/// `(4M-3)×(4N-3)`, a one-pixel linearly-extrapolated border, then decimation by
/// two — yielding a `2M`×`2N` plane.
fn imenlarge2(p: &Plane) -> Plane {
    let (n_in, m_in) = (p.w, p.h); // N = cols (width), M = rows (height)
    let rw = 4 * n_in - 3;
    let rh = 4 * m_in - 3;
    let t1 = imresize_bilinear(p, rw, rh);

    // Place t1 in the interior of a (4M-1)×(4N-1) plane and fill the 1-pixel
    // border by linear extrapolation (rows first, then columns, matching the
    // reference's overwrite order at the corners).
    let (tw, th) = (4 * n_in - 1, 4 * m_in - 1);
    let mut t2 = Plane::new(tw, th);
    for y in 0..rh {
        for x in 0..rw {
            t2.data[(y + 1) * tw + (x + 1)] = t1.at(x, y);
        }
    }
    for x in 0..tw {
        t2.data[x] = 2.0 * t2.at(x, 1) - t2.at(x, 2);
        t2.data[(th - 1) * tw + x] = 2.0 * t2.at(x, th - 2) - t2.at(x, th - 3);
    }
    for y in 0..th {
        t2.data[y * tw] = 2.0 * t2.at(1, y) - t2.at(2, y);
        t2.data[y * tw + (tw - 1)] = 2.0 * t2.at(tw - 2, y) - t2.at(tw - 3, y);
    }

    // Decimate by two: t2(1:2:end, 1:2:end) → 2M x 2N.
    let (ow, oh) = (2 * n_in, 2 * m_in);
    let mut out = Plane::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            out.data[y * ow + x] = t2.at(2 * x, 2 * y);
        }
    }
    out
}

/// Bilinear resize to `out_w`×`out_h` using the MATLAB/Octave `imresize`
/// coordinate convention `in = (out - 0.5) * (in_size / out_size) + 0.5`, with
/// edge clamping. Only used for enlargement, so no anti-aliasing applies.
fn imresize_bilinear(p: &Plane, out_w: usize, out_h: usize) -> Plane {
    let mut out = Plane::new(out_w, out_h);
    let sx = p.w as f64 / out_w as f64;
    let sy = p.h as f64 / out_h as f64;
    for oy in 0..out_h {
        let yin = ((oy as f64 + 0.5) * sy + 0.5).clamp(1.0, p.h as f64);
        let y0 = (yin.floor() as usize).min(p.h) - 1; // 0-based, in [0, h-1]
        let y1 = (y0 + 1).min(p.h - 1);
        let fy = yin - (y0 + 1) as f64;
        for ox in 0..out_w {
            let xin = ((ox as f64 + 0.5) * sx + 0.5).clamp(1.0, p.w as f64);
            let x0 = (xin.floor() as usize).min(p.w) - 1;
            let x1 = (x0 + 1).min(p.w - 1);
            let fx = xin - (x0 + 1) as f64;
            let v = (1.0 - fy) * (1.0 - fx) * p.at(x0, y0)
                + (1.0 - fy) * fx * p.at(x1, y0)
                + fy * (1.0 - fx) * p.at(x0, y1)
                + fy * fx * p.at(x1, y1);
            out.data[oy * out_w + ox] = v;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Symmetric eigendecomposition (cyclic Jacobi), for the small (≤10x10) GSM
// covariance matrices.
// ---------------------------------------------------------------------------

/// Eigendecomposition of a symmetric `n`×`n` matrix `a` (row-major), returning
/// `(eigenvalues, eigenvectors)` where eigenvector `j` is column `j` of the
/// returned row-major `n`×`n` matrix. Uses cyclic Jacobi rotations, which are
/// deterministic and accurate for the small matrices here.
fn jacobi_symmetric(a: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = a.to_vec();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    for _ in 0..100 {
        let mut off = 0.0;
        for p in 0..n {
            for q in p + 1..n {
                off += a[p * n + q] * a[p * n + q];
            }
        }
        if off <= 1e-30 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq.abs() <= 1e-300 {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let t = if theta == 0.0 {
                    1.0
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let akp = a[k * n + p];
                    let akq = a[k * n + q];
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let apk = a[p * n + k];
                    let aqk = a[q * n + k];
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let vkp = v[k * n + p];
                    let vkq = v[k * n + q];
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let eig = (0..n).map(|i| a[i * n + i]).collect();
    (eig, v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Srgb8;
    use crate::image::Image;
    use crate::ssim::{SsimOptions, ssim};

    fn solid_srgb8(width: u32, height: u32, value: u8) -> Image<Srgb8> {
        Image::srgb8(width, height, vec![value; (width * height * 3) as usize]).unwrap()
    }

    #[test]
    fn identical_images_are_one() {
        let img = solid_srgb8(64, 64, 100);
        let score = iwssim(&img, &img, IwssimOptions::default()).unwrap();
        assert!((score - 1.0).abs() < 1e-12, "expected 1.0, got {score}");
    }

    #[test]
    fn identical_textured_image_is_one() {
        // A non-flat image exercises the weighted-pooling path (non-zero info
        // weights) while still being identical, so it must score exactly 1.0.
        let mut data = vec![0u8; 64 * 64 * 3];
        for (i, px) in data.chunks_mut(3).enumerate() {
            let x = (i % 64) as f64;
            let y = (i / 64) as f64;
            let v = (128.0 + 60.0 * (x * 0.3).sin() * (y * 0.2).cos()).round() as u8;
            px[0] = v;
            px[1] = v;
            px[2] = v;
        }
        let img = Image::srgb8(64, 64, data).unwrap();
        let score = iwssim(&img, &img, IwssimOptions::default()).unwrap();
        assert!((score - 1.0).abs() < 1e-12, "expected 1.0, got {score}");
    }

    #[test]
    fn num_scales_matches_reference_thresholds() {
        // Five scales need the coarsest band to still admit the 11x11 window:
        // 176 = 11 * 2^4 reaches it, 175 does not.
        assert_eq!(num_scales(176, 176), 5);
        assert_eq!(num_scales(175, 175), 4);
        assert_eq!(num_scales(88, 88), 4);
        assert_eq!(num_scales(11, 11), 1);
        // However large the image, the count is capped at MAX_SCALES so the
        // renormalized weight slice never runs past WEIGHTS.
        assert_eq!(num_scales(4096, 4096), MAX_SCALES);
    }

    #[test]
    fn single_scale_matches_plain_ssim() {
        // Too small to build a pyramid: IW-SSIM collapses to one full-SSIM scale
        // on the original image, which must equal `ssim` exactly.
        let reference = solid_srgb8(16, 16, 100);
        let distorted = solid_srgb8(16, 16, 120);
        assert_eq!(num_scales(16, 16), 1);
        let iw = iwssim(&reference, &distorted, IwssimOptions::default()).unwrap();
        let plain = ssim(&reference, &distorted, SsimOptions::default()).unwrap();
        assert!((iw - plain).abs() < 1e-12, "iw={iw}, plain={plain}");
    }

    #[test]
    fn distortion_lowers_the_score() {
        let reference = solid_srgb8(192, 192, 128);
        let mut data = vec![128u8; 192 * 192 * 3];
        for (i, sample) in data.iter_mut().enumerate() {
            if i % 5 == 0 {
                *sample = 170;
            }
        }
        let distorted = Image::srgb8(192, 192, data).unwrap();
        let score = iwssim(&reference, &distorted, IwssimOptions::default()).unwrap();
        assert!(score < 1.0 && score.is_finite(), "distorted scored {score}");
    }

    #[test]
    fn constant_image_is_finite() {
        // A flat band has zero information content everywhere; the weighted pool
        // must fall back to the uniform mean rather than divide by zero.
        let reference = solid_srgb8(192, 192, 128);
        let distorted = solid_srgb8(192, 192, 130);
        let score = iwssim(&reference, &distorted, IwssimOptions::default()).unwrap();
        assert!(score.is_finite() && score > 0.0, "got {score}");
    }

    #[test]
    fn image_below_window_is_rejected() {
        let img = Image::srgb8(10, 10, vec![0; 10 * 10 * 3]).unwrap();
        let err = iwssim(&img, &img, IwssimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::ImageTooSmall(10, 10, 11)));
    }

    #[test]
    fn dimension_mismatch_is_an_error() {
        let a = Image::srgb8(16, 16, vec![0; 16 * 16 * 3]).unwrap();
        let b = Image::srgb8(16, 12, vec![0; 16 * 12 * 3]).unwrap();
        let err = iwssim(&a, &b, IwssimOptions::default()).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }));
    }

    #[test]
    fn parent_scale_targets_all_but_the_last_band_pass_scale() {
        // Pins both the gating *and* the `s + 1` offset that select the parent
        // band: every band-pass scale takes the next-coarser band, except the
        // last band-pass scale (`nsc - 2`), whose parent would be the residual.
        // Five scales: band-pass 0,1,2 take parents 1,2,3; scale 3 takes none.
        assert_eq!(parent_scale(0, 5), Some(1));
        assert_eq!(parent_scale(1, 5), Some(2));
        assert_eq!(parent_scale(2, 5), Some(3));
        assert_eq!(parent_scale(3, 5), None);
        // Three scales: scale 0 takes parent 1; scale 1 (last band-pass) none.
        assert_eq!(parent_scale(0, 3), Some(1));
        assert_eq!(parent_scale(1, 3), None);
        // Two scales: the single band-pass scale's parent is the residual: none.
        assert_eq!(parent_scale(0, 2), None);
    }

    #[test]
    fn pool_weighted_is_the_information_weighted_mean() {
        // `iw` is the (band-2) block grid; the cs `term` is the (band-10) valid
        // region, so `pool_weighted` reads `iw` offset by `bound1 = (w-map_w)/2`.
        // Here map = 2x2 and iw = 4x4, so bound1 = 1: only the inner 2x2 of `iw`
        // is read, and the border must be ignored. Expected value is the
        // information-weighted mean computed by hand, with a different index
        // expression than the function uses, so an index slip is caught.
        let term = [10.0, 20.0, 30.0, 40.0]; // map 2x2, row-major
        let mut iw = Plane::new(4, 4);
        iw.data = vec![999.0; 16]; // sentinel: the border must NOT be read
        let weights = [[2.0, 5.0], [3.0, 7.0]]; // [j][i] at the inner 2x2
        for (j, row) in weights.iter().enumerate() {
            for (i, &wgt) in row.iter().enumerate() {
                iw.data[(j + 1) * 4 + (i + 1)] = wgt;
            }
        }
        let got = pool_weighted(&term, 2, 2, &iw);
        let num = 10.0 * 2.0 + 20.0 * 5.0 + 30.0 * 3.0 + 40.0 * 7.0;
        let den = 2.0 + 5.0 + 3.0 + 7.0;
        assert!(
            (got - num / den).abs() < 1e-12,
            "weighted mean: got {got}, want {}",
            num / den
        );
    }

    #[test]
    fn pool_weighted_falls_back_to_uniform_mean_when_weights_vanish() {
        // A flat band carries zero information everywhere; rather than divide by
        // zero (as the reference would), the pool falls back to the plain mean.
        let term = [2.0, 4.0, 6.0, 8.0];
        let mut iw = Plane::new(2, 2);
        iw.data = vec![0.0; 4];
        let got = pool_weighted(&term, 2, 2, &iw);
        assert!(
            (got - 5.0).abs() < 1e-12,
            "uniform-mean fallback: got {got}"
        );
    }

    /// A deterministic synthetic band `a·sin(b·x+0.3y)·cos(c·y−0.2x) +
    /// d·sin(0.17·(x²−y))`, matching `synth_band` in `scripts/gen-iwssim-goldens.py`.
    fn synth_band(w: usize, h: usize, a: f64, b: f64, c: f64, d: f64) -> Plane {
        let mut p = Plane::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let (xf, yf) = (x as f64, y as f64);
                p.data[y * w + x] = a * (b * xf + 0.3 * yf).sin() * (c * yf - 0.2 * xf).cos()
                    + d * (0.17 * (xf * xf - yf)).sin();
            }
        }
        p
    }

    #[test]
    fn info_content_weight_map_matches_independent_reference() {
        // The information-content weighting is the defining piece of IW-SSIM, yet
        // it only *reweights* an already near-uniform cs map, so its effect on the
        // final score is tiny (and globally scale-invariant) — an end-to-end
        // golden cannot pin it. This pins the map directly against the independent
        // NumPy/`pyrtools` reference (`scripts/gen-iwssim-goldens.py`, the
        // `info_content_weight_map` goldens), on a fixed, well-conditioned band
        // pair, with and without a parent band. Regenerate both together.
        //
        // The reference's `sigma_nsq` is the 8-bit default 0.4. The bands are
        // chosen so the GSM covariance has a cleanly positive smallest eigenvalue,
        // so an independent eigensolver agrees to far better than this tolerance.
        const IW_NO_PARENT: [f64; 100] = [
            54.098629926,
            58.087524623,
            68.171476426,
            65.283204892,
            64.143776494,
            71.546571069,
            71.411930712,
            68.083370322,
            72.756043031,
            78.513654621,
            57.174579933,
            61.257557847,
            64.439739637,
            59.168473961,
            59.862615098,
            71.724290599,
            73.191762705,
            73.638696317,
            75.880665985,
            73.967382674,
            57.469642383,
            57.359386310,
            59.545573301,
            63.555194173,
            65.843954054,
            73.190179625,
            75.293830801,
            73.959133024,
            73.601305638,
            65.784918125,
            55.832604981,
            58.103171200,
            59.588215385,
            68.297747296,
            70.576089331,
            73.932219385,
            75.132018725,
            69.960839486,
            64.777326824,
            61.832483198,
            62.346206777,
            66.985233052,
            65.731567970,
            69.684158068,
            70.633130329,
            71.950047359,
            71.177798519,
            64.572715778,
            65.497226848,
            63.980877102,
            69.924314761,
            71.695749367,
            71.508307317,
            66.718600494,
            63.596291390,
            64.061702005,
            66.870249837,
            66.551130843,
            67.795856569,
            63.148662953,
            70.607579283,
            65.035355578,
            63.662987063,
            59.104339836,
            61.694398446,
            68.628587878,
            66.394221162,
            64.670732058,
            67.167452258,
            62.489849361,
            64.694108671,
            60.799730512,
            64.739486544,
            65.385646033,
            72.332799649,
            73.734218027,
            62.637300198,
            60.118316409,
            67.192493135,
            62.404331869,
            67.522116130,
            69.113202100,
            69.982113646,
            68.103860586,
            74.721555746,
            71.709916788,
            59.554354315,
            58.831233250,
            65.507024896,
            63.426348805,
            69.563827192,
            72.759254693,
            72.660996431,
            66.578778364,
            68.202246897,
            62.256642973,
            61.174819463,
            63.368786562,
            69.260731471,
            69.380076624,
        ];
        const IW_WITH_PARENT: [f64; 100] = [
            65.179199793,
            70.329210667,
            82.688025465,
            79.487219428,
            76.313247826,
            81.657009345,
            81.778795360,
            80.478592785,
            87.538584562,
            89.230832416,
            66.857627878,
            72.244009882,
            76.635073217,
            70.887828863,
            70.428723724,
            81.516630268,
            83.239553703,
            85.330778476,
            89.696170957,
            84.122339818,
            65.945218577,
            66.488472157,
            69.463268389,
            74.160487662,
            76.260412628,
            83.061614632,
            85.481696859,
            85.213472669,
            86.755690793,
            75.021267134,
            63.857529276,
            66.439387252,
            68.338411642,
            78.292848877,
            80.982114189,
            83.867888666,
            85.301816527,
            80.659117853,
            77.059190463,
            70.559359622,
            71.069940872,
            76.083750435,
            74.836868241,
            79.493503755,
            81.086766484,
            81.730099126,
            80.881946544,
            75.154192405,
            78.407582022,
            73.007192936,
            79.464534731,
            81.350300718,
            81.239403567,
            76.181347362,
            73.610338283,
            73.284238912,
            76.130056899,
            77.820189131,
            81.252795601,
            72.446224431,
            80.497900270,
            74.348414719,
            72.730352673,
            67.842536085,
            71.768428328,
            78.640003891,
            75.628410947,
            75.831414033,
            80.109386726,
            71.957142878,
            74.769515932,
            70.434473932,
            74.013687741,
            74.626875560,
            83.183569357,
            84.254915467,
            71.421127207,
            69.995238904,
            78.621932881,
            71.604942884,
            78.513501526,
            79.860556866,
            79.603453193,
            77.399320920,
            84.947271833,
            81.552636532,
            68.201186230,
            67.505174983,
            75.299058248,
            72.489833313,
            81.068139813,
            83.637318609,
            82.412038769,
            76.272855985,
            77.712818434,
            71.141305870,
            71.139665109,
            72.337415404,
            78.906445693,
            79.108117206,
        ];

        let band_r = synth_band(12, 12, 30.0, 0.40, 0.30, 9.0);
        let delta = synth_band(12, 12, 5.0, 0.70, 0.50, 2.0);
        let mut band_d = Plane::new(12, 12);
        for k in 0..band_d.data.len() {
            band_d.data[k] = band_r.data[k] + 0.6 * delta.data[k];
        }
        let parent = synth_band(6, 6, 22.0, 0.55, 0.25, 6.0);
        let sigma_nsq = 0.4;

        let mut worst = 0.0_f64;
        for (golden, parent) in [
            (&IW_NO_PARENT[..], None),
            (&IW_WITH_PARENT[..], Some(&parent)),
        ] {
            let iw = info_content_weight_map(&band_r, &band_d, parent, sigma_nsq);
            assert_eq!(iw.data.len(), golden.len(), "map size");
            for (k, (&got, &want)) in iw.data.iter().zip(golden).enumerate() {
                worst = worst.max((got - want).abs());
                assert!(
                    (got - want).abs() <= 1e-4,
                    "info weight [{k}]: got {got}, want {want} (parent={})",
                    parent.is_some(),
                );
            }
        }
        eprintln!("info_content_weight_map worst |Δ| vs reference = {worst:e}");
    }
}
