# Phase 1: Project Bootstrap & Manifest Parsing

> Foundation: Rust project setup, CLI skeleton, and `Autoware.toml` parsing.

---

## Design

aw-kit is a single static Rust binary with no runtime dependencies beyond `docker`. This phase establishes the project structure, CLI framework (clap), and the manifest parser that converts `Autoware.toml` into typed Rust structs.

The manifest is the single source of truth for user intent. Every subsequent phase depends on a correctly parsed `ManifestConfig`. The TOML schema must support all sections defined in the design doc: `[workspace]`, `[components]`, `[platform]`, `[patch.<component>]`, `[[package]]`, and `[registry]`.

### Crate layout

```
aw-kit/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs           # entry point, clap dispatch
│   ├── cli.rs            # command definitions (build, run, new, upgrade, push)
│   ├── manifest.rs       # Autoware.toml → ManifestConfig
│   ├── error.rs          # unified error types
│   └── lib.rs            # re-exports
├── tests/
│   └── fixtures/         # sample Autoware.toml files for each scenario
└── README.md
```

### Key dependencies

- `clap` (derive) — CLI argument parsing
- `serde` + `toml` — manifest deserialization
- `thiserror` — error types
- `tracing` + `tracing-subscriber` — structured logging

### Manifest types (sketch)

```rust
struct ManifestConfig {
    workspace: Workspace,
    components: BTreeMap<String, bool>,
    platform: Option<Platform>,
    patch: BTreeMap<String, BTreeMap<String, PatchSource>>,
    package: Vec<Package>,
    registry: Option<Registry>,
}

struct Workspace { autoware: String }
struct Platform { arch: Option<String>, device: Option<String>, jetpack: Option<String> }
enum PatchSource { Git { git: String, branch: Option<String>, tag: Option<String> }, Path { path: PathBuf } }
struct Package { name: String, path: PathBuf, extends: String }
struct Registry { url: String, prefix: String }
```

---

## Work Items

- [x] Initialize Rust project with `cargo init` in `aw-kit/` directory at repo root
- [x] Add `Cargo.toml` with dependencies: `clap`, `serde`, `toml`, `thiserror`, `tracing`, `tracing-subscriber`
- [x] Define CLI commands in `cli.rs` using clap derive: `build`, `run`, `new`, `upgrade`, `push`, `rebase`
- [x] Implement `ManifestConfig` struct hierarchy in `manifest.rs` with serde Deserialize
- [x] Implement `ManifestConfig::from_file(path)` that reads and parses `Autoware.toml`
- [x] Implement validation logic: required fields, valid component names, patch references match declared components
- [x] Define error types in `error.rs` using `thiserror`
- [x] Create test fixtures: `minimal.toml`, `patched.toml`, `orin.toml`, `custom-pkg.toml`, `full.toml`
- [x] Write unit tests for manifest parsing of each fixture
- [x] Write unit tests for validation error cases (missing workspace, patch referencing undeclared component, etc.)
- [x] Wire CLI to parse manifest and print resolved config (`aw-kit build --dry-run` shows parsed manifest)
- [x] Set up `tracing` subscriber initialization in `main.rs`
- [ ] Add CI workflow for `cargo test`, `cargo clippy`, `cargo fmt --check`

---

## Acceptance Criteria

- [x] `cargo build --release` produces a static binary under 10 MB
- [x] `aw-kit --help` prints all subcommands with descriptions
- [x] `aw-kit build --dry-run` in a directory with `Autoware.toml` prints the parsed manifest as structured output
- [x] All 5 fixture TOML files parse without error
- [x] Invalid manifests produce clear, actionable error messages with line numbers
- [x] `cargo clippy` and `cargo fmt --check` pass with zero warnings
- [x] Unit test coverage for all manifest field combinations
