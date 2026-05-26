# iqa-rs

`iqa-rs` provides a single, ergonomic API over the patchwork of visual quality assessment metrics available in the Rust ecosystem. It wraps existing crates where they exist and fills in the gaps where they don't, so you can compute PSNR, SSIMULACRA2, and friends without juggling a different type, color space, and pixel format for each one.

## Metrics

The table below tracks the planned metric set — the nine-metric full-reference core that covers essentially every published JXL/AVIF codec comparison from the last two years, plus LPIPS as a learned reference metric.

The `Implementation` column points at the implementation `iqa-rs` is built on. We prefer porting or binding the upstream source implementation over reusing a Rust-specific reimplementation: cross-compilation is a requirement, but portability beyond that is not, so staying close to the reference keeps results faithful.

| IQA         | Implementation                                                                              | Status          |
| ----------- | ------------------------------------------------------------------------------------------- | --------------- |
| SSIMULACRA2 | [cloudinary/ssimulacra2](https://github.com/cloudinary/ssimulacra2)                         | Require testing |
| Butteraugli | [libjxl](https://github.com/libjxl/libjxl)                                                  | Planned         |
| DSSIM       | [kornelski/dssim](https://github.com/kornelski/dssim)                                       | Planned         |
| XPSNR       | [fraunhoferhhi/xpsnr](https://github.com/fraunhoferhhi/xpsnr)                               | Planned         |
| PSNR        | Native implementation                                                                       | Require testing |
| PSNR-HVS-M  | [xiph/daala — `tools/psnrhvs.c`](https://github.com/xiph/daala/blob/master/tools/psnrhvs.c) | Planned         |
| SSIM        | Native implementation                                                                       | Planned         |
| MS-SSIM     | [xiph/daala — `tools/ssim.c`](https://github.com/xiph/daala/blob/master/tools/ssim.c)       | Planned         |
| CIEDE2000   | Native implementation                                                                       | Planned         |
| LPIPS       | [richzhang/PerceptualSimilarity](https://github.com/richzhang/PerceptualSimilarity)         | Not planned*    |

*: LPIPS is Python implementation only.

<!-- TODO: the links for xiph/daala are broken. I also suspect they don't have a good library implementation for them. -->

**Status legend:**

- **Planned** — selected for implementation, not yet started.
- **Require testing** — implemented, but not yet validated against reference outputs.
- **Stable and tested** — implemented and verified against the reference implementation.

## Cargo features

Each metric is gated behind its own Cargo feature:

| Feature       | Default | Notes                                                               |
| ------------- | ------- | ------------------------------------------------------------------- |
| `psnr`        | yes     | Native Rust; no system dependencies.                                |
| `ssimulacra2` | yes     | Binds the vendored C++ reference; see the build requirements below. |

**Every metric is enabled by default for convenience** — `cargo add iqa-rs`
gets you the full set. Some metrics (such as `ssimulacra2`) bind native C/C++
code and therefore need a C++ toolchain and system libraries to build.

For leaner builds, disable the default features and opt into exactly the
metrics you need:

```toml
[dependencies]
# Pure-Rust subset only — no C/C++ toolchain required.
iqa-rs = { version = "0.1", default-features = false, features = ["psnr"] }
```

### Building with `ssimulacra2`

SSIMULACRA2 is bound via FFI to the original C++ reference rather than
reimplemented, so enabling it (including via the default feature set) has extra
requirements:

1. **Submodules.** The C++ sources are vendored under `third_party/`:

   ```sh
   git submodule update --init --recursive
   ```

2. **A C++ toolchain.** `build.rs` compiles the reference with the `cc` crate,
   which uses `c++` by default (override with the `CXX` environment variable).

3. **lcms2.** libjxl's color management needs the system `lcms2` library,
   located via `pkg-config`:

   ```sh
   sudo apt install liblcms2-dev   # Debian / Ubuntu
   brew install little-cms2        # macOS / Homebrew
   ```

Then build or test with the feature enabled:

```sh
cargo test --features ssimulacra2
```

## Development

It is important that development velocity is maintained regardless of project complexity. Unit tests for all contributions are expected, especially for platform-specific behaviours!

### Setup

Install [lefthook](https://github.com/evilmartians/lefthook) and activate the pre-commit and pre-push hooks:

```sh
# macOS / Homebrew
brew install lefthook

# Linux (Homebrew on Linux)
brew install lefthook

# via cargo
cargo install lefthook

# then install the hooks
lefthook install
```

The pre-commit hook runs `cargo fmt --check` and `cargo clippy`.
The pre-push hook runs the full test suite.

### Releasing

Releases are automated with [release-plz](https://release-plz.dev) and driven by
[Conventional Commits](https://www.conventionalcommits.org): the commit messages
landed on `master` decide the next version number and fill in `CHANGELOG.md`.

To cut a release:

1. Land changes on `master` with Conventional Commit messages (`feat:`, `fix:`,
   `refactor!:`, ...).
2. release-plz maintains an open **release PR** that bumps the version in
   `Cargo.toml`, updates `CHANGELOG.md`, and refreshes `Cargo.lock`. Review it as
   you would any other PR.
3. **Merge the release PR.** Merging it is the whole release: release-plz
   publishes the crate to crates.io, pushes a `vX.Y.Z` git tag, and creates the
   matching GitHub release.

Publishing authenticates with crates.io
[trusted publishing](https://crates.io/docs/trusted-publishing) over OIDC, so no
registry token is stored in the repository. The workflow lives in
[`.github/workflows/release-plz.yml`](.github/workflows/release-plz.yml).

`0.1.0` was published by hand to bootstrap the crate — trusted publishing can
only be configured once a crate already exists on crates.io. Every release after
it is fully automated by merging the release PR.
