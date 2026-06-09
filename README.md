# iqa

[![Crates.io](https://img.shields.io/crates/v/iqa.svg)](https://crates.io/crates/iqa)
[![Docs.rs](https://img.shields.io/docsrs/iqa)](https://docs.rs/iqa)
[![CI](https://github.com/justin13888/iqa-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/justin13888/iqa-rs/actions/workflows/ci.yml)
![License](https://img.shields.io/crates/l/iqa.svg)

`iqa` provides a single, ergonomic API over the patchwork of visual quality assessment metrics available in the Rust ecosystem. It wraps existing crates where they exist and fills in the gaps where they don't, so you can compute PSNR, SSIMULACRA2, and friends without juggling a different type, color space, and pixel format for each one.

## Installation

```sh
cargo add iqa
```

This pulls in every metric. Some (such as `ssimulacra2`) compile vendored C/C++
and therefore need a C++ toolchain — but no system libraries: `lcms2` is
vendored and built from source by default. See
[Cargo features](#cargo-features) for the details. For a pure-Rust build with no
C/C++ toolchain at all, disable the defaults and take just the metrics you need:

```toml
[dependencies]
iqa = { version = "0.1", default-features = false, features = ["psnr"] }
```

## Quick start

`iqa` consumes a tightly packed, row-major sample buffer and deliberately
leaves image decoding to you (here, the [`image`](https://crates.io/crates/image)
crate):

```rust
use iqa::{Image, PsnrOptions, SsimOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Decode however you like, then hand iqa the raw samples.
    let reference = image::open("reference.png")?.to_rgb8();
    let distorted = image::open("distorted.jpg")?.to_rgb8();

    let (w, h) = reference.dimensions();
    let reference = Image::srgb8(w, h, reference.into_raw())?;
    let (w, h) = distorted.dimensions();
    let distorted = Image::srgb8(w, h, distorted.into_raw())?;

    // PSNR — pure Rust, always available.
    let psnr = iqa::psnr(&reference, &distorted, PsnrOptions::default())?;
    println!("PSNR:        {psnr:.3} dB");

    // SSIM — pure Rust; 1.0 = identical.
    let ssim = iqa::ssim(&reference, &distorted, SsimOptions::default())?;
    println!("SSIM:        {ssim:.3}");

    // DSSIM — pure Rust; (1 - SSIM)/2, so 0.0 = identical and lower is better.
    let dssim = iqa::dssim(&reference, &distorted, iqa::DssimOptions::default())?;
    println!("DSSIM:       {dssim:.3}");

    // SSIMULACRA2 — requires the `ssimulacra2` feature; 100 = identical.
    let ssimulacra2 = iqa::ssimulacra2(&reference, &distorted)?;
    println!("SSIMULACRA2: {ssimulacra2:.3}");

    // Butteraugli — requires the `butteraugli` feature; 0 = identical, lower is better.
    let butteraugli = iqa::butteraugli(&reference, &distorted, iqa::ButteraugliOptions::default())?;
    println!("Butteraugli: {butteraugli:.3}");

    Ok(())
}
```

Both inputs must share one pixel format, so a color-space, channel, or bit-depth
mismatch is a compile error rather than a meaningless score. See
[`examples/compare.rs`](examples/compare.rs) for the full pipeline — run it with
`cargo run --release --example compare -- reference.png distorted.jpg`.

## Metrics

The table below tracks the planned metric set — the nine-metric full-reference core that covers essentially every published JXL/AVIF codec comparison from the last two years, plus LPIPS as a learned reference metric.

The `Implementation` column points at the implementation `iqa` is built on. We prefer porting or binding the upstream source implementation over reusing a Rust-specific reimplementation: cross-compilation is a requirement, but portability beyond that is not, so staying close to the reference keeps results faithful.

| IQA         | Implementation                                                                              | Status          |
| ----------- | ------------------------------------------------------------------------------------------- | --------------- |
| SSIMULACRA2 | [cloudinary/ssimulacra2](https://github.com/cloudinary/ssimulacra2)                         | Require testing |
| Butteraugli | [libjxl](https://github.com/libjxl/libjxl)                                                  | Stable and tested |
| DSSIM       | Native implementation†                                                                      | Require testing |
| XPSNR       | [fraunhoferhhi/xpsnr](https://github.com/fraunhoferhhi/xpsnr)                               | Planned         |
| PSNR        | Native implementation                                                                       | Require testing |
| PSNR-HVS-M  | [Ponomarenko `psnrhvsm.m`](https://www.ponomarenko.info/psnrhvsm.htm) | Stable and tested |
| SSIM        | Native implementation                                                                       | Require testing |
| MS-SSIM     | [xiph/daala — `tools/ssim.c`](https://github.com/xiph/daala/blob/master/tools/ssim.c)       | Planned         |
| CIEDE2000   | Native implementation                                                                       | Planned         |
| LPIPS       | [richzhang/PerceptualSimilarity](https://github.com/richzhang/PerceptualSimilarity)         | Not planned*    |

*: LPIPS is Python implementation only.

†: DSSIM is implemented as the classic structural-dissimilarity transform
`(1 - SSIM) / 2` over the native SSIM (Wang et al. 2004). This is intentionally
**not** [kornelski/dssim](https://github.com/kornelski/dssim)'s multi-scale
L\*a\*b\* metric: that implementation is AGPL-licensed and defined only by its
source, so it cannot be reproduced under this crate's permissive
(MIT OR Apache-2.0) license. No numeric parity with it is implied.

<!-- TODO: the links for xiph/daala are broken. I also suspect they don't have a good library implementation for them. -->

**Status legend:**

- **Planned** — selected for implementation, not yet started.
- **Require testing** — implemented, but not yet validated against reference outputs.
- **Stable and tested** — implemented and verified against the reference implementation.

Butteraugli is cross-validated numerically: `tests/butteraugli_reference.rs`
checks `iqa::butteraugli` against the exact distances libjxl's own
`butteraugli_main` (v0.8.2, the vendored version) produces on the same pixels —
matching to the reference tool's printed precision. `scripts/gen-butteraugli-goldens.sh`
rebuilds those reference values from source.

PSNR-HVS-M is cross-validated the same way: `tests/psnr_hvs_m_reference.rs` pins
`iqa::psnr_hvs_m` against Ponomarenko's original `psnrhvsm.m` run on the same
grayscale fixtures (`scripts/gen-psnrhvs-goldens.sh`, via Octave) — matching
exactly — with an independent NumPy reimplementation (`gen-psnrhvs-goldens.py`)
as a no-Octave cross-check.

## Cargo features

Each metric is gated behind its own Cargo feature:

| Feature          | Default | Notes                                                                          |
| ---------------- | ------- | ------------------------------------------------------------------------------ |
| `psnr`           | yes     | Native Rust; no system dependencies.                                           |
| `ssim`           | yes     | Native Rust; no system dependencies.                                           |
| `dssim`          | yes     | Native Rust; structural dissimilarity `(1 - SSIM) / 2`. Enables `ssim`.        |
| `psnr-hvs-m`     | yes     | Native Rust; DCT-domain PSNR with a CSF and contrast masking.                  |
| `ssimulacra2`    | yes     | Binds the vendored C++ reference; see the build requirements below.            |
| `butteraugli`    | yes     | Binds vendored libjxl C++; shares the same native build as `ssimulacra2`.       |
| `vendored-lcms2` | yes     | Builds the `lcms2` dependency from vendored source — no system lib needed. Mutually exclusive with `system-lcms2`. |
| `system-lcms2`   | no      | Links a system `lcms2` via `pkg-config` instead. Mutually exclusive with `vendored-lcms2` — enable exactly one. |

**Every metric is enabled by default for convenience** — `cargo add iqa`
gets you the full set. Some metrics (such as `ssimulacra2`) bind native C/C++
code and therefore need a C++ toolchain and system libraries to build.

For leaner builds, disable the default features and opt into exactly the
metrics you need:

```toml
[dependencies]
# Pure-Rust subset only — no C/C++ toolchain required.
iqa = { version = "0.1", default-features = false, features = ["psnr"] }
```

### Building the C++ metrics (`ssimulacra2`, `butteraugli`)

SSIMULACRA2 and Butteraugli are bound via FFI to their original libjxl-derived
C++ rather than reimplemented, and share one native build. Enabling either
(including via the default feature set) needs a native build environment:

1. **A C++ toolchain.** `build.rs` compiles the reference with the `cc` crate,
   which uses `c++` by default (override with the `CXX` environment variable).

That is the only requirement. libjxl's color management depends on `lcms2`,
which `iqa` **vendors and builds from source by default** (the `vendored-lcms2`
feature) — there is no system library to install.

If you would rather link a system `lcms2` (for example, as a distro packager),
turn the vendored feature off and enable `system-lcms2`, which locates the
library via `pkg-config`:

```toml
[dependencies]
iqa = { version = "0.1", default-features = false, features = ["psnr", "ssim", "ssimulacra2", "system-lcms2"] }
```

```sh
sudo apt install liblcms2-dev   # Debian / Ubuntu
brew install little-cms2        # macOS / Homebrew
```

When you depend on `iqa` from crates.io the vendored C/C++ sources (including
`lcms2`) are packaged inside the published crate, so there are no submodules to
fetch.

**Building from a git checkout** (contributors) additionally needs those
sources, which live under `third_party/` as git submodules (`ssimulacra2`,
`highway`, and `lcms2`):

```sh
git submodule update --init --recursive
```

Butteraugli's extra libjxl sources are hand-vendored directly under
`third_party/butteraugli/` (see its README), so they need no submodule fetch.

Either way, build or test with the feature enabled:

```sh
cargo test --features ssimulacra2,butteraugli
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

### Checking size

`just size` builds the library under every unique feature combination and prints
the compiled footprint of each, plus the published-crate size. It also enforces
that the C++ FFI **compiles away cleanly**: every pure-Rust combo must
link zero native code and pull neither `cc` nor `pkg-config` into its build
graph. A violation exits non-zero. The same check runs weekly (and on any PR that
touches `Cargo.toml`, `build.rs`, `third_party/`, or the script) via the `Size`
workflow, which posts the size table to the run summary.

### Releasing

Releases are automated with [release-plz](https://release-plz.dev) and driven by
[Conventional Commits](https://www.conventionalcommits.org): the commit messages
landed on `master` decide the next version number and fill in `CHANGELOG.md`.

#### How release-plz picks the next version

release-plz scans every commit landed since the last release tag, maps each to a
bump from its Conventional Commit type, and applies the **largest** bump any one
of them implies. The version in the open release PR therefore reflects everything
accumulated on `master` since the last release, and rises as more commits land.

While the crate is pre-1.0 (`0.y.z`) the bumps are deliberately conservative —
following Cargo's compatibility rules, the *minor* slot plays the role of "major",
so a breaking change moves `0.1.z → 0.2.0` rather than `1.0.0`:

| Highest-ranked commit on `master`           | Bump while pre-1.0 (`0.y.z`) | Bump once `≥ 1.0.0` |
| ------------------------------------------- | ---------------------------- | ------------------- |
| `feat!:` / `fix!:` / any `BREAKING CHANGE:` | minor (`0.1.1 → 0.2.0`)      | major               |
| `feat:`                                     | patch (`0.1.1 → 0.1.2`)      | minor               |
| `fix:`                                      | patch (`0.1.1 → 0.1.2`)      | patch               |
| `docs:`, `chore:`, `test:`, `refactor:`, …  | patch (`0.1.1 → 0.1.2`)      | patch               |

The practical consequence: while the crate is still `0.1.z`, a plain `feat:` does
**not** reach `0.2.0` — only a breaking-change commit (`feat!:` or a
`BREAKING CHANGE:` footer) bumps the minor. This is why `0.1.1` shipped three new
metrics as a patch rather than a minor release.

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
