//! Modrinth API client.
//!
//! Read-only, unauthenticated endpoints are enough to search, resolve
//! dependencies and download files. User-Agent identifies the launcher, as
//! required by Modrinth's API rules.

pub mod client;
pub mod installed;

pub use client::{
    ModrinthClient, ModrinthProject, ModrinthSearch, ModrinthVersion, SearchHit, SearchQuery,
    VersionDependency, VersionFile,
};
pub use installed::{InstalledMod, InstalledMods};

#[derive(Debug, thiserror::Error)]
pub enum ModrinthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Modrinth returned HTTP {status} for {url}: {body}")]
    Status {
        status: u16,
        url: String,
        body: String,
    },

    #[error("no compatible version of project {0} for Minecraft {1} ({2})")]
    NoCompatibleVersion(String, String, String),

    #[error("invalid response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, ModrinthError>;

pub(crate) fn user_agent() -> String {
    format!(
        "spud-launcher/{} (https://github.com/spud-launcher/spud)",
        env!("CARGO_PKG_VERSION")
    )
}
