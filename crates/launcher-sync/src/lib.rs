//! Instance backup and restore against a pluggable remote store.
//!
//! The shape is deliberately boring: each instance is a folder on the remote,
//! each backup is one zip archive plus a `manifest.json` listing what is there.
//! There is no daemon, no watcher and no background upload — a push is a user
//! action, a pull is a user action.
//!
//! ```text
//! <remote>/
//! └── instances/
//!     └── fabric-26-3/
//!         ├── manifest.json
//!         └── snapshots/
//!             ├── 20261104-183012-1f3c9ab2.zip
//!             └── 20261103-101155-77aa01de.zip
//! ```
//!
//! Snapshots are named by timestamp *and* a hash prefix, so two devices pushing
//! the same minute can never collide, and an unchanged instance is not uploaded
//! twice.
//!
//! Conflict policy, in one sentence: **a restore never overwrites anything.**
//! Pulling a snapshot hands the archive to the normal import path, which creates
//! a new instance. Two machines can therefore back up and restore freely without
//! either of them silently clobbering the other; the cost is that "replace my
//! local copy" is an explicit delete-then-restore, not a merge.
//!
//! [`provider::SyncProvider`] is six methods wide on purpose. A folder (a
//! mounted drive, a Syncthing folder, a network share) is implemented here and
//! is enough for a real workflow; S3 and Dropbox are the same six methods over
//! HTTP and are documented in `docs/sync.md` as the next step.

pub mod manifest;
pub mod provider;
pub mod state;

pub use manifest::{retention, Snapshot, SyncManifest, MANIFEST_NAME, SCHEMA};
pub use provider::{sha256_file, FolderProvider, SyncProvider};
pub use state::{InstanceSyncState, SyncState};

use std::path::Path;

use chrono::Utc;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: std::path::PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// A key that could escape the provider root — refused before it reaches
    /// the filesystem, never sanitised into something else.
    #[error("unsafe remote key: {0}")]
    UnsafeKey(String),

    #[error("no such snapshot in the manifest: {0}")]
    SnapshotMissing(String),

    /// The bytes on the remote do not match the hash in the manifest. The
    /// downloaded file is deleted rather than imported.
    #[error("snapshot {file} failed verification (expected {expected}, got {actual})")]
    HashMismatch {
        file: String,
        expected: String,
        actual: String,
    },

    /// A remote written by a newer launcher. Guessing at an unknown shape is
    /// worse than telling the user to update.
    #[error("remote manifest uses schema {0}, this launcher understands {SCHEMA}")]
    UnsupportedSchema(u32),
}

