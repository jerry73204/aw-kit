use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read manifest at {path}: {source}")]
    ManifestRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse manifest at {path}: {source}")]
    ManifestParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("validation error: {0}")]
    Validation(String),

    #[error("platform error: {0}")]
    Platform(String),

    #[error("codegen error: {0}")]
    Codegen(String),

    #[error("build error: {0}")]
    Build(String),

    #[error("I/O error: {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
