use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ModrinthError, Result};

/// Manifest of files installed by the launcher into one instance.
/// Stored at `<instance>/.minecraft/.spud/mods.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMods {
    #[serde(default = "schema")]
    pub schema: u32,
    #[serde(default)]
    pub mods: Vec<InstalledMod>,
}

fn schema() -> u32 {
    1
}

impl Default for InstalledMods {
    fn default() -> Self {
        Self {
            schema: schema(),
            mods: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub project_id: String,
    pub version_id: String,
    pub project_title: String,
    pub version_number: String,
    pub file_name: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: u64,
    /// `mod`, `resourcepack`, `shader`, `modpack`.
    #[serde(default)]
    pub project_type: String,
    pub installed_at: DateTime<Utc>,
}

impl InstalledMods {
    pub fn load(game_dir: &Path) -> Result<Self> {
        let path = manifest_path(game_dir);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let raw = std::fs::read(&path).map_err(|source| ModrinthError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(serde_json::from_slice(&raw)?)
    }

    pub fn save(&self, game_dir: &Path) -> Result<()> {
        let path = manifest_path(game_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ModrinthError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(&path, bytes).map_err(|source| ModrinthError::Io {
            path: path.clone(),
            source,
        })
    }

    pub fn upsert(&mut self, entry: InstalledMod) {
        match self
            .mods
            .iter_mut()
            .find(|m| m.project_id == entry.project_id)
        {
            Some(existing) => *existing = entry,
            None => self.mods.push(entry),
        }
    }

    pub fn remove(&mut self, project_id: &str) -> Option<InstalledMod> {
        let index = self.mods.iter().position(|m| m.project_id == project_id)?;
        Some(self.mods.remove(index))
    }
}

pub fn manifest_path(game_dir: &Path) -> PathBuf {
    game_dir.join(".spud").join("mods.json")
}

pub fn target_dir(game_dir: &Path, project_type: &str) -> PathBuf {
    match project_type {
        "resourcepack" => game_dir.join("resourcepacks"),
        "shader" => game_dir.join("shaderpacks"),
        _ => game_dir.join("mods"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_same_project() {
        let mut mods = InstalledMods::default();
        let entry = |version: &str| InstalledMod {
            project_id: "p".into(),
            version_id: version.into(),
            project_title: "Test".into(),
            version_number: version.into(),
            file_name: "test.jar".into(),
            sha1: None,
            size: 0,
            project_type: "mod".into(),
            installed_at: Utc::now(),
        };
        mods.upsert(entry("1"));
        mods.upsert(entry("2"));
        assert_eq!(mods.mods.len(), 1);
        assert_eq!(mods.mods[0].version_id, "2");
    }
}
