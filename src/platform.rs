use std::{fmt, str::FromStr};

use crate::{
    error::{Error, Result},
    manifest::Platform,
};

// ---------------------------------------------------------------------------
// Arch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Amd64,
    Arm64,
}

impl FromStr for Arch {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "amd64" | "x86_64" => Ok(Self::Amd64),
            "arm64" | "aarch64" => Ok(Self::Arm64),
            other => Err(Error::Platform(format!(
                "unknown arch '{other}'. Supported: amd64, arm64",
            ))),
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Amd64 => write!(f, "amd64"),
            Self::Arm64 => write!(f, "arm64"),
        }
    }
}

// ---------------------------------------------------------------------------
// DockerRuntime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerRuntime {
    Default,
    Nvidia,
}

impl fmt::Display for DockerRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Nvidia => write!(f, "nvidia"),
        }
    }
}

// ---------------------------------------------------------------------------
// Device knowledge base
// ---------------------------------------------------------------------------

struct DeviceInfo {
    name: &'static str,
    arch: Arch,
    cuda_arch: u32,
    jetpack_versions: &'static [&'static str],
    base_image: &'static str,
    device_mounts: &'static [&'static str],
}

const KNOWN_DEVICES: &[DeviceInfo] = &[
    DeviceInfo {
        name: "jetson-agx-orin",
        arch: Arch::Arm64,
        cuda_arch: 87,
        jetpack_versions: &["6.0", "6.1"],
        base_image: "nvcr.io/nvidia/l4t-cuda:12.6.68-devel",
        device_mounts: &[
            "/dev/nvhost-ctrl",
            "/dev/nvhost-ctrl-gpu",
            "/dev/nvhost-prof-gpu",
            "/dev/nvmap",
        ],
    },
    DeviceInfo {
        name: "jetson-orin-nx",
        arch: Arch::Arm64,
        cuda_arch: 87,
        jetpack_versions: &["6.0", "6.1"],
        base_image: "nvcr.io/nvidia/l4t-cuda:12.6.68-devel",
        device_mounts: &[
            "/dev/nvhost-ctrl",
            "/dev/nvhost-ctrl-gpu",
            "/dev/nvhost-prof-gpu",
            "/dev/nvmap",
        ],
    },
    DeviceInfo {
        name: "jetson-orin-nano",
        arch: Arch::Arm64,
        cuda_arch: 87,
        jetpack_versions: &["6.0", "6.1"],
        base_image: "nvcr.io/nvidia/l4t-cuda:12.6.68-devel",
        device_mounts: &[
            "/dev/nvhost-ctrl",
            "/dev/nvhost-ctrl-gpu",
            "/dev/nvhost-prof-gpu",
            "/dev/nvmap",
        ],
    },
];

