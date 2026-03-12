0# Phase 2: Platform Resolution

> Validate the mandatory `[platform]` section and resolve it into build parameters.

---

## Design

Platform resolution is the bridge between user intent and concrete build decisions. It determines base images, CUDA architecture flags, Docker runtime options, and device mounts.

`[platform]` is **mandatory** in `Autoware.toml`. The `arch` field is required; `device` and `jetpack` are optional (only needed for Jetson targets). This makes the manifest fully declarative and portable — no host probing, no implicit behavior.

aw-kit validates the declared platform against a known-platforms table and resolves all derived build parameters into a `ResolvedPlatform` struct that downstream phases consume.

### Platform knowledge base

aw-kit maintains an internal table of known platforms:

| Device             | Arch  | SM | JetPack versions | Base image                           |
|--------------------|-------|----|------------------|--------------------------------------|
| (none / desktop)   | amd64 | —  | N/A              | `nvidia/cuda:12.x-devel-ubuntu22.04` |
| `jetson-agx-orin`  | arm64 | 87 | 6.0, 6.1         | `nvcr.io/nvidia/l4t-cuda:12.x-devel` |
| `jetson-orin-nx`   | arm64 | 87 | 6.0, 6.1         | `nvcr.io/nvidia/l4t-cuda:12.x-devel` |
| `jetson-orin-nano` | arm64 | 87 | 6.0, 6.1         | `nvcr.io/nvidia/l4t-cuda:12.x-devel` |

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
    image_suffix: String,                // e.g. "arm64" or "orin-jp6.1-arm64"
}
```

### CUDA component registry

Not all components need CUDA. aw-kit tracks which components have CUDA in their dependency graph:

- `perception` — TensorRT, CUDA pointcloud processing
- `localization` — CUDA NDT scan matching

Others (`planning`, `control`, `vehicle`, `sensing`) use standard arch images.

---

## Work Items

- [x] Create `platform.rs` module
- [x] Define `Arch` enum (`Amd64`, `Arm64`) with `FromStr` and `Display`
- [x] Define `DockerRuntime` enum (`Default`, `Nvidia`)
- [x] Define `ResolvedPlatform` struct with all derived fields
- [x] Build platform knowledge table as a static lookup (device → SM, base image, mounts)
- [x] Implement `resolve_platform(platform: &Platform) -> Result<ResolvedPlatform>`
- [x] Validate `arch` is a known value (`amd64` or `arm64`)
- [x] Validate `device` (if present) against knowledge table, error on unknown device
- [x] Validate `jetpack` is required when `device` is a Jetson, error if missing
- [x] Resolve derived fields: `cuda_arch`, `base_image`, `runtime`, `device_mounts`, `image_suffix`
- [x] Implement CUDA component registry: `fn needs_cuda_rebuild(component: &str) -> bool`
- [x] Print resolved platform summary to stderr
- [x] Write unit tests for amd64 desktop resolution (arch only, no device)
- [x] Write unit tests for Jetson Orin resolution (arch + device + jetpack)
- [x] Write unit tests for unknown device rejection
- [x] Write unit tests for missing jetpack on Jetson rejection

---

## Acceptance Criteria

- [x] `[platform]` without `arch` is rejected at parse time (serde required field)
- [x] `arch = "amd64"` with no device resolves to standard CUDA base image and default runtime
- [x] `arch = "arm64"` + `device = "jetson-agx-orin"` + `jetpack = "6.1"` resolves with L4T base, nvidia runtime, SM 87, device mounts
- [x] Unknown device produces a clear error listing supported devices
- [x] Jetson device without `jetpack` produces a clear error
- [x] `ResolvedPlatform.image_suffix` correctly encodes platform for image tagging
- [x] `needs_cuda_rebuild()` returns true only for `perception` and `localization`
