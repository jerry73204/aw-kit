use std::{fmt, str::FromStr};

use crate::{
    error::{Error, Result},
    images,
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

impl Arch {
    /// Docker platform string for `--platform` flag.
    pub fn docker_platform(&self) -> &'static str {
        match self {
            Self::Amd64 => "linux/amd64",
            Self::Arm64 => "linux/arm64",
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
    device_mounts: &'static [&'static str],
}

const KNOWN_DEVICES: &[DeviceInfo] = &[
    DeviceInfo {
        name: "jetson-agx-orin",
        arch: Arch::Arm64,
        cuda_arch: 87,
        jetpack_versions: &["6.0", "6.1"],
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
// CUDA variant registry
// ---------------------------------------------------------------------------

/// Components that have pre-built `-cuda` image variants in the registry.
const CUDA_VARIANT_COMPONENTS: &[&str] = &["sensing-perception", "universe"];

/// Returns `true` if the component has a pre-built `-cuda` image variant.
pub fn has_cuda_variant(component: &str) -> bool {
    CUDA_VARIANT_COMPONENTS.contains(&component)
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
    pub use_cuda: bool,
    pub base_image: String,
    pub runtime: DockerRuntime,
    pub device_mounts: Vec<String>,
}

impl ResolvedPlatform {
    /// Print a summary of the resolved platform to stderr.
    pub fn print_summary(&self) {
        eprintln!("Platform:");
        eprintln!("  arch:       {}", self.arch);
        eprintln!("  docker:     {}", self.arch.docker_platform());
        if let Some(ref device) = self.device {
            eprintln!("  device:     {device}");
        }
        if let Some(ref jp) = self.jetpack {
            eprintln!("  jetpack:    {jp}");
        }
        if let Some(sm) = self.cuda_arch {
            eprintln!("  cuda arch:  SM {sm}");
        }
        eprintln!("  cuda:       {}", self.use_cuda);
        eprintln!("  base image: {}", self.base_image);
        eprintln!("  runtime:    {}", self.runtime);
        if !self.device_mounts.is_empty() {
            eprintln!("  mounts:     {}", self.device_mounts.join(", "));
        }
    }
}

/// Resolve a manifest `[platform]` into concrete build parameters.
pub fn resolve_platform(platform: &Platform) -> Result<ResolvedPlatform> {
    let arch: Arch = platform.arch.parse()?;

    match &platform.device {
        Some(device_name) => {
            resolve_with_device(arch, device_name, &platform.jetpack, platform.cuda)
        }
        None => Ok(resolve_desktop(arch, platform.cuda)),
    }
}

fn resolve_desktop(arch: Arch, cuda: Option<bool>) -> ResolvedPlatform {
    let images = images::load();
    ResolvedPlatform {
        arch,
        device: None,
        jetpack: None,
        cuda_arch: None,
        use_cuda: cuda.unwrap_or(false),
        base_image: images.base.desktop,
        runtime: DockerRuntime::Default,
        device_mounts: Vec::new(),
    }
}

fn resolve_with_device(
    arch: Arch,
    device_name: &str,
    jetpack: &Option<String>,
    cuda: Option<bool>,
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

    if arch != info.arch {
        return Err(Error::Platform(format!(
            "device '{device_name}' requires arch '{}', but manifest declares '{arch}'",
            info.arch,
        )));
    }

    let jp = jetpack.as_ref().ok_or_else(|| {
        Error::Platform(format!(
            "device '{device_name}' requires 'jetpack' to be set. Supported versions: {}",
            info.jetpack_versions.join(", "),
        ))
    })?;

    if !info.jetpack_versions.contains(&jp.as_str()) {
        return Err(Error::Platform(format!(
            "unsupported jetpack '{jp}' for device '{device_name}'. Supported: {}",
            info.jetpack_versions.join(", "),
        )));
    }

    // Jetson devices always use CUDA unless explicitly disabled.
    let use_cuda = cuda.unwrap_or(true);
    let images = images::load();

    Ok(ResolvedPlatform {
        arch,
        device: Some(device_name.to_string()),
        jetpack: Some(jp.clone()),
        cuda_arch: Some(info.cuda_arch),
        use_cuda,
        base_image: images.base.jetson,
        runtime: DockerRuntime::Nvidia,
        device_mounts: info
            .device_mounts
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
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
            cuda: None,
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
        assert!(!r.use_cuda);
        assert_eq!(r.runtime, DockerRuntime::Default);
        assert!(r.device_mounts.is_empty());
    }

    #[test]
    fn amd64_desktop_with_cuda() {
        let p = Platform {
            arch: "amd64".to_string(),
            cuda: Some(true),
            device: None,
            jetpack: None,
        };
        let r = resolve_platform(&p).unwrap();
        assert!(r.use_cuda);
        assert_eq!(r.runtime, DockerRuntime::Default);
    }

    #[test]
    fn arm64_desktop() {
        let r = resolve_platform(&plat("arm64", None, None)).unwrap();
        assert_eq!(r.arch, Arch::Arm64);
        assert_eq!(r.runtime, DockerRuntime::Default);
        assert!(!r.use_cuda);
    }

    // -- Jetson Orin --------------------------------------------------------

    #[test]
    fn jetson_agx_orin() {
        let r = resolve_platform(&plat("arm64", Some("jetson-agx-orin"), Some("6.1"))).unwrap();
        assert_eq!(r.arch, Arch::Arm64);
        assert_eq!(r.device.as_deref(), Some("jetson-agx-orin"));
        assert_eq!(r.jetpack.as_deref(), Some("6.1"));
        assert_eq!(r.cuda_arch, Some(87));
        assert!(r.use_cuda); // auto-enabled for Jetson
        assert_eq!(r.runtime, DockerRuntime::Nvidia);
        assert!(!r.device_mounts.is_empty());
        assert!(r.base_image.contains("l4t"));
    }

    #[test]
    fn jetson_orin_nx() {
        let r = resolve_platform(&plat("arm64", Some("jetson-orin-nx"), Some("6.0"))).unwrap();
        assert_eq!(r.device.as_deref(), Some("jetson-orin-nx"));
        assert_eq!(r.cuda_arch, Some(87));
        assert!(r.use_cuda);
    }

    #[test]
    fn jetson_orin_nano() {
        let r = resolve_platform(&plat("arm64", Some("jetson-orin-nano"), Some("6.1"))).unwrap();
        assert_eq!(r.device.as_deref(), Some("jetson-orin-nano"));
        assert!(r.use_cuda);
    }

    #[test]
    fn jetson_cuda_explicitly_disabled() {
        let p = Platform {
            arch: "arm64".to_string(),
            cuda: Some(false),
            device: Some("jetson-agx-orin".to_string()),
            jetpack: Some("6.1".to_string()),
        };
        let r = resolve_platform(&p).unwrap();
        assert!(!r.use_cuda);
        assert_eq!(r.runtime, DockerRuntime::Nvidia); // runtime is still nvidia
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

    // -- CUDA variant registry ----------------------------------------------

    #[test]
    fn cuda_variant_components() {
        assert!(has_cuda_variant("sensing-perception"));
        assert!(has_cuda_variant("universe"));
        assert!(!has_cuda_variant("planning-control"));
        assert!(!has_cuda_variant("localization-mapping"));
        assert!(!has_cuda_variant("vehicle-system"));
        assert!(!has_cuda_variant("api"));
    }
}
