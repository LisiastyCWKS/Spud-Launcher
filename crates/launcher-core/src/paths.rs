use std::path::{Path, PathBuf};

use crate::error::{CoreError, IoContext, Result};

pub const APP_QUALIFIER: &str = "dev";
pub const APP_ORGANISATION: &str = "spud";
pub const APP_NAME: &str = "SpudLauncher";

/// Layout on disk:
///
/// ```text
/// <root>/
/// ├── settings.json
/// ├── accounts.json            # public metadata; tokens live in the OS keyring
/// ├── instances/
/// │   └── <slug>/
/// │       ├── instance.json
/// │       └── .minecraft/      # isolated game directory
/// ├── meta/
/// │   ├── version_manifest_v2.json
/// │   └── versions/<id>/<id>.json
/// ├── libraries/               # shared, immutable jars
/// ├── assets/
/// │   ├── indexes/
/// │   ├── objects/             # content-addressed by SHA-1
/// │   └── virtual/             # legacy pre-1.7 asset layout
/// └── cache/
///     └── downloads/           # resumable partial downloads
/// ```
#[derive(Debug, Clone)]
pub struct LauncherPaths {
    root: PathBuf,
}

impl LauncherPaths {
    /// Resolve the data directory. A `portable.txt` next to the executable
    /// switches the launcher into portable mode (data beside the binary).
    pub fn discover() -> Result<Self> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("portable.txt").exists() {
                    return Self::at(dir.join("data"));
                }
            }
        }

        let dirs = directories::ProjectDirs::from(APP_QUALIFIER, APP_ORGANISATION, APP_NAME)
            .ok_or(CoreError::NoHomeDir)?;
        Self::at(dirs.data_dir().to_path_buf())
    }

    /// Explicit root, primarily for tests and portable installs.
    pub fn at(root: impl Into<PathBuf>) -> Result<Self> {
        let paths = Self { root: root.into() };
        paths.ensure_layout()?;
        Ok(paths)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances_dir().join(id)
    }

    pub fn instance_config(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("instance.json")
    }

    /// Isolated `.minecraft` for one instance.
    pub fn game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join(".minecraft")
    }

    pub fn instance_mods_dir(&self, id: &str) -> PathBuf {
        self.game_dir(id).join("mods")
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.root.join("meta")
    }

    pub fn version_json(&self, version_id: &str) -> PathBuf {
        self.meta_dir()
            .join("versions")
            .join(version_id)
            .join(format!("{version_id}.json"))
    }

    /// Vanilla client jar. Loader versions reference the vanilla jar, never
    /// their own, so this is always keyed by the Minecraft version.
    pub fn version_jar(&self, version_id: &str) -> PathBuf {
        self.meta_dir()
            .join("versions")
            .join(version_id)
            .join(format!("{version_id}.jar"))
    }

    pub fn log_configs_dir(&self) -> PathBuf {
        self.assets_dir().join("log_configs")
    }

    /// Shared jars, addressed by their Maven path (`net/fabricmc/...`).
    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn asset_objects_dir(&self) -> PathBuf {
        self.assets_dir().join("objects")
    }

    pub fn asset_indexes_dir(&self) -> PathBuf {
        self.assets_dir().join("indexes")
    }

    pub fn asset_virtual_dir(&self, index: &str) -> PathBuf {
        self.assets_dir().join("virtual").join(index)
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Content-addressed mod store shared by every instance.
    pub fn modstore_dir(&self) -> PathBuf {
        self.root.join("modstore")
    }

    /// Installed plugins, one directory per plugin.
    pub fn plugins_dir(&self) -> PathBuf {
        self.root.join("plugins")
    }

    /// Optional user-supplied background images for the UI.
    pub fn backgrounds_dir(&self) -> PathBuf {
        self.root.join("backgrounds")
    }

    /// Partial downloads live here until their hash verifies.
    pub fn download_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("downloads")
    }

    pub fn instance_natives_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("natives")
    }

    pub fn instance_logs_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("logs")
    }

    fn ensure_layout(&self) -> Result<()> {
        for dir in [
            self.instances_dir(),
            self.meta_dir().join("versions"),
            self.libraries_dir(),
            self.asset_objects_dir(),
            self.asset_indexes_dir(),
            self.download_cache_dir(),
            self.modstore_dir(),
            self.plugins_dir(),
            self.backgrounds_dir(),
        ] {
            std::fs::create_dir_all(&dir).at(&dir)?;
        }
        Ok(())
    }
}

/// Filesystem/i18n-safe instance id: lowercase ASCII, `[a-z0-9-]`.
/// Never trust a display name as a directory name.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = true; // suppress a leading dash

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "instance".to_string()
    } else {
        out.truncate(48);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugs_are_portable() {
        assert_eq!(slugify("Mój Nowy Świat!"), "m-j-nowy-wiat");
        assert_eq!(slugify("  fabric/1.20.1  "), "fabric-1-20-1");
        assert_eq!(slugify("***"), "instance");
    }
}
