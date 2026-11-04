//! `launcher-core` — the domain layer.
//!
//! This crate knows nothing about HTTP, Tauri, keyrings or the UI. It owns:
//! * filesystem layout ([`paths`]),
//! * instance configuration ([`instance`]),
//! * Java discovery ([`java`]),
//! * Mojang version metadata and rule evaluation ([`version`]),
//! * construction of the JVM command line ([`launch`]).
//!
//! Everything else in the workspace depends on these types, never the reverse.

pub mod error;
pub mod hardware;
pub mod instance;
pub mod java;
pub mod launch;
pub mod modstore;
pub mod paths;
pub mod plugin;
pub mod settings;
pub mod version;

pub use error::{CoreError, Result};
pub use instance::{
    GameSpec, InstanceConfig, JavaMode, JavaSettings, LoaderKind, NewInstance, TuningState,
    WindowSettings,
};
pub use paths::LauncherPaths;
pub use settings::{AiProvider, LauncherSettings, SyncBackend, Theme};
