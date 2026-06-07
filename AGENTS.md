# iqa

`iqa` provides a single, ergonomic API over the patchwork of visual quality assessment metrics available in the Rust ecosystem. It wraps existing crates where they exist and fills in the gaps where they don't, so you can compute PSNR, SSIMULACRA2, and friends without juggling a different type, color space, and pixel format for each one.

## Implementation Expectations

- Prefer original implementations even if it is not native Rust
- Every IQA is feature-gated
- Non-rust libraries and dependencies are vendored as a Git submodule in `third_party/`
