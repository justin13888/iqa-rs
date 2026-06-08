// C-ABI shim around libjxl's Butteraugli comparator.
//
// `ButteraugliDistance` takes `jxl::ImageBundle`s, not raw buffers, so this
// shim builds an sRGB-tagged bundle from the caller's packed RGB float data
// (the same approach as `ssimulacra2_shim.cc`). It fills a per-pixel diffmap
// and pools it into a scalar distance with the caller's `pnorm`, so the
// p-norm is configurable rather than fixed at Butteraugli's internal default.
//
// libjxl is compiled with `-fno-exceptions` and aborts on `JXL_CHECK`
// failures, so all input validation happens here, before the reference code
// is ever entered.

#include "butteraugli_shim.h"

#include <cmath>
#include <cstddef>
#include <utility>

#include "lib/jxl/color_encoding_internal.h"
#include "lib/jxl/enc_butteraugli_comparator.h"
#include "lib/jxl/enc_butteraugli_pnorm.h"
#include "lib/jxl/enc_color_management.h"
#include "lib/jxl/image.h"
#include "lib/jxl/image_bundle.h"
#include "lib/jxl/image_metadata.h"

namespace {

// Builds an sRGB `ImageBundle` by copying `rgb` (packed, row-major, 0..1) into
// the three color planes. `metadata` must outlive the returned bundle.
jxl::ImageBundle MakeBundle(const jxl::ImageMetadata* metadata,
                            const float* rgb, unsigned width,
                            unsigned height) {
  jxl::Image3F color(width, height);
  // Highway pads each row up to a SIMD-vector multiple; those trailing lanes
  // are uninitialized at allocation. Butteraugli's convolutions use unaligned
  // loads that touch them, so leaving them unset makes the score depend on
  // whatever garbage the allocation happened to contain -- nondeterministic
  // across builds, and only invisible when `width` is already vector-aligned
  // (e.g. 64). Replicate the last real pixel across the padding (edge clamp) so
  // every lane Butteraugli can read is defined and the boundary stays benign.
  const size_t row_pixels = static_cast<size_t>(color.PixelsPerRow());
  for (unsigned y = 0; y < height; ++y) {
    float* r = color.PlaneRow(0, y);
    float* g = color.PlaneRow(1, y);
    float* b = color.PlaneRow(2, y);
    const float* src = rgb + static_cast<size_t>(y) * width * 3;
    for (unsigned x = 0; x < width; ++x) {
      r[x] = src[x * 3 + 0];
      g[x] = src[x * 3 + 1];
      b[x] = src[x * 3 + 2];
    }
    for (size_t x = width; x < row_pixels; ++x) {
      r[x] = r[width - 1];
      g[x] = g[width - 1];
      b[x] = b[width - 1];
    }
  }
  jxl::ImageBundle bundle(metadata);
  bundle.SetFromImage(std::move(color), jxl::ColorEncoding::SRGB(false));
  return bundle;
}

}  // namespace

extern "C" BaResult iqa_butteraugli_rgb_f32(const float* orig_rgb,
                                            const float* dist_rgb,
                                            unsigned width, unsigned height,
                                            float intensity_target,
                                            float hf_asymmetry, double pnorm) {
  BaResult result = {0.0, 0};
  if (orig_rgb == nullptr || dist_rgb == nullptr) return result;
  // The reference algorithm requires at least 8x8; smaller inputs would hit a
  // JXL_CHECK abort inside libjxl.
  if (width < 8 || height < 8) return result;

  jxl::ImageMetadata metadata;
  jxl::ImageBundle orig = MakeBundle(&metadata, orig_rgb, width, height);
  jxl::ImageBundle dist = MakeBundle(&metadata, dist_rgb, width, height);

  jxl::ButteraugliParams params;
  params.hf_asymmetry = hf_asymmetry;
  params.intensity_target = intensity_target;

  // `ButteraugliDistance` converts to linear sRGB, runs the comparator, and
  // writes the per-pixel diffmap into `diffmap`. We discard its own (fixed)
  // pooled score and re-pool the diffmap with the caller's `pnorm`.
  jxl::ImageF diffmap;
  jxl::ButteraugliDistance(orig, dist, params, jxl::GetJxlCms(), &diffmap,
                           /*pool=*/nullptr);
  double score = jxl::ComputeDistanceP(diffmap, params, pnorm);
  if (std::isnan(score) || std::isinf(score)) return result;

  result.score = score;
  result.ok = 1;
  return result;
}