fn known_device_names() -> String {
    KNOWN_DEVICES
        .iter()
        .map(|d| d.name)
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Default base images (no specific device)
// ---------------------------------------------------------------------------

const AMD64_BASE_IMAGE: &str = "nvidia/cuda:12.6.3-devel-ubuntu22.04";
const ARM64_BASE_IMAGE: &str = "arm64v8/ubuntu:22.04";

// ---------------------------------------------------------------------------
// CUDA component registry
// ---------------------------------------------------------------------------

const CUDA_COMPONENTS: &[&str] = &["perception", "localization"];

/// Returns `true` if the component has CUDA dependencies and needs a
/// platform-specific rebuild on Jetson targets.
pub fn needs_cuda_rebuild(component: &str) -> bool {
    CUDA_COMPONENTS.contains(&component)
}

// ---------------------------------------------------------------------------
// ResolvedPlatform
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ResolvedPlatform {
    pub arch: Arch,
    pub device: Option<String>,
    pub jetpack: Option<String>,
    pub cuda_arch: Option<u32>,
    pub base_image: String,
    pub runtime: DockerRuntime,
    pub device_mounts: Vec<String>,
    pub image_suffix: String,
}

impl ResolvedPlatform {
    /// Print a summary of the resolved platform to stderr.
    pub fn print_summary(&self) {
        eprintln!("Platform:");
        eprintln!("  arch:       {}", self.arch);
        if let Some(ref device) = self.device {
            eprintln!("  device:     {device}");
        }
        if let Some(ref jp) = self.jetpack {
            eprintln!("  jetpack:    {jp}");
        }
        if let Some(sm) = self.cuda_arch {
            eprintln!("  cuda arch:  SM {sm}");
        }
        eprintln!("  base image: {}", self.base_image);
        eprintln!("  runtime:    {}", self.runtime);
        if !self.device_mounts.is_empty() {
            eprintln!("  mounts:     {}", self.device_mounts.join(", "));
        }
        eprintln!("  suffix:     {}", self.image_suffix);
    }
}

/// Resolve a manifest `[platform]` into concrete build parameters.
pub fn resolve_platform(platform: &Platform) -> Result<ResolvedPlatform> {
    let arch: Arch = platform.arch.parse()?;

    match &platform.device {
        Some(device_name) => resolve_with_device(arch, device_name, &platform.jetpack),
        None => Ok(resolve_desktop(arch)),
    }
}

fn resolve_desktop(arch: Arch) -> ResolvedPlatform {
    let base_image = match arch {
        Arch::Amd64 => AMD64_BASE_IMAGE,
        Arch::Arm64 => ARM64_BASE_IMAGE,
    };

    ResolvedPlatform {
        arch,
        device: None,
        jetpack: None,
        cuda_arch: None,
        base_image: base_image.to_string(),
        runtime: DockerRuntime::Default,
        device_mounts: Vec::new(),
        image_suffix: arch.to_string(),
    }
}

fn resolve_with_device(
    arch: Arch,
    device_name: &str,
    jetpack: &Option<String>,
) -> Result<ResolvedPlatform> {
    let info = KNOWN_DEVICES
        .iter()
        .find(|d| d.name == device_name)
        .ok_or_else(|| {
            Error::Platform(format!(
                "unknown device '{device_name}'. Supported devices: {}",
                known_device_names(),
            ))
        })?;

    // Validate arch matches the device.
    if arch != info.arch {
        return Err(Error::Platform(format!(
            "device '{device_name}' requires arch '{}', but manifest declares '{arch}'",
            info.arch,
        )));
    }

    // JetPack is required for Jetson devices.
    let jp = jetpack.as_ref().ok_or_else(|| {
        Error::Platform(format!(
            "device '{device_name}' requires 'jetpack' to be set. Supported versions: {}",
            info.jetpack_versions.join(", "),
        ))
    })?;

    // Validate JetPack version.
    if !info.jetpack_versions.contains(&jp.as_str()) {
        return Err(Error::Platform(format!(
            "unsupported jetpack '{jp}' for device '{device_name}'. Supported: {}",
            info.jetpack_versions.join(", "),
        )));
    }

    // Build image suffix: e.g. "orin-jp6.1-arm64"
    // Strip "jetson-" prefix for brevity, and "agx-"/"nano-" etc. for the common case.
    let short_device = device_name.strip_prefix("jetson-").unwrap_or(device_name);
    let image_suffix = format!("{short_device}-jp{jp}-{arch}");

    Ok(ResolvedPlatform {
        arch,
        device: Some(device_name.to_string()),
        jetpack: Some(jp.clone()),
        cuda_arch: Some(info.cuda_arch),
        base_image: info.base_image.to_string(),
        runtime: DockerRuntime::Nvidia,
        device_mounts: info
            .device_mounts
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        image_suffix,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Platform;

    fn plat(arch: &str, device: Option<&str>, jetpack: Option<&str>) -> Platform {
        Platform {
            arch: arch.to_string(),
            device: device.map(|s| s.to_string()),
            jetpack: jetpack.map(|s| s.to_string()),
        }
    }

    // -- Desktop (no device) ------------------------------------------------

    #[test]
    fn amd64_desktop() {
        let r = resolve_platform(&plat("amd64", None, None)).unwrap();
        assert_eq!(r.arch, Arch::Amd64);
        assert!(r.device.is_none());
        assert!(r.cuda_arch.is_none());
        assert_eq!(r.runtime, DockerRuntime::Default);
        assert!(r.device_mounts.is_empty());
        assert_eq!(r.image_suffix, "amd64");
        assert!(r.base_image.contains("cuda"));
    }

    #[test]
    fn arm64_desktop() {
        let r = resolve_platform(&plat("arm64", None, None)).unwrap();
        assert_eq!(r.arch, Arch::Arm64);
        assert_eq!(r.runtime, DockerRuntime::Default);
        assert_eq!(r.image_suffix, "arm64");
    }

    // -- Jetson Orin --------------------------------------------------------

    #[test]
    fn jetson_agx_orin() {
        let r = resolve_platform(&plat("arm64", Some("jetson-agx-orin"), Some("6.1"))).unwrap();
        assert_eq!(r.arch, Arch::Arm64);
        assert_eq!(r.device.as_deref(), Some("jetson-agx-orin"));
        assert_eq!(r.jetpack.as_deref(), Some("6.1"));
        assert_eq!(r.cuda_arch, Some(87));
        assert_eq!(r.runtime, DockerRuntime::Nvidia);
        assert!(!r.device_mounts.is_empty());
        assert!(r.base_image.contains("l4t"));
        assert_eq!(r.image_suffix, "agx-orin-jp6.1-arm64");
    }

    #[test]
    fn jetson_orin_nx() {
        let r = resolve_platform(&plat("arm64", Some("jetson-orin-nx"), Some("6.0"))).unwrap();
        assert_eq!(r.device.as_deref(), Some("jetson-orin-nx"));
        assert_eq!(r.cuda_arch, Some(87));
        assert_eq!(r.image_suffix, "orin-nx-jp6.0-arm64");
    }

    #[test]
    fn jetson_orin_nano() {
        let r = resolve_platform(&plat("arm64", Some("jetson-orin-nano"), Some("6.1"))).unwrap();
        assert_eq!(r.device.as_deref(), Some("jetson-orin-nano"));
        assert_eq!(r.image_suffix, "orin-nano-jp6.1-arm64");
    }

    // -- Validation errors --------------------------------------------------

    #[test]
    fn unknown_arch_rejected() {
        let err = resolve_platform(&plat("riscv64", None, None)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown arch 'riscv64'"), "{msg}");
    }

    #[test]
    fn unknown_device_rejected() {
        let err = resolve_platform(&plat("arm64", Some("jetson-xavier"), Some("5.0"))).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown device 'jetson-xavier'"), "{msg}");
        assert!(msg.contains("jetson-agx-orin"), "{msg}");
    }

    #[test]
    fn jetson_missing_jetpack_rejected() {
        let err = resolve_platform(&plat("arm64", Some("jetson-agx-orin"), None)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("requires 'jetpack'"), "{msg}");
    }

    #[test]
    fn jetson_bad_jetpack_rejected() {
        let err =
            resolve_platform(&plat("arm64", Some("jetson-agx-orin"), Some("5.0"))).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unsupported jetpack '5.0'"), "{msg}");
    }

    #[test]
    fn arch_mismatch_rejected() {
        let err =
            resolve_platform(&plat("amd64", Some("jetson-agx-orin"), Some("6.1"))).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("requires arch 'arm64'"), "{msg}");
    }

    // -- CUDA component registry --------------------------------------------

    #[test]
    fn cuda_components() {
        assert!(needs_cuda_rebuild("perception"));
        assert!(needs_cuda_rebuild("localization"));
        assert!(!needs_cuda_rebuild("planning"));
        assert!(!needs_cuda_rebuild("control"));
        assert!(!needs_cuda_rebuild("vehicle"));
        assert!(!needs_cuda_rebuild("sensing"));
    }
}
