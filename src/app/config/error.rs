use std::path::PathBuf;

/// Why `config.toml` could not be saved.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file exists but is not TOML; it is left untouched.
    #[error("{} is not valid TOML, fix it first (not overwritten): {source}", .path.display())]
    NotToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("cannot read {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Serialize(#[from] toml::ser::Error),
    /// Creating the directory, writing the staging file or renaming it failed.
    #[error(transparent)]
    Write(#[from] std::io::Error),
}
