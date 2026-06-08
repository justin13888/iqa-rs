# Vendored Butteraugli sources (from libjxl)

Butteraugli is part of Google's [libjxl]. The `ssimulacra2` submodule
(`cloudinary/ssimulacra2`) already vendors a pruned subset of libjxl under
`third_party/ssimulacra2/src/lib/jxl/`, and that subset contains almost every
file Butteraugli needs. The files here are the small remainder that the subset
omits — the convolution strategy translation units and the Butteraugli
comparator itself.

These are **hand-vendored** (copied directly, not a git submodule) rather than
pulling all of libjxl, to keep the published crate and a contributor checkout
small. They are compiled by `build.rs` (only when the `butteraugli` feature is
enabled) **against the headers in the ssimulacra2 libjxl subset**, so there is a
single, consistent libjxl header set and no duplicate translation units.

## Provenance

- Upstream: <https://github.com/libjxl/libjxl>
- Version: **v0.8.2** (these files are byte-identical across v0.8.0–v0.8.2;
  v0.8.2 chosen as the latest 0.8.x patch). This matches the libjxl 0.8 subset
  vendored by `cloudinary/ssimulacra2`, against whose headers they compile.
- Layout mirrors libjxl's `lib/` tree so each `#include "lib/jxl/..."` (including
  the Highway `HWY_TARGET_INCLUDE` re-includes) resolves unchanged.

## Files

Copied verbatim from `libjxl/lib/`:

```
lib/jxl/convolve-inl.h
lib/jxl/convolve_separable5.cc
lib/jxl/convolve_separable7.cc
lib/jxl/convolve_slow.cc
lib/jxl/convolve_symmetric3.cc
lib/jxl/convolve_symmetric5.cc
lib/jxl/butteraugli/butteraugli.{cc,h}
lib/jxl/enc_butteraugli_comparator.{cc,h}
lib/jxl/enc_butteraugli_pnorm.{cc,h}
lib/jxl/enc_comparator.{cc,h}
lib/jxl/enc_gamma_correct.h
```

`convolve.h` and every other header these sources include are provided by the
ssimulacra2 libjxl subset (and are not duplicated here).

## License

libjxl is BSD-3-Clause; see [`LICENSE`](./LICENSE). Compatible with this crate's
`MIT OR Apache-2.0`.

[libjxl]: https://github.com/libjxl/libjxl
