//! Local bookkeeping for sync: what this machine last pushed and pulled.
//!
//! Lives next to the other launcher data (`<root>/sync/state.json`) and never
//! travels. Its only job is to answer two UI questions without a network call:
//! "when did I last back this up" and "has another device pushed since then".
//!
//! A corrupt or missing file is treated as "nothing was ever synced" — losing
//! this file costs a timestamp, never a backup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::manifest::SyncManifest;
use crate::{Result, SyncError};

const STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceSyncState {
    pub last_push_at: Option<DateTime<Utc>>,
    pub last_snapshot: Option<String>,
    pub last_pull_at: Option<DateTime<Utc>>,
    /// The remote `updated_at` the user has already seen. A newer value means
    /// another device pushed something.
    pub seen_remote_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default = "schema")]
    pub schema: u32,
    #[serde(default)]
    pub instances: BTreeMap<String, InstanceSyncState>,
}

fn schema() -> u32 {
    STATE_SCHEMA
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            schema: STATE_SCHEMA,
            instances: BTreeMap::new(),
        }
    }
}

impl SyncState {
    pub fn path(root: &Path) -> PathBuf {
        root.join("sync").join("state.json")
    }

    /// Read the state file. A missing or malformed file is an empty state: this
    /// is a convenience cache, not a source of truth.
    pub fn load(root: &Path) -> Self {
        let path = Self::path(root);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = Self::path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SyncError::io(parent, e))?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| SyncError::Json {
            path: path.clone(),
            source,
        })?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, &bytes).map_err(|e| SyncError::io(&temp, e))?;
        std::fs::rename(&temp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            SyncError::io(&path, e)
        })?;
        Ok(())
    }

    pub fn entry(&mut self, instance_id: &str) -> &mut InstanceSyncState {
        self.instances.entry(instance_id.to_string()).or_default()
    }

    /// Record a successful push and the remote state it produced.
    pub fn touch_push(&mut self, instance_id: &str, manifest: &SyncManifest, snapshot: &str) {
        let updated_at = manifest.updated_at;
        let entry = self.entry(instance_id);
        entry.last_push_at = Some(Utc::now());
        entry.last_snapshot = Some(snapshot.to_string());
        entry.seen_remote_updated_at = updated_at;
    }

    pub fn touch_pull(&mut self, instance_id: &str) {
        self.entry(instance_id).last_pull_at = Some(Utc::now());
    }

    /// True when the remote lists something newer than what this machine has
    /// seen — i.e. another device pushed in the meantime.
    pub fn remote_is_newer(&self, instance_id: &str, remote: Option<DateTime<Utc>>) -> bool {
        let Some(remote) = remote else {
            return false;
        };
        match self.instances.get(instance_id).and_then(|entry| entry.seen_remote_updated_at) {
            Some(seen) => remote > seen,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::SyncManifest;

    #[test]
    fn state_round_trips_and_survives_a_missing_file() {
        let dir = tempfile::tempdir().expect("temp");
        assert!(SyncState::load(dir.path()).instances.is_empty());

        let mut state = SyncState::default();
        state.touch_pull("fabric-26-3");
        state.save(dir.path()).expect("save");

        let back = SyncState::load(dir.path());
        assert!(back.instances["fabric-26-3"].last_pull_at.is_some());
    }

    #[test]
    fn a_malformed_state_file_is_treated_as_empty() {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(dir.path().join("sync")).expect("mkdir");
        std::fs::write(SyncState::path(dir.path()), b"{ not json").expect("write");
        assert!(SyncState::load(dir.path()).instances.is_empty());
    }

    #[test]
    fn newer_remote_is_detected_after_a_push() {
        let mut state = SyncState::default();
        let mut manifest = SyncManifest::empty("id");
        manifest.updated_at = Some(Utc::now());

        // Never synced: any remote timestamp counts as new.
        assert!(state.remote_is_newer("id", manifest.updated_at));
        assert!(!state.remote_is_newer("id", None));

        state.touch_push("id", &manifest, "snapshots/a.zip");
        assert!(!state.remote_is_newer("id", manifest.updated_at));

        let later = manifest.updated_at.expect("stamp") + chrono::Duration::seconds(30);
        assert!(state.remote_is_newer("id", Some(later)));
    }
}
