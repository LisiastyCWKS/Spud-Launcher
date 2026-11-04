//! The per-instance manifest.
//!
//! One JSON file per instance on the remote; it is the only index of what is
//! stored, so it is written last and written atomically. A snapshot that
//! uploads successfully but is not yet listed is invisible to `pull`, which is
//! the safe direction to fail in.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::provider::SyncProvider;
use crate::{instance_prefix, Result, SyncError};

/// Bumped only when the on-disk shape changes incompatibly. A remote written by
/// a *newer* launcher is refused rather than guessed at.
pub const SCHEMA: u32 = 1;

pub const MANIFEST_NAME: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    #[serde(default = "schema")]
    pub schema: u32,
    #[serde(default)]
    pub instance_id: String,
    /// Display name at push time; the local `instance.json` stays authoritative.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub snapshots: Vec<Snapshot>,
}

fn schema() -> u32 {
    SCHEMA
}

impl SyncManifest {
    pub fn empty(instance_id: &str) -> Self {
        Self {
            schema: SCHEMA,
            instance_id: instance_id.to_string(),
            name: String::new(),
            updated_at: None,
            snapshots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Relative to the instance prefix, e.g. `snapshots/20261104-183012-1f3c9ab2.zip`.
    pub file: String,
    pub sha256: String,
    pub bytes: u64,
    pub created_at: DateTime<Utc>,
    /// Device label, so a second machine knows where a backup came from.
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub mc_version: String,
    #[serde(default)]
    pub loader: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub note: String,
}

/// Caller-supplied half of a snapshot: everything except the hash and size,
/// which come from the file itself.
#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub name: String,
    pub origin: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub note: String,
}

fn manifest_key(instance_id: &str) -> String {
    format!("{}/{MANIFEST_NAME}", instance_prefix(instance_id))
}

pub fn load(provider: &dyn SyncProvider, instance_id: &str) -> Result<SyncManifest> {
    let key = manifest_key(instance_id);
    let Some(bytes) = provider.read(&key)? else {
        return Ok(SyncManifest::empty(instance_id));
    };
    let manifest: SyncManifest = serde_json::from_slice(&bytes).map_err(|source| SyncError::Json {
        path: std::path::PathBuf::from(&key),
        source,
    })?;
    if manifest.schema > SCHEMA {
        return Err(SyncError::UnsupportedSchema(manifest.schema));
    }
    Ok(manifest)
}

pub fn save(provider: &dyn SyncProvider, instance_id: &str, manifest: &SyncManifest) -> Result<()> {
    let key = manifest_key(instance_id);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|source| SyncError::Json {
        path: std::path::PathBuf::from(&key),
        source,
    })?;
    provider.write(&key, &bytes)
}

/// Trim the manifest to `keep` snapshots, deleting the older archives.
///
/// Returns the files that were removed. `keep` is floored at 1: a manifest with
/// zero snapshots is what `remove_snapshot` is for, not retention.
pub fn retention(
    manifest: &mut SyncManifest,
    provider: &dyn SyncProvider,
    instance_id: &str,
    keep: usize,
) -> Result<Vec<String>> {
    let keep = keep.max(1);
    let mut removed = Vec::new();
    while manifest.snapshots.len() > keep {
        let snapshot = manifest.snapshots.pop().expect("non-empty");
        let key = format!("{}/{file}", instance_prefix(instance_id), file = snapshot.file);
        provider.remove(&key)?;
        removed.push(snapshot.file);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::FolderProvider;

    fn provider() -> (tempfile::TempDir, FolderProvider) {
        let dir = tempfile::tempdir().expect("temp");
        let provider = FolderProvider::new(dir.path().join("remote")).expect("provider");
        (dir, provider)
    }

    #[test]
    fn a_missing_manifest_is_an_empty_one() {
        let (_dir, provider) = provider();
        let manifest = load(&provider, "whatever").expect("load");
        assert!(manifest.snapshots.is_empty());
        assert!(manifest.updated_at.is_none());
        assert_eq!(manifest.instance_id, "whatever");
    }

    #[test]
    fn manifests_round_trip() {
        let (_dir, provider) = provider();
        let mut manifest = SyncManifest::empty("id");
        manifest.name = "Fabric 26.3".into();
        manifest.updated_at = Some(Utc::now());
        manifest.snapshots.push(Snapshot {
            file: "snapshots/x.zip".into(),
            sha256: "ab".repeat(32),
            bytes: 12,
            created_at: Utc::now(),
            origin: "box".into(),
            mc_version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: Some("0.16.5".into()),
            note: "before shaders".into(),
        });
        save(&provider, "id", &manifest).expect("save");
        let back = load(&provider, "id").expect("load");
        assert_eq!(back.name, "Fabric 26.3");
        assert_eq!(back.snapshots.len(), 1);
        assert_eq!(back.snapshots[0].note, "before shaders");
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let (_dir, provider) = provider();
        provider
            .write("instances/id/manifest.json", br#"{"schema": 99, "snapshots": []}"#)
            .expect("write");
        let error = load(&provider, "id").expect_err("must refuse");
        assert!(matches!(error, SyncError::UnsupportedSchema(99)));
    }

    #[test]
    fn retention_never_drops_below_one_snapshot() {
        let (_dir, provider) = provider();
        let mut manifest = SyncManifest::empty("id");
        for index in 0..3 {
            manifest.snapshots.push(Snapshot {
                file: format!("snapshots/{index}.zip"),
                sha256: "0".repeat(64),
                bytes: 1,
                created_at: Utc::now(),
                origin: String::new(),
                mc_version: String::new(),
                loader: String::new(),
                loader_version: None,
                note: String::new(),
            });
        }
        let removed = retention(&mut manifest, &provider, "id", 0).expect("retention");
        assert_eq!(removed.len(), 2);
        assert_eq!(manifest.snapshots.len(), 1);
    }
}
