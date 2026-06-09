// C-ABI shim around the cloudinary/ssimulacra2 C++ reference.
//
// `ComputeSSIMULACRA2` takes `jxl::ImageBundle`s, not raw buffers, so this
// shim builds an sRGB-tagged bundle from the caller's packed RGB float data.
// libjxl is compiled with `-fno-exceptions` and aborts on `JXL_CHECK`
// failures, so all input validation happens here, before the reference code
// is ever entered.

#include "ssimulacra2_shim.h"

#include <cmath>
#include <cstddef>
#include <utility>

#include "lib/jxl/color_encoding_internal.h"
#include "lib/jxl/image.h"
#include "lib/jxl/image_bundle.h"
#include "lib/jxl/image_metadata.h"
#include "ssimulacra2.h"

namespace {

// Builds an sRGB `ImageBundle` by copying `rgb` (packed, row-major, 0..1) into
// the three color planes. `metadata` must outlive the returned bundle.
jxl::ImageBundle MakeBundle(const jxl::ImageMetadata* metadata,
                            const float* rgb, unsigned width,
                            unsigned height) {
  jxl::Image3F color(width, height);
  // Highway pads each row up to a SIMD-vector multiple; those trailing lanes are
  // uninitialized at allocation. In the sibling butteraugli_shim.cc this was an
  // observable bug (its convolutions read past the row, making the score depend
  // on heap garbage; fixed in commit 9828f69). SSIMULACRA2 is different: every
  // read of this image clamps to `xsize()-1` (see `Downsample` in
  // ssimulacra2.cc), so the padding is *not* read and the score is unaffected by
  // its contents -- verified by poisoning the padding and observing no change.
  // We still replicate the last real pixel across it (edge clamp), for parity
  // with the sibling shim and as defense against a future libjxl/algorithm
  // change that does touch it; for aligned widths the fill is a no-op.
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

extern "C" S2Result iqa_ssimulacra2_rgb_f32(const float* orig_rgb,
                                            const float* dist_rgb,
                                            unsigned width, unsigned height) {
  S2Result result = {0.0, 0};
  if (orig_rgb == nullptr || dist_rgb == nullptr) return result;
  // The reference algorithm requires at least 8x8; smaller inputs would hit a
  // JXL_CHECK abort inside libjxl.
  if (width < 8 || height < 8) return result;

  jxl::ImageMetadata metadata;
  jxl::ImageBundle orig = MakeBundle(&metadata, orig_rgb, width, height);
  jxl::ImageBundle dist = MakeBundle(&metadata, dist_rgb, width, height);

  Msssim msssim = ComputeSSIMULACRA2(orig, dist);
  double score = msssim.Score();
  if (std::isnan(score) || std::isinf(score)) return result;

  result.score = score;
  result.ok = 1;
  return result;
}
