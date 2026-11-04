use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{read_json, write_json_atomic, CoreError, IoContext, Result};
use crate::paths::{slugify, LauncherPaths};

/// Current `instance.json` schema version. Bump only with a migration.
pub const INSTANCE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    pub fn is_modded(self) -> bool {
        !matches!(self, Self::Vanilla)
    }
}

impl std::fmt::Display for LoaderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSpec {
    /// Vanilla version, e.g. `1.21.1`.
    pub version: String,
    #[serde(default = "vanilla")]
    pub loader: LoaderKind,
    /// Loader build (`0.16.5`, `21.1.80`, ...). `None` for Vanilla.
    #[serde(default)]
    pub loader_version: Option<String>,
    /// Fully resolved version JSON id (e.g. `fabric-loader-0.16.5-1.21.1`).
    /// Filled by the installer; the launch planner refuses to run without it.
    #[serde(default)]
    pub version_id: Option<String>,
}

fn vanilla() -> LoaderKind {
    LoaderKind::Vanilla
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JavaMode {
    /// Pick the newest detected runtime that satisfies the version requirement.
    Auto,
    /// Use `path` verbatim.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaSettings {
    #[serde(default = "auto_java")]
    pub mode: JavaMode,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default = "default_min_memory")]
    pub min_memory_mb: u32,
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: u32,
    /// Appended after the launcher-provided JVM flags, before `-cp`.
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn auto_java() -> JavaMode {
    JavaMode::Auto
}
fn default_min_memory() -> u32 {
    512
}
fn default_max_memory() -> u32 {
    3072
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self {
            mode: JavaMode::Auto,
            path: None,
            min_memory_mb: default_min_memory(),
            max_memory_mb: default_max_memory(),
            extra_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSettings {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub fullscreen: bool,
}

fn default_width() -> u32 {
    1280
}
fn default_height() -> u32 {
    720
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            fullscreen: false,
        }
    }
}

/// Record of the last automatic hardware tuning, so the UI can say "tuned for
/// this machine on <date>" and can offer a re-run when the machine changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningState {
    pub applied_at: DateTime<Utc>,
    pub cpu_threads: u32,
    pub memory_total_mb: u64,
    pub modded: bool,
    pub java_major: u32,
    pub max_memory_mb: u32,
}

/// One isolated game installation. Serialized as `<instance>/instance.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    #[serde(default = "schema_version")]
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_played_at: Option<DateTime<Utc>>,
    pub game: GameSpec,
    #[serde(default)]
    pub java: JavaSettings,
    #[serde(default)]
    pub window: WindowSettings,
    /// Raw JVM flags for this instance only (e.g. `-XX:+UseG1GC`).
    #[serde(default)]
    pub jvm_args: Vec<String>,
    /// Extra arguments appended after the vanilla ones.
    #[serde(default)]
    pub game_args: Vec<String>,
    /// Environment variables passed to the game process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub notes: String,
    /// Filled by the hardware tuner; absent for instances tuned by hand or
    /// created before the tuner existed. Additive, so schema 1 files still load.
    #[serde(default)]
    pub tuning: Option<TuningState>,
}

fn schema_version() -> u32 {
    INSTANCE_SCHEMA
}

/// Input for instance creation, mirrored by the create dialog.
#[derive(Debug, Clone, Deserialize)]
pub struct NewInstance {
    pub name: String,
    pub mc_version: String,
    #[serde(default = "vanilla")]
    pub loader: LoaderKind,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
}

/// Compact representation sent to the UI list (avoids shipping every config field).
#[derive(Debug, Clone, Serialize)]
pub struct InstanceSummary {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub icon: Option<String>,
    pub last_played_at: Option<DateTime<Utc>>,
    pub max_memory_mb: u32,
    pub path: PathBuf,
    pub ready: bool,
}

impl InstanceConfig {
    /// Create the directory tree and write `instance.json`. The instance id is
    /// derived from the name and de-duplicated, never user-controlled.
    pub fn create(paths: &LauncherPaths, draft: NewInstance) -> Result<Self> {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(CoreError::InvalidId("empty instance name".into()));
        }

        let id = unique_id(paths, &slugify(name));
        let dir = paths.instance_dir(&id);
        std::fs::create_dir_all(&dir).at(&dir)?;

        for sub in [
            "mods",
            "resourcepacks",
            "shaderpacks",
            "saves",
            "config",
            "logs",
            "screenshots",
        ] {
            let p = paths.game_dir(&id).join(sub);
            std::fs::create_dir_all(&p).at(&p)?;
        }

