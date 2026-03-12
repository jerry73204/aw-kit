# Phase 7: Package Scaffolding & Upgrade Workflow

> `aw-kit new` scaffolds custom packages; `aw-kit upgrade` and `aw-kit rebase` manage version transitions.

---

## Design

### `aw-kit new` — Package Scaffolding

Creates a new ROS2 package that extends an Autoware component. The command:

```bash
aw-kit new autosdv_behavioral_planner --extends planning
```

Generates:
```
src/autosdv_behavioral_planner/
├── CMakeLists.txt          # find_package() calls from the extended component
├── package.xml             # ROS2 package manifest
├── include/
│   └── autosdv_behavioral_planner/
└── src/
    └── behavioral_planner_node.cpp
```

The key insight: `CMakeLists.txt` is pre-populated with `find_package()` calls for the dependencies available in the extended component's image. This means `colcon build` works immediately inside the devcontainer.

aw-kit maintains a dependency registry per component — the list of ROS2 packages installed in each component image. This can be:
- Hardcoded initially (extracted once from each image)
- Queried dynamically via `docker run <image> colcon list` (future)

The command also:
- Adds a `[[package]]` entry to `Autoware.toml`
- Generates `.devcontainer/devcontainer.json` (from Phase 4)

### `aw-kit upgrade` — Version Transition

```bash
aw-kit upgrade --to 0.46.0
```

Steps:
1. Update `[workspace] autoware` version in `Autoware.toml`
2. For each `[patch.<component>]`, check if the patch still applies cleanly to the new version
3. Report results: clean patches, conflicting patches
4. If all clean → update lock file
5. If conflicts → instruct user to run `aw-kit rebase <component>`

### `aw-kit rebase` — Patch Conflict Resolution

```bash
aw-kit rebase localization
```

For git-sourced patches:
1. Fetch the new upstream source for the patched package at the new version
2. Attempt `git cherry-pick` or `git am` of the patch commits onto the new base
3. If conflicts → open the user's editor (like `git rebase --continue`)
4. Once resolved → update the patch source and rebuild

For path-based patches:
1. Show a diff of what changed in the upstream package between versions
2. User manually reconciles their local patch
3. `aw-kit rebase --continue` after manual resolution

---

## Work Items

### Scaffolding (`aw-kit new`)

- [ ] Add `new` subcommand to CLI: `aw-kit new <name> --extends <component>`
- [ ] Create component dependency registry: map component name → list of available ROS2 packages
- [ ] Generate `CMakeLists.txt` with `find_package()` calls from the extended component
- [ ] Generate `package.xml` with correct package metadata and dependencies
- [ ] Generate boilerplate `src/<name>_node.cpp` with basic rclcpp node
- [ ] Generate `include/<name>/` directory structure
- [ ] Create source directory at `src/<name>/`
- [ ] Append `[[package]]` entry to `Autoware.toml`
- [ ] Trigger devcontainer generation (reuse Phase 4 codegen)
- [ ] Print next-steps guidance: "Open in VS Code with Remote Containers"
- [ ] Write unit tests: generated CMakeLists.txt contains correct find_package calls
- [ ] Write unit tests: package.xml is valid XML with correct fields

### Upgrade (`aw-kit upgrade`)

- [ ] Add `upgrade` subcommand to CLI: `aw-kit upgrade --to <version>`
- [ ] Update `Autoware.toml` workspace version
- [ ] For each patch, determine if upstream package changed between old and new version
- [ ] Implement patch compatibility check: try applying patch to new upstream
- [ ] Report results with clear status per patch (clean / conflict)
- [ ] On all-clean: update lock file, print success
- [ ] On conflicts: print instructions to run `aw-kit rebase`

### Rebase (`aw-kit rebase`)

- [ ] Add `rebase` subcommand to CLI: `aw-kit rebase <component>`
- [ ] Fetch upstream source at new version for the patched package
- [ ] Attempt automated patch application (cherry-pick / am)
- [ ] On conflict: open editor, print `aw-kit rebase --continue` instructions
- [ ] Implement `--continue` flag: verify conflicts resolved, proceed with build
- [ ] For path-based patches: show upstream diff, guide manual resolution
- [ ] Write integration tests: upgrade with no patches succeeds
- [ ] Write integration tests: upgrade with clean patch succeeds
- [ ] Write integration tests: upgrade with conflicting patch reports correctly

---

## Acceptance Criteria

### Scaffolding
- [ ] `aw-kit new foo --extends planning` creates a valid ROS2 package at `src/foo/`
- [ ] Generated `CMakeLists.txt` includes relevant `find_package()` calls for the planning component
- [ ] Generated `package.xml` is valid and lists correct dependencies
- [ ] `Autoware.toml` is updated with the new `[[package]]` entry
- [ ] DevContainer JSON is generated and references the correct image
- [ ] The scaffolded package builds successfully inside the component's container

### Upgrade
- [ ] `aw-kit upgrade --to 0.46.0` updates the workspace version
- [ ] Clean patches are reported as compatible
- [ ] Conflicting patches are reported with file-level conflict details
- [ ] Upgrade without patches completes without prompts

### Rebase
- [ ] `aw-kit rebase localization` attempts to reapply patches on the new version
- [ ] Conflicts open the user's editor with clear markers
- [ ] `aw-kit rebase --continue` resumes after manual conflict resolution
- [ ] Successfully rebased patches produce updated source and trigger rebuild
