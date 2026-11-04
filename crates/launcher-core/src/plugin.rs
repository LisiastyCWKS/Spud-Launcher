//! Plugin discovery and activation state.
//!
//! A plugin is a folder under `<root>/plugins/` with a `plugin.json` manifest
//! and one JavaScript entry file:
//!
//! ```text
//! plugins/
//! ├── state.json                 # which plugins the user turned on
//! └── hello-spud/
//!     ├── plugin.json
//!     └── main.js
//! ```
//!
//! Scripts run in the launcher's own webview, not in a native dylib. That is a
//! deliberate choice, not a shortcut:
//!
//! * Rust `cdylib` plugins have no stable ABI across compiler versions, so every
//!   launcher update would break every plugin (or ship a C ABI and a hand-built
//!   vtable — a much larger surface than the whole plugin system).
//! * A panicking native plugin takes the launcher down with it. A JS exception
//!   is caught by the host and reported against the plugin that threw it, which
//!   is exactly the "error isolation" this feature is supposed to provide.
//! * The game, Modrinth and the filesystem are already reachable through the
//!   curated bridge; a plugin never needs raw process access.
//!
//! The trade-off is honest and documented: plugin code is trusted once enabled,
//! because it shares the webview with the launcher UI. Plugins are therefore
//! **disabled by default** — dropping a folder in is never enough to run it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{read_json, write_json_atomic, Result};
use crate::paths::LauncherPaths;

/// Bridge version implemented by this launcher build. A manifest asking for a
/// different major version is listed but refuses to load, rather than silently
/// running against an API it was not written for.
pub const PLUGIN_API: u32 = 1;

/// Hard ceiling on the entry script. A "plugin" larger than this is not a
/// plugin; refusing early keeps a stray binary out of the webview.
pub const MAX_SOURCE_BYTES: u64 = 512 * 1024;

const STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Major bridge version the plugin was written against.
    #[serde(default = "default_api")]
    pub api: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Entry file relative to the plugin folder. Must stay inside it.
    #[serde(default = "default_main")]
    pub main: String,
    /// Events the plugin intends to subscribe to. Informational: the bridge
    /// delivers whatever the host emits, but this is what the settings UI shows
    /// before the user enables something.
    #[serde(default)]
    pub hooks: Vec<String>,
}

fn default_api() -> u32 {
    PLUGIN_API
}

fn default_main() -> String {
    "main.js".to_string()
}

impl PluginManifest {
    /// The id doubles as a folder name and as the namespace for the plugin's
    /// storage, so it may only contain characters that are safe in both.
    pub fn valid_id(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginState {
    #[serde(default = "schema")]
    pub schema: u32,
    /// Explicitly enabled ids. Anything absent is off — a plugin has to be
    /// turned on, it is never on because it happens to be on disk.
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,
}

fn schema() -> u32 {
    STATE_SCHEMA
}

impl PluginState {
    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.get(id).copied().unwrap_or(false)
    }
}

/// One row in the plugins list. `source` is filled only for plugins that can
/// actually run, so a broken or disabled plugin never carries executable text
/// into the webview.
#[derive(Debug, Clone, Serialize)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    pub description: String,
    pub author: String,
    pub main: String,
    pub hooks: Vec<String>,
    pub enabled: bool,
    /// Set when the folder cannot be loaded: bad manifest, bad id, escaping
    /// path, oversized script, incompatible api version.
    pub error: Option<String>,
    pub source: Option<String>,
    pub dir: String,
}

pub struct PluginHost {
    root: PathBuf,
}

impl PluginHost {
    pub fn new(paths: &LauncherPaths) -> Self {
        Self {
            root: paths.plugins_dir(),
        }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    pub fn load_state(&self) -> Result<PluginState> {
        let path = self.state_path();
        if !path.is_file() {
            return Ok(PluginState::default());
        }
        let mut state: PluginState = read_json(&path)?;
        state.schema = STATE_SCHEMA;
        Ok(state)
    }

    fn save_state(&self, state: &PluginState) -> Result<()> {
        write_json_atomic(&self.state_path(), state)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let mut state = self.load_state()?;
        state.enabled.insert(id.to_string(), enabled);
        self.save_state(&state)
    }

    /// Every plugin folder, sorted by id. Unreadable folders are skipped rather
    /// than reported: a stray `.DS_Store` is not a user-facing error.
    pub fn list(&self) -> Vec<PluginEntry> {
        let Ok(state) = self.load_state() else {
            return Vec::new();
        };
        let Ok(dirs) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };

        let mut entries: Vec<PluginEntry> = dirs
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| self.read_one(&entry.path(), &state))
            .collect();
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        entries
    }

    fn read_one(&self, dir: &Path, state: &PluginState) -> Option<PluginEntry> {
        let folder = dir.file_name()?.to_string_lossy().to_string();
        let manifest_path = dir.join("plugin.json");
        if !manifest_path.is_file() {
            return None;
        }
        let enabled = state.is_enabled(&folder);
        let fallback = PluginEntry {
            id: folder.clone(),
            name: folder.clone(),
            version: "0.0.0".to_string(),
            api: PLUGIN_API,
            description: String::new(),
            author: String::new(),
            main: default_main(),
            hooks: Vec::new(),
            enabled: false,
            error: None,
            source: None,
            dir: dir.display().to_string(),
        };

        let manifest: PluginManifest = match read_json(&manifest_path) {
            Ok(manifest) => manifest,
            Err(error) => {
                return Some(PluginEntry {
                    error: Some(format!("plugin.json is not valid: {error}")),
                    ..fallback
                })
            }
        };

        let mut entry = PluginEntry {
            id: manifest.id.clone(),
            name: if manifest.name.trim().is_empty() {
                folder.clone()
            } else {
                manifest.name.clone()
            },
            version: manifest.version.clone(),
            api: manifest.api,
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            main: manifest.main.clone(),
            hooks: manifest.hooks.clone(),
            enabled,
            error: None,
            source: None,
            dir: dir.display().to_string(),
        };

        if let Some(error) = validate(&manifest, dir) {
            entry.error = Some(error);
            return Some(entry);
        }
        // The folder name is the namespace the state file and the plugin's own
        // storage key off, so a manifest that disagrees with it is a mistake
        // worth surfacing instead of silently running under another name.
        if manifest.id != folder {
            entry.error = Some(format!(
                "manifest id \"{}\" does not match the folder name \"{folder}\"",
                manifest.id
            ));
            return Some(entry);
        }

        if enabled {
            match std::fs::read_to_string(dir.join(&manifest.main)) {
                Ok(source) => {
                    if source.len() as u64 > MAX_SOURCE_BYTES {
                        entry.error = Some(format!(
                            "{} is larger than the {} KiB plugin limit",
                            manifest.main,
                            MAX_SOURCE_BYTES / 1024
                        ));
                    } else {
                        entry.source = Some(source);
                    }
                }
                Err(error) => entry.error = Some(format!("cannot read {}: {error}", manifest.main)),
            }
        }

        Some(entry)
    }
}

