# aw-kit task runner

profile := "dev-release"

# List available recipes
default:
    @just --list

# Build the binary
build:
    cargo build --profile {{ profile }}

# Remove build artifacts
clean:
    cargo clean

# Run with arguments (e.g., just run -- build --dry-run)
run *ARGS:
    cargo run --profile {{ profile }} -- {{ ARGS }}

# Run clippy and format check
check:
    cargo +nightly fmt --check
    cargo clippy --profile {{ profile }} -- -D warnings

# Run tests
test:
    cargo nextest run --no-fail-fast

# Format code
format:
    cargo +nightly fmt

# Full CI pipeline: format check, clippy, test
ci: check test
