use std::path::PathBuf;

/// Errors raised by the domain layer. Every variant names the thing that
/// failed, because the UI surfaces these verbatim.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid instance id: {0}")]
    InvalidId(String),

    #[error("instance not found: {0}")]
    InstanceNotFound(String),

    #[error("instance already exists: {0}")]
    InstanceExists(String),

    #[error("no Java runtime selected or detected")]
    JavaMissing,

    #[error("cannot resolve the user home directory")]
    NoHomeDir,

    #[error("Java {required}+ is required, selected runtime is Java {actual}")]
    JavaTooOld { required: u32, actual: u32 },

    #[error("invalid version JSON: {0}")]
    InvalidVersion(String),

    #[error("cannot parse Maven coordinate: {0}")]
    Maven(String),

    #[error("archive error: {0}")]
    Archive(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

/// Attach the path to std I/O errors so the UI can point at the exact file.
pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::result::Result<T, std::io::Error> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| CoreError::Io {
            path: path.into(),
            source,
        })
    }
}

/// Read JSON with the path preserved in the error.
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let raw = std::fs::read(path).at(path)?;
    serde_json::from_slice(&raw).map_err(|source| CoreError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Write JSON atomically: temp file in the same directory, then rename.
/// A crash mid-write leaves the previous config intact.
pub(crate) fn write_json_atomic<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidId(path.display().to_string()))?;
    std::fs::create_dir_all(parent).at(parent)?;

    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config")
    ));
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| CoreError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    std::fs::write(&tmp, &bytes).at(&tmp)?;
    std::fs::rename(&tmp, path).at(path)?;
    Ok(())
}
