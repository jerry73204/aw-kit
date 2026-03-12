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

- [ ] Create `runner.rs` module
- [ ] Implement `run(detach: bool)` — invoke `docker compose up` on generated compose
- [ ] Implement `stop()` — invoke `docker compose down`
- [ ] Implement `logs(component: Option<String>, follow: bool)` — invoke `docker compose logs`
- [ ] Check for generated compose file existence before running; auto-build or error if missing
- [ ] Forward Docker Compose stdout/stderr to the terminal
- [ ] Handle Ctrl-C gracefully: forward SIGINT to compose, clean shutdown
- [ ] Create `registry.rs` module
- [ ] Implement `push(registry: &Registry, build_result: &BuildResult)` — tag and push all built images
- [ ] Implement `check_remote(registry: &Registry, tag: &str) -> Option<String>` — query registry for existing image
- [ ] Integrate `--pull` flag into build pipeline: check remote before local build
- [ ] Implement `docker login` detection: warn if not authenticated to configured registry
- [ ] Add `push` subcommand to CLI
- [ ] Add `--pull` flag to `build` subcommand
- [ ] Add `stop` and `logs` subcommands to CLI
- [ ] Write integration tests: runner invokes correct compose commands
- [ ] Write unit tests: registry tag computation
- [ ] Write integration tests: push tags images with correct names

---

## Acceptance Criteria

- [ ] `aw-kit run` starts all enabled components via Docker Compose
- [ ] `aw-kit run --detach` starts components in background
- [ ] `aw-kit stop` cleanly shuts down all running components
- [ ] `aw-kit logs planning` shows logs for the planning service
- [ ] `aw-kit logs -f` follows all service logs
- [ ] Running without a prior build gives a clear message and builds automatically
- [ ] `aw-kit push` pushes built images with correct tags to configured registry
- [ ] `aw-kit build --pull` skips local builds when pre-built images exist in registry
- [ ] `aw-kit build --pull` falls back to local build when images not in registry
- [ ] Missing registry auth produces a helpful error message
