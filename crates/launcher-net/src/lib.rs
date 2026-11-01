//! `launcher-net` — all outbound I/O for the launcher.
//!
//! The crate is deliberately independent of Tauri: progress is reported over
//! an `mpsc` channel, so the same installer can be driven from a CLI or a test.

pub mod cache;
pub mod download;
pub mod install;
pub mod loaders;
pub mod mrpack;

pub use download::{
    DownloadControl, DownloadEvent, DownloadManager, DownloadSpec, DownloadSummary, ProgressTracker,
};
pub use install::{GameInstaller, InstallOutcome};
pub use loaders::{LoaderBuild, LoaderService};

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unexpected HTTP {status} for {url}")]
    HttpStatus { status: u16, url: String },

    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("download cancelled")]
    Cancelled,

    #[error("{0} download(s) failed: {1}")]
    Partial(usize, String),

    #[error("invalid metadata: {0}")]
    Manifest(String),

    #[error("unsupported archive: {0}")]
    Archive(String),

    #[error("background task failed: {0}")]
    Join(String),

    #[error("process failed: {0}")]
    Process(String),

    #[error(transparent)]
    Core(#[from] launcher_core::CoreError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, NetError>;

pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::result::Result<T, std::io::Error> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| NetError::Io {
            path: path.into(),
            source,
        })
    }
}

pub(crate) fn user_agent() -> String {
    format!(
        "spud-launcher/{} (https://github.com/spud-launcher/spud)",
        env!("CARGO_PKG_VERSION")
    )
}