/// Everything that must hold before a plugin's code is handed to the webview.
fn validate(manifest: &PluginManifest, dir: &Path) -> Option<String> {
    if !manifest.valid_id() {
        return Some(
            "id must be 1–64 characters of a-z, 0-9, '-' or '_' and match the folder name"
                .to_string(),
        );
    }
    if manifest.version.trim().is_empty() {
        return Some("version is required".to_string());
    }
    if manifest.api != PLUGIN_API {
        return Some(format!(
            "written for plugin API {}, this launcher speaks API {PLUGIN_API}",
            manifest.api
        ));
    }
    let entry = Path::new(&manifest.main);
    if entry.is_absolute() || entry.components().count() != 1 {
        return Some("main must be a single file name inside the plugin folder".to_string());
    }
    if !dir.join(entry).is_file() {
        return Some(format!("{} is missing", manifest.main));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("spud-plugins-{name}-{}", std::process::id()));
        if root.exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_plugin(root: &Path, id: &str, manifest: &str, source: &str) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let mut file = std::fs::File::create(dir.join("plugin.json")).unwrap();
        file.write_all(manifest.as_bytes()).unwrap();
        std::fs::write(dir.join("main.js"), source).unwrap();
    }

    const GOOD: &str = r#"{
        "id": "hello-spud",
        "name": "Hello Spud",
        "version": "1.0.0",
        "api": 1,
        "description": "demo",
        "author": "you",
        "main": "main.js"
    }"#;

    #[test]
    fn plugins_are_disabled_until_enabled() {
        let root = temp_root("disabled");
        write_plugin(&root, "hello-spud", GOOD, "Spud.log('hi');");
        let host = PluginHost::at(&root);

        let list = host.list();
        assert_eq!(list.len(), 1);
        assert!(!list[0].enabled);
        // Disabled plugins must not carry their source into the webview.
        assert!(list[0].source.is_none());
        assert!(list[0].error.is_none());

        host.set_enabled("hello-spud", true).unwrap();
        let list = host.list();
        assert!(list[0].enabled);
        assert_eq!(list[0].source.as_deref(), Some("Spud.log('hi');"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_traversal_in_main_is_rejected() {
        let root = temp_root("escape");
        let manifest = r#"{
            "id": "evil",
            "name": "Evil",
            "version": "1.0.0",
            "api": 1,
            "main": "../../etc/passwd"
        }"#;
        write_plugin(&root, "evil", manifest, "// noop");
        let list = PluginHost::at(&root).list();
        assert_eq!(list.len(), 1);
        assert!(list[0].error.is_some());
        assert!(list[0].source.is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn api_version_mismatch_is_reported_not_loaded() {
        let root = temp_root("api");
        let manifest = r#"{
            "id": "future",
            "name": "Future",
            "version": "2.0.0",
            "api": 99,
            "main": "main.js"
        }"#;
        write_plugin(&root, "future", manifest, "// noop");
        let host = PluginHost::at(&root);
        host.set_enabled("future", true).unwrap();

        let list = host.list();
        assert!(list[0].error.as_deref().unwrap().contains("API 99"));
        assert!(list[0].source.is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn id_must_match_the_folder() {
        let root = temp_root("mismatch");
        let manifest = r#"{
            "id": "other-name",
            "name": "Other",
            "version": "1.0.0",
            "api": 1,
            "main": "main.js"
        }"#;
        write_plugin(&root, "folder-name", manifest, "// noop");
        let list = PluginHost::at(&root).list();
        assert!(list[0]
            .error
            .as_deref()
            .unwrap()
            .contains("does not match"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
