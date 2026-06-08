/* C-ABI shim around libjxl's Butteraugli comparator.
 *
 * This header is deliberately pure C so `bindgen` never has to parse C++. It
 * is iqa-rs's own code, kept beside the vendored sources rather than inside
 * them. The matching implementation is `butteraugli_shim.cc`. */

#ifndef IQA_BUTTERAUGLI_SHIM_H_
#define IQA_BUTTERAUGLI_SHIM_H_

#ifdef __cplusplus
extern "C" {
#endif

/* Outcome of a Butteraugli computation. `ok` is non-zero on success, in which
 * case `score` holds the distance (0.0 = identical, higher = more different);
 * on failure `ok` is 0 and `score` is unset. */
typedef struct BaResult {
  double score;
  int ok;
} BaResult;

/* Computes the Butteraugli distance between two images.
 *
 * `orig_rgb` and `dist_rgb` are tightly packed, row-major RGB buffers of
 * `width * height * 3` floats, each sample in 0.0..=1.0 and sRGB-encoded.
 * `intensity_target` is the display luminance in nits that 1.0 maps to
 * (libjxl default 80.0); `hf_asymmetry` weights new high-frequency artifacts
 * over blurred-away features (libjxl default 1.0); `pnorm` is the exponent used
 * to pool the per-pixel diffmap into a scalar (Butteraugli's canonical p is
 * 3.0). Both images must be at least 8x8. Returns `ok = 0` on invalid input. */
BaResult iqa_butteraugli_rgb_f32(const float* orig_rgb, const float* dist_rgb,
                                 unsigned width, unsigned height,
                                 float intensity_target, float hf_asymmetry,
                                 double pnorm);

#ifdef __cplusplus
}
#endif

#endif /* IQA_BUTTERAUGLI_SHIM_H_ */
