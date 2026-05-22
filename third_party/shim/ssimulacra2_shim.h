/* C-ABI shim around the cloudinary/ssimulacra2 C++ reference.
 *
 * This header is deliberately pure C so `bindgen` never has to parse C++. It
 * is iqa-rs's own code, kept beside the vendored submodule rather than inside
 * it. The matching implementation is `ssimulacra2_shim.cc`. */

#ifndef IQA_SSIMULACRA2_SHIM_H_
#define IQA_SSIMULACRA2_SHIM_H_

#ifdef __cplusplus
extern "C" {
#endif

/* Outcome of a SSIMULACRA2 computation. `ok` is non-zero on success, in which
 * case `score` holds the result; on failure `ok` is 0 and `score` is unset. */
typedef struct S2Result {
  double score;
  int ok;
} S2Result;

/* Computes the SSIMULACRA2 score between two images.
 *
 * `orig_rgb` and `dist_rgb` are tightly packed, row-major RGB buffers of
 * `width * height * 3` floats, each sample in 0.0..=1.0 and sRGB-encoded.
 * Both images must be at least 8x8. Returns `ok = 0` on invalid input. */
S2Result iqa_ssimulacra2_rgb_f32(const float* orig_rgb, const float* dist_rgb,
                                 unsigned width, unsigned height);

#ifdef __cplusplus
}
#endif

#endif /* IQA_SSIMULACRA2_SHIM_H_ */
