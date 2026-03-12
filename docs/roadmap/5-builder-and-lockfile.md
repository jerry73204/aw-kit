# Phase 5: Builder & Lock File

> Execute `docker buildx build` for generated Dockerfiles and write `Autoware.lock` with pinned digests.

---

## Design

The builder is the execution engine that turns the codegen output into container images. It wraps `docker buildx` with caching, progress reporting, and digest capture. After all builds complete, it writes `Autoware.lock` to pin the exact state.

### Build execution

For each `BuildStep::Pull`:
- `docker pull <image>` (or `docker buildx imagetools inspect` for digest-only)
- Capture the image digest

For each `BuildStep::BuildOverlay`:
- `docker buildx build -f <dockerfile> -t <tag> <context>`
- Use BuildKit cache (`--cache-from`, `--cache-to`) for layer reuse
- Capture the built image digest

Build steps execute in dependency order (layers must build sequentially per component), but independent components can build in parallel.

### Lock file format

```toml
# Autoware.lock — auto-generated. Commit this to git.
[workspace]
autoware    = "0.45.1"
generated   = "2026-03-12T10:00:00Z"

[[component]]
name         = "localization"
image        = "ghcr.io/autowarefoundation/openadkit/localization"
tag          = "0.45.1-arm64"
digest       = "sha256:..."
patched      = true
patch-digest = "sha256:..."

[[package]]
name          = "autosdv_behavioral_planner"
source-digest = "sha256:..."
built-digest  = "sha256:..."
```

### `--locked` mode

`aw-kit build --locked` reads `Autoware.lock` and verifies all images match their pinned digests. If any mismatch, it errors without building. This enables reproducible deployments.

### Incremental builds

Compare current source hashes against lock file digests. Only rebuild components whose inputs changed. Report what was skipped and why.

---

## Work Items

- [x] Create `builder.rs` module with `DockerEngine` trait for testability
- [x] Implement `execute_plan(engine, manifest, plan, platform, project_root) -> Result<BuildResult>`
- [x] Implement image pulling with digest capture: `docker pull` + `docker inspect --format`
- [x] Implement overlay building: `docker buildx build` with correct `-f`, `-t`, `--platform`, context
- [ ] Add BuildKit cache flags: `--cache-from type=local,src=.aw-kit/cache` and `--cache-to`
- [ ] Implement parallel builds for independent components (tokio tasks or rayon)
- [x] Stream build output to stderr (stdout/stderr inherited from docker subprocess)
- [x] Capture image digests after build via `docker inspect`
- [x] Implement progress reporting: pull/build count and elapsed time
- [x] Create `lockfile.rs` module
- [x] Define `LockFile` struct with serde Serialize + Deserialize
- [x] Implement `LockFile::write(path)` — serialize to TOML with header comment
- [x] Implement `LockFile::read(path) -> Result<LockFile>`
- [x] Implement `--locked` mode: read lock file, verify all digests match, error on mismatch
- [ ] Implement incremental comparison: `LockFile::diff(current_sources) -> Vec<ChangedComponent>`
- [ ] Integrate incremental detection into build pipeline: skip unchanged components
- [x] Print build summary: time elapsed, images pulled/built
- [x] Handle build failures gracefully: report which step failed with actionable error message
- [x] Implement source fetching: `git clone --depth 1` for git patches, existence check for local paths
- [x] Wire builder into `main.rs`: `aw-kit build` executes plan and writes `Autoware.lock`
- [x] Implement `build_lock()` to construct `LockFile` from `BuildResult`
- [x] Write unit tests: mock `DockerEngine` verifies correct pull/build invocations
- [x] Write unit tests: lock file serialization round-trip
- [ ] Write unit tests: incremental diff detection (changed vs unchanged sources)
- [x] Write unit tests: `--locked` mode verification with matching/mismatched digests

---

## Acceptance Criteria

- [x] `aw-kit build` pulls upstream images and reports their digests
- [x] `aw-kit build` with patches builds overlay images using generated Dockerfiles
- [x] Built images are tagged following the naming scheme from Phase 3
- [x] `Autoware.lock` is written after successful build with all digests
- [x] `Autoware.lock` format is TOML with `[[component]]` and `[[package]]` arrays
- [x] `aw-kit build --locked` succeeds when lock file matches current state
- [x] `aw-kit build --locked` fails with clear error when digests mismatch
- [ ] Incremental builds skip components with unchanged source digests
- [x] Build failures produce actionable error messages identifying the failing step
- [ ] Parallel builds of independent components work correctly
