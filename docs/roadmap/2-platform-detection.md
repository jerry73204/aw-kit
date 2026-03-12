# Phase 2: Platform Detection & Resolution

> Detect or validate the target platform (arch, device, JetPack version) and resolve it into build parameters.

---

## Design

Platform resolution is the bridge between user intent and concrete build decisions. It determines base images, CUDA architecture flags, Docker runtime options, and device mounts.

Two paths:

1. **Auto-detect** — no `[platform]` in manifest. aw-kit probes the host via `uname -m`, `/etc/nv_tegra_release`, and GPU device queries.
2. **Explicit** — `[platform]` section present. aw-kit validates the fields and resolves derived parameters.

Both paths produce a `ResolvedPlatform` struct that downstream phases consume.

### Platform knowledge base

aw-kit maintains an internal table of known platforms:

| Device | Arch | SM | JetPack versions | Base image |
|--------|------|----|-------------------|------------|
| `x86_64` desktop | amd64 | varies | N/A | `nvidia/cuda:12.x-devel-ubuntu22.04` |
| `jetson-agx-orin` | arm64 | 87 | 6.0, 6.1 | `nvcr.io/nvidia/l4t-cuda:12.x-devel` |
| `jetson-orin-nx` | arm64 | 87 | 6.0, 6.1 | `nvcr.io/nvidia/l4t-cuda:12.x-devel` |
| `jetson-orin-nano` | arm64 | 87 | 6.0, 6.1 | `nvcr.io/nvidia/l4t-cuda:12.x-devel` |

### Resolved output

```rust
struct ResolvedPlatform {
    arch: Arch,                          // amd64 | arm64
    device: Option<String>,              // e.g. "jetson-agx-orin"
    jetpack: Option<String>,             // e.g. "6.1"
    cuda_arch: Option<u32>,              // e.g. 87
    base_image: String,                  // resolved base image URI
    runtime: DockerRuntime,              // default | nvidia
    device_mounts: Vec<String>,          // /dev/nvhost-*, etc.
    image_suffix: String,               // e.g. "arm64" or "orin-jp6.1-arm64"
}
```

### CUDA component registry

Not all components need CUDA. aw-kit tracks which components have CUDA in their dependency graph:

- `perception` — TensorRT, CUDA pointcloud processing
- `localization` — CUDA NDT scan matching

Others (`planning`, `control`, `vehicle`, `sensing`) use standard arch images.

---

## Work Items

- [ ] Create `platform.rs` module
- [ ] Define `Arch` enum (`Amd64`, `Arm64`) with `FromStr` and `Display`
- [ ] Define `DockerRuntime` enum (`Default`, `Nvidia`)
- [ ] Define `ResolvedPlatform` struct with all derived fields
- [ ] Implement host detection: `uname -m` → `Arch`
- [ ] Implement JetPack detection: parse `/etc/nv_tegra_release` for L4T version, map to JetPack
- [ ] Implement GPU detection: parse `/proc/driver/nvidia/gpus/*/information` or `tegra` sysfs
- [ ] Build platform knowledge table as a static lookup (device → SM, base image, mounts)
- [ ] Implement `resolve_platform(manifest_platform: Option<Platform>) -> Result<ResolvedPlatform>`
- [ ] Handle auto-detect path: probe host, match against known devices
- [ ] Handle explicit path: validate fields against knowledge table, error on unknown device
- [ ] Implement CUDA component registry: `fn needs_cuda_rebuild(component: &str) -> bool`
- [ ] Print detection summary to stderr (as shown in design doc section 4.3)
- [ ] Print "Tip: Add to Autoware.toml..." when auto-detecting (nudge toward explicit config)
- [ ] Write unit tests with mocked filesystem/command outputs for each platform variant
- [ ] Write integration test that runs detection on the current host

---

## Acceptance Criteria

- [ ] On an x86_64 host without `[platform]`, resolves to `amd64` with standard CUDA base image
- [ ] On a Jetson Orin host without `[platform]`, detects device, JetPack, and SM correctly
- [ ] Explicit `[platform]` with valid fields resolves without probing the host
- [ ] Unknown device in `[platform]` produces a clear error listing supported devices
- [ ] `ResolvedPlatform.image_suffix` correctly encodes platform for image tagging
- [ ] `needs_cuda_rebuild()` returns true only for `perception` and `localization`
- [ ] Detection summary output matches the format in the design doc
