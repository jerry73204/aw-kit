//! Container image references, loaded from `images.toml` at compile time.

use serde::Deserialize;

/// Parsed image configuration.
#[derive(Debug, Deserialize)]
pub struct ImageConfig {
    pub upstream: Upstream,
    pub devel: Devel,
    pub base: Base,
}

#[derive(Debug, Deserialize)]
pub struct Upstream {
    /// Registry + image name for pre-built component images.
    pub image: String,
}

#[derive(Debug, Deserialize)]
pub struct Devel {
    /// Build-stage image (non-CUDA).
    pub image: String,
    /// Build-stage image (CUDA).
    pub image_cuda: String,
}

#[derive(Debug, Deserialize)]
pub struct Base {
    /// Default base image for desktop platforms.
    pub desktop: String,
    /// Base image for Jetson devices.
    pub jetson: String,
}

/// Load the image config. Panics on parse error (caught at first use).
pub fn load() -> ImageConfig {
    const TOML_SRC: &str = include_str!("../images.toml");
    toml::from_str(TOML_SRC).expect("failed to parse images.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_toml_parses() {
        let cfg = load();
        assert!(!cfg.upstream.image.is_empty());
        assert!(!cfg.devel.image.is_empty());
        assert!(!cfg.devel.image_cuda.is_empty());
        assert!(!cfg.base.desktop.is_empty());
        assert!(!cfg.base.jetson.is_empty());
    }
}
