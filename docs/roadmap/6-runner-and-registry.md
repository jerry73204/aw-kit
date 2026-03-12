# Phase 6: Runner, Registry & Push

> Execute `docker compose` on generated files, and support pushing/pulling built images to/from a registry.

---

## Design

### Runner

The runner is a thin wrapper around `docker compose` that operates on the generated compose file in `.aw-kit/compose/`. Commands:

- `aw-kit run` → `docker compose -f .aw-kit/compose/docker-compose.yml up`
- `aw-kit run --detach` → `docker compose up -d`
- `aw-kit stop` → `docker compose down`
- `aw-kit logs [component]` → `docker compose logs [-f] [service]`

The runner ensures `aw-kit build` has been run first (checks for `.aw-kit/compose/docker-compose.yml`). If not, it runs the build automatically or prompts the user.

### Registry

The `[registry]` section enables team workflows:

```toml
[registry]
url    = "harbor.autosdv.edu.tw"
prefix = "autosdv/openadkit"
```

Commands:
- `aw-kit push` — pushes all locally-built images to the configured registry
- `aw-kit build --pull` — checks the registry for pre-built patched images before building locally

Tag scheme on registry:
```
<registry>/<prefix>/<component>:<aw-version>[-<platform>][-p<patch-hash>][-<arch>]
```

### Registry image resolution

When `--pull` is used:
1. For each component that would need a build, compute the expected tag
2. Query the registry: `docker buildx imagetools inspect <tag>`
3. If the image exists and digest matches expected inputs → pull instead of build
4. If not found → fall back to local build

This avoids rebuilding on every Orin unit when one unit has already built and pushed.

---

## Work Items

- [x] Create `runner.rs` module
- [x] Implement `run(project_root, detach)` — invoke `docker compose up` on generated compose
- [x] Implement `stop(project_root)` — invoke `docker compose down`
- [x] Implement `logs(project_root, component, follow)` — invoke `docker compose logs`
- [x] Check for generated compose file existence before running; error if missing with guidance
- [x] Forward Docker Compose stdout/stderr/stdin to the terminal
- [ ] Handle Ctrl-C gracefully: forward SIGINT to compose, clean shutdown
- [x] Create `registry.rs` module
- [x] Implement `push(registry, plan, result)` — tag and push all locally-built overlay images
- [x] Implement `check_remote(image) -> Option<String>` — query registry via `docker buildx imagetools inspect`
- [x] Implement `pull_from_registry(registry, plan)` — pull pre-built overlays, re-tag as local tags
- [x] Integrate `--pull` flag into build pipeline: check remote before local build
- [ ] Implement `docker login` detection: warn if not authenticated to configured registry
- [x] Wire `push` subcommand in `main.rs` — reads lock file, pushes overlay images
- [x] Wire `--pull` flag in `build` subcommand — calls `pull_from_registry` before executing plan
- [x] Wire `run`, `stop`, `logs` subcommands in `main.rs`
- [x] Write unit tests: runner errors when compose file missing (3 tests)
- [x] Write unit tests: registry tag computation (2 tests)
- [ ] Write integration tests: push tags images with correct names

---

## Acceptance Criteria

- [x] `aw-kit run` starts all enabled components via Docker Compose
- [x] `aw-kit run --detach` starts components in background
- [x] `aw-kit stop` cleanly shuts down all running components
- [x] `aw-kit logs planning` shows logs for the planning service
- [x] `aw-kit logs -f` follows all service logs
- [x] Running without a prior build gives a clear error message pointing to `aw-kit build`
- [x] `aw-kit push` pushes built images with correct tags to configured registry
- [x] `aw-kit build --pull` checks registry for pre-built images before building locally
- [x] `aw-kit build --pull` falls back to local build when images not in registry
- [ ] Missing registry auth produces a helpful error message
