//! Microsoft account flow and token storage.
//!
//! Device code flow is used instead of a redirect URI: the launcher never sees
//! the user's password, and the app does not need a loopback HTTP server.

pub mod microsoft;
pub mod store;

pub use microsoft::{
    DeviceCode, LoginResult, MicrosoftAuth, MinecraftProfile, StoredTokens,
};
pub use store::{
    offline_uuid, validate_offline_name, AccountKind, AccountRecord, AccountStore, AccountsFile,
    TokenVault,
};

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Microsoft client id is not configured — paste an Azure application (client) ID in Settings → Microsoft sign-in, or set SPUD_MS_CLIENT_ID")]
    MissingClientId,

    #[error("device login failed: {code} — {message}")]
    DeviceFlow { code: String, message: String },

    #[error("this Microsoft account owns no Minecraft profile")]
    NoGameOwnership,

    #[error("invalid offline nickname `{0}` — 3 to 16 characters, letters, digits and underscore")]
    OfflineName(String),

    #[error("Xbox authentication failed ({code}): {message}")]
    Xbox { code: String, message: String },

    #[error("no refresh token is stored for this account")]
    NoRefreshToken,

    #[error("secure storage error: {0}")]
    Keyring(String),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid response: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Core(#[from] launcher_core::CoreError),
}

pub type Result<T> = std::result::Result<T, AuthError>;

pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::result::Result<T, std::io::Error> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|source| AuthError::Io {
            path: path.into(),
            source,
        })
    }
}
