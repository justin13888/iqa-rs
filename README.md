# iqa-rs

`iqa-rs` provides a single, ergonomic API over the patchwork of visual quality assessment metrics available in the Rust ecosystem. It wraps existing crates where they exist and fills in the gaps where they don't, so you can compute PSNR, SSIMULACRA2, and friends without juggling a different type, color space, and pixel format for each one.

## Metrics

The table below tracks the planned metric set — the nine-metric full-reference core that covers essentially every published JXL/AVIF codec comparison from the last two years, plus LPIPS as a learned reference metric.

The `Implementation` column points at the implementation `iqa-rs` is built on. We prefer porting or binding the upstream source implementation over reusing a Rust-specific reimplementation: cross-compilation is a requirement, but portability beyond that is not, so staying close to the reference keeps results faithful.

| IQA         | Implementation                                                                              | Status      |
| ----------- | ------------------------------------------------------------------------------------------- | ----------- |
| SSIMULACRA2 | [cloudinary/ssimulacra2](https://github.com/cloudinary/ssimulacra2)                         | Planned     |
| Butteraugli | [libjxl](https://github.com/libjxl/libjxl)                                                  | Planned     |
| DSSIM       | [kornelski/dssim](https://github.com/kornelski/dssim)                                       | Planned     |
| XPSNR       | [fraunhoferhhi/xpsnr](https://github.com/fraunhoferhhi/xpsnr)                               | Planned     |
| PSNR        | Native implementation                                                                       | Planned     |
| PSNR-HVS-M  | [xiph/daala — `tools/psnrhvs.c`](https://github.com/xiph/daala/blob/master/tools/psnrhvs.c) | Planned     |
| SSIM        | Native implementation                                                                       | Planned     |
| MS-SSIM     | [xiph/daala — `tools/ssim.c`](https://github.com/xiph/daala/blob/master/tools/ssim.c)       | Planned     |
| CIEDE2000   | Native implementation                                                                       | Planned     |
| LPIPS       | [richzhang/PerceptualSimilarity](https://github.com/richzhang/PerceptualSimilarity)         | Not planned* |

*: LPIPS is Python implementation only.

<!-- TODO: the links for xiph/daala are broken. I also suspect they don't have a good library implementation for them. -->

**Status legend:**

- **Planned** — selected for implementation, not yet started.
- **Require testing** — implemented, but not yet validated against reference outputs.
- **Stable and tested** — implemented and verified against the reference implementation.