        let now = Utc::now();
        let config = Self {
            schema: INSTANCE_SCHEMA,
            id: id.clone(),
            name: name.to_string(),
            icon: None,
            created_at: now,
            updated_at: now,
            last_played_at: None,
            game: GameSpec {
                version: draft.mc_version,
                loader: draft.loader,
                loader_version: draft.loader_version,
                version_id: None,
            },
            java: JavaSettings {
                max_memory_mb: draft.max_memory_mb.unwrap_or_else(default_max_memory),
                ..JavaSettings::default()
            },
            window: WindowSettings::default(),
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            env: BTreeMap::new(),
            notes: String::new(),
            tuning: None,
        };

        config.save(paths)?;
        Ok(config)
    }

    pub fn load(paths: &LauncherPaths, id: &str) -> Result<Self> {
        let path = paths.instance_config(id);
        if !path.exists() {
            return Err(CoreError::InstanceNotFound(id.to_string()));
        }
        let mut config: Self = read_json(&path)?;
        if config.id != id {
            return Err(CoreError::InvalidId(format!(
                "instance.json id `{}` does not match directory `{id}`",
                config.id
            )));
        }
        // Tolerate hand-edited files from older builds.
        if config.schema > INSTANCE_SCHEMA {
            return Err(CoreError::InvalidVersion(format!(
                "instance schema {} is newer than supported {INSTANCE_SCHEMA}",
                config.schema
            )));
        }
        config.schema = INSTANCE_SCHEMA;
        Ok(config)
    }

    pub fn save(&self, paths: &LauncherPaths) -> Result<()> {
        write_json_atomic(&paths.instance_config(&self.id), self)
    }

    /// Persist a mutated config and refresh `updated_at`.
    pub fn save_touched(&mut self, paths: &LauncherPaths) -> Result<()> {
        self.updated_at = Utc::now();
        self.save(paths)
    }

    pub fn summary(&self, paths: &LauncherPaths) -> InstanceSummary {
        InstanceSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            mc_version: self.game.version.clone(),
            loader: self.game.loader,
            loader_version: self.game.loader_version.clone(),
            icon: self.icon.clone(),
            last_played_at: self.last_played_at,
            max_memory_mb: self.java.max_memory_mb,
            path: paths.game_dir(&self.id),
            ready: self.game.version_id.is_some(),
        }
    }

    /// Delete with files, or just hide the config? Callers must ask the user.
    pub fn delete(self, paths: &LauncherPaths) -> Result<()> {
        let dir = paths.instance_dir(&self.id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).at(&dir)?;
        }
        Ok(())
    }
}

/// List instances, sorted by last played (never played last).
pub fn list_instances(paths: &LauncherPaths) -> Result<Vec<InstanceSummary>> {
    let mut out = Vec::new();
    let dir = paths.instances_dir();
    std::fs::create_dir_all(&dir).at(&dir)?;

    for entry in std::fs::read_dir(&dir).at(&dir)? {
        let entry = entry.at(&dir)?;
        if !entry.file_type().at(entry.path())?.is_dir() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        match InstanceConfig::load(paths, &id) {
            Ok(config) => out.push(config.summary(paths)),
            Err(err) => tracing::warn!(%id, %err, "skipping unreadable instance"),
        }
    }

    out.sort_by(|a, b| b.last_played_at.cmp(&a.last_played_at));
    Ok(out)
}

fn unique_id(paths: &LauncherPaths, base: &str) -> String {
    let mut candidate = base.to_string();
    let mut n = 2;
    while paths.instance_dir(&candidate).exists() {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

/// Validate that an id cannot escape the instances directory.
pub fn validate_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(CoreError::InvalidId(id.to_string()))
    }
}

pub fn instance_path(paths: &LauncherPaths, id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    let dir = paths.instance_dir(id);
    if !dir.exists() {
        return Err(CoreError::InstanceNotFound(id.to_string()));
    }
    Ok(dir)
}

/// Path to a file inside the instance's isolated game dir, refusing traversal.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = base.join(relative);
    let normalized = candidate
        .components()
        .fold(PathBuf::new(), |mut acc, c| match c {
            std::path::Component::ParentDir => {
                acc.pop();
                acc
            }
            other => {
                acc.push(other);
                acc
            }
        });
    if !normalized.starts_with(base) {
        return Err(CoreError::InvalidId(relative.to_string()));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cannot_traverse() {
        assert!(validate_id("fabric-1-20-1").is_ok());
        assert!(validate_id("../escape").is_err());
        assert!(validate_id("Upper").is_err());
    }

    #[test]
    fn safe_join_rejects_parent_dirs() {
        let base = Path::new("/data/instances/demo/.minecraft");
        assert!(safe_join(base, "../../settings.json").is_err());
        assert!(safe_join(base, "mods/ok.jar").is_ok());
    }
}
