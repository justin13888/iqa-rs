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
