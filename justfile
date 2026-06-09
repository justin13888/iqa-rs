# List available recipes
default:
    @just --list

# Format all code
format:
    cargo fmt

# Check formatting without modifying files
format-check:
    cargo fmt --check

# Run the linter
lint:
    cargo clippy -- -D warnings

# Run the linter and apply fixes
lint-fix:
    cargo clippy --fix --allow-dirty --allow-staged

# Run the test suite
test:
    cargo test

# Generate a code coverage report
coverage:
    cargo llvm-cov

# Measure compiled size per feature combo and verify the FFI compiles away
size:
    ./scripts/check-size.sh

# --- Mutation testing -------------------------------------------------------
# Install once with `cargo install cargo-mutants`. A surviving (MISSED) mutant
# is a hole in the suite; the bar is zero survivors except the documented
# equivalents in `.cargo/mutants.toml`. Features are scoped per metric because
# the crate cannot build with `--all-features` (the lcms2 backends are
# mutually exclusive).

# Mutation-test PSNR.
mutants-psnr:
    cargo mutants --no-default-features --features psnr -f src/psnr.rs

# Mutation-test SSIM. `ms-ssim` is enabled too: src/ssim.rs also computes the
# contrast-structure (cs) term, which only MS-SSIM consumes, so MS-SSIM's tests
# are what pin those lines.
mutants-ssim:
    cargo mutants --no-default-features --features ssim,ms-ssim -f src/ssim.rs

# Mutation-test DSSIM (the transform itself; `dssim` pulls in `ssim`).
mutants-dssim:
    cargo mutants --no-default-features --features dssim -f src/dssim.rs

# Mutation-test all three previously-untested metrics.
mutants: mutants-psnr mutants-ssim mutants-dssim

# Audit sweep over the already-"stable" metrics. Slow: the FFI metrics rebuild
# vendored C/C++. cargo-mutants only mutates the Rust wrappers; the C++ cores
# stay pinned by their golden reference tests.
mutants-sweep:
    cargo mutants --no-default-features --features ms-ssim -f src/ms_ssim.rs
    cargo mutants --no-default-features --features psnr-hvs-m -f src/psnr_hvs_m.rs
    cargo mutants --no-default-features --features ciede2000 -f src/ciede2000.rs
    cargo mutants --no-default-features --features ssimulacra2,vendored-lcms2 -f src/ssimulacra2/mod.rs -f src/ssimulacra2/ffi.rs
    cargo mutants --no-default-features --features butteraugli,vendored-lcms2 -f src/butteraugli/mod.rs -f src/butteraugli/ffi.rs