impl SyncError {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Prefix every key for one instance lives under.
pub fn instance_prefix(instance_id: &str) -> String {
    format!("instances/{instance_id}")
}

/// What one push did, in enough detail for the UI to report it honestly.
#[derive(Debug, Clone)]
pub struct PushOutcome {
    pub snapshot: Snapshot,
    /// Snapshot files deleted by retention, oldest first.
    pub removed: Vec<String>,
    pub manifest: SyncManifest,
    /// True when identical bytes were already stored: nothing was uploaded and
    /// `snapshot` is the one that was already there.
    pub deduplicated: bool,
}

/// Push one archive and rewrite the manifest.
///
/// `origin` is a human-readable device label, stored in the snapshot so a
/// second machine can see where a backup came from before restoring it.
pub fn push(
    provider: &dyn SyncProvider,
    instance_id: &str,
    archive: &Path,
    meta: manifest::SnapshotMeta,
    keep: usize,
) -> Result<PushOutcome> {
    let (sha256, bytes) = sha256_file(archive)?;
    let mut current = manifest::load(provider, instance_id)?;

    // Same bytes already on the remote (any snapshot): touch nothing, just
    // report the existing one. This is what makes "back up" cheap to spam.
    if let Some(existing) = current
        .snapshots
        .iter()
        .find(|snapshot| snapshot.sha256 == sha256)
        .cloned()
    {
        return Ok(PushOutcome {
            snapshot: existing,
            removed: Vec::new(),
            manifest: current,
            deduplicated: true,
        });
    }

    let stamp = Utc::now();
    let file = format!(
        "snapshots/{}-{}.zip",
        stamp.format("%Y%m%d-%H%M%S"),
        &sha256[..8]
    );
    let key = format!("{}/{file}", instance_prefix(instance_id));
    provider.put_file(&key, archive)?;

    let snapshot = Snapshot {
        file,
        sha256,
        bytes,
        created_at: stamp,
        origin: meta.origin,
        mc_version: meta.mc_version,
        loader: meta.loader,
        loader_version: meta.loader_version,
        note: meta.note,
    };

    current.instance_id = instance_id.to_string();
    if !meta.name.is_empty() {
        current.name = meta.name;
    }
    current.snapshots.insert(0, snapshot.clone());
    current.snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let removed = retention(&mut current, provider, instance_id, keep)?;
    current.updated_at = Some(stamp);
    manifest::save(provider, instance_id, &current)?;

    Ok(PushOutcome {
        snapshot,
        removed,
        manifest: current,
        deduplicated: false,
    })
}

/// Download one snapshot and verify it against the manifest hash.
pub fn pull(
    provider: &dyn SyncProvider,
    instance_id: &str,
    file: &str,
    target: &Path,
) -> Result<Snapshot> {
    let current = manifest::load(provider, instance_id)?;
    let snapshot = current
        .snapshots
        .iter()
        .find(|snapshot| snapshot.file == file)
        .cloned()
        .ok_or_else(|| SyncError::SnapshotMissing(file.to_string()))?;

    let key = format!("{}/{file}", instance_prefix(instance_id));
    provider.get_file(&key, target)?;

    let (actual, _) = sha256_file(target)?;
    if actual != snapshot.sha256 {
        let _ = std::fs::remove_file(target);
        return Err(SyncError::HashMismatch {
            file: file.to_string(),
            expected: snapshot.sha256.clone(),
            actual,
        });
    }

    Ok(snapshot)
}

/// Remote manifests for every instance the store knows about, newest first.
pub fn remotes(provider: &dyn SyncProvider) -> Result<Vec<SyncManifest>> {
    let mut out = Vec::new();
    for key in provider.list("instances/")? {
        // `instances/<id>/manifest.json`
        let mut parts = key.split('/');
        if parts.next() != Some("instances") {
            continue;
        }
        let Some(instance_id) = parts.next() else {
            continue;
        };
        if parts.next() != Some(MANIFEST_NAME) {
            continue;
        }
        let Ok(manifest) = manifest::load(provider, instance_id) else {
            continue;
        };
        out.push(manifest);
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Drop one snapshot from the manifest and from the store.
pub fn remove_snapshot(
    provider: &dyn SyncProvider,
    instance_id: &str,
    file: &str,
) -> Result<SyncManifest> {
    let mut current = manifest::load(provider, instance_id)?;
    let before = current.snapshots.len();
    current.snapshots.retain(|snapshot| snapshot.file != file);
    if current.snapshots.len() == before {
        return Err(SyncError::SnapshotMissing(file.to_string()));
    }
    let key = format!("{}/{file}", instance_prefix(instance_id));
    provider.remove(&key)?;
    current.updated_at = Some(Utc::now());
    manifest::save(provider, instance_id, &current)?;
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use manifest::SnapshotMeta;

    fn provider() -> (tempfile::TempDir, FolderProvider) {
        let dir = tempfile::tempdir().expect("temp");
        let provider = FolderProvider::new(dir.path().join("remote")).expect("provider");
        (dir, provider)
    }

    fn archive(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write archive");
        path
    }

    fn meta(name: &str) -> SnapshotMeta {
        SnapshotMeta {
            name: name.to_string(),
            origin: "test-box".to_string(),
            mc_version: "1.21.1".to_string(),
            loader: "fabric".to_string(),
            loader_version: Some("0.16.5".to_string()),
            note: String::new(),
        }
    }

    #[test]
    fn push_then_pull_round_trips_the_archive() {
        let (dir, provider) = provider();
        let source = archive(dir.path(), "a.zip", b"instance-bytes");
        let out = push(&provider, "fabric-26-3", &source, meta("Fabric 26.3"), 5).expect("push");
        assert!(!out.deduplicated);
        assert!(out.snapshot.file.ends_with(".zip"));

        let target = dir.path().join("restored.zip");
        let snapshot = pull(&provider, "fabric-26-3", &out.snapshot.file, &target).expect("pull");
        assert_eq!(snapshot.sha256, out.snapshot.sha256);
        assert_eq!(std::fs::read(&target).expect("read"), b"instance-bytes");
    }

    #[test]
    fn pushing_identical_bytes_is_a_no_op() {
        let (dir, provider) = provider();
        let source = archive(dir.path(), "a.zip", b"same");
        let first = push(&provider, "id", &source, meta("x"), 5).expect("first");
        let second = push(&provider, "id", &source, meta("x"), 5).expect("second");
        assert!(second.deduplicated);
        assert_eq!(first.snapshot.file, second.snapshot.file);
        assert_eq!(
            manifest::load(&provider, "id").expect("manifest").snapshots.len(),
            1
        );
    }

    #[test]
    fn retention_keeps_the_newest_and_deletes_the_rest() {
        let (dir, provider) = provider();
        for index in 0..4 {
            let source = archive(dir.path(), &format!("{index}.zip"), format!("body-{index}").as_bytes());
            // A distinct second between pushes keeps `created_at` ordered.
            push(&provider, "id", &source, meta("x"), 2).expect("push");
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        let manifest = manifest::load(&provider, "id").expect("manifest");
        assert_eq!(manifest.snapshots.len(), 2);
        let on_disk = provider.list("instances/id/snapshots/").expect("list");
        assert_eq!(on_disk.len(), 2);
        assert_eq!(on_disk.len(), manifest.snapshots.len());
    }

    #[test]
    fn a_corrupted_snapshot_is_refused_and_removed() {
        let (dir, provider) = provider();
        let source = archive(dir.path(), "a.zip", b"good");
        let out = push(&provider, "id", &source, meta("x"), 5).expect("push");

        // Corrupt the stored file behind the manifest's back.
        let stored = dir
            .path()
            .join("remote/instances/id")
            .join(&out.snapshot.file);
        std::fs::write(&stored, b"tampered").expect("corrupt");

        let target = dir.path().join("pulled.zip");
        let error = pull(&provider, "id", &out.snapshot.file, &target).expect_err("must fail");
        assert!(matches!(error, SyncError::HashMismatch { .. }));
        assert!(!target.exists(), "a failed pull leaves no file behind");
    }

    #[test]
    fn pulling_a_file_the_manifest_does_not_list_fails() {
        let (dir, provider) = provider();
        let _ = dir;
        let error = pull(&provider, "id", "snapshots/nope.zip", &std::path::PathBuf::from("x"))
            .expect_err("must fail");
        assert!(matches!(error, SyncError::SnapshotMissing(_)));
    }

    #[test]
    fn remove_snapshot_updates_manifest_and_store() {
        let (dir, provider) = provider();
        let source = archive(dir.path(), "a.zip", b"a");
        let out = push(&provider, "id", &source, meta("x"), 5).expect("push");
        let after = remove_snapshot(&provider, "id", &out.snapshot.file).expect("remove");
        assert!(after.snapshots.is_empty());
        assert!(provider
            .list("instances/id/snapshots/")
            .expect("list")
            .is_empty());
    }

    #[test]
    fn remotes_lists_one_manifest_per_instance() {
        let (dir, provider) = provider();
        push(&provider, "alpha", &archive(dir.path(), "a.zip", b"a"), meta("Alpha"), 5)
            .expect("push");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        push(&provider, "beta", &archive(dir.path(), "b.zip", b"b"), meta("Beta"), 5)
            .expect("push");

        let remotes = remotes(&provider).expect("remotes");
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].instance_id, "beta", "newest manifest first");
    }
}
