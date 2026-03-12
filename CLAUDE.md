# aw-kit

Autoware deployment toolkit — one manifest (`Autoware.toml`), all the container plumbing.

## Quick reference

```sh
just check    # clippy + test + fmt check
just format   # auto-format
just ci       # full CI pipeline
just run -- --manifest tests/fixtures/full.toml build --dry-run
```

## Project structure

```
src/
├── main.rs        # CLI entry point, tracing init, dry-run output
├── cli.rs         # clap command definitions
├── manifest.rs    # Autoware.toml parser + validation + unit tests
├── error.rs       # error types (ManifestRead, ManifestParse, Validation)
└── lib.rs         # module re-exports
tests/
├── fixtures/      # sample Autoware.toml files (minimal, patched, orin, custom-pkg, full)
└── manifest_fixtures.rs  # integration tests loading fixture files
```

## Conventions

- Rust edition 2024.
- Use `thiserror` for error types. All errors live in `error.rs`.
- Use `clap` derive for CLI. Global flags go on `Cli`, subcommand flags on `Command` variants.
- Use `tracing` for logging (not `println!` for diagnostics).
- Manifest types use `serde::Deserialize`. Validation runs after deserialization in `ManifestConfig::validate()`.
- Known component names are defined in `KNOWN_COMPONENTS` in `manifest.rs`.
- Test fixtures are TOML files in `tests/fixtures/`. Unit tests for parsing live in `manifest.rs`; integration tests in `tests/`.
- `cargo clippy -- -D warnings` must pass with zero warnings.
- `cargo fmt --check` must pass.

## Design docs

Implementation roadmap is in `../internal/roadmap/`. The design spec is `../aw-kit.md`.
