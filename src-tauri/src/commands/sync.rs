//! Cloud sync: instance backup and restore against a configured remote folder.
//!
//! The command layer is deliberately thin. Archiving is the existing instance
//! export (so a backup is the same bytes as an export, worlds included), and
//! restoring is the existing import (so a restore is a normal instance, staged
//! and validated). Everything that decides *what* goes where — the provider,
//! the manifest, retention, hashing — lives in `launcher-sync` where it is
//! unit-tested without Tauri.
//!
//! Conflict policy: **pull never overwrites a local instance.** The archive is
//! imported as a new instance. Replacing a local copy is therefore an explicit
//! "delete, then restore", never a silent merge. `remote_is_newer` in the local
//! state file is what the UI uses to warn that another device pushed.

use chrono::{DateTime, Utc};
use launcher_core::instance::{list_instances, validate_id, InstanceConfig};
use launcher_sync::manifest::{Snapshot, SnapshotMeta, SyncManifest};
use launcher_sync::state::SyncState;
use launcher_sync::{push, remotes, FolderProvider};
use serde::Serialize;
use tauri::State;

use crate::commands::transfer::{import_archive, instance_export_to};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct SyncInstanceStatus {
    pub instance_id: String,
    pub name: String,
    pub last_push_at: Option<String>,
    pub last_snapshot: Option<String>,
    pub last_pull_at: Option<String>,
    pub remote_updated_at: Option<String>,
    /// True when the remote lists something newer than this machine has seen.
    pub remote_newer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub configured: bool,
    pub backend: String,
    pub folder: Option<String>,
    /// The folder exists and is a directory.
    pub reachable: bool,
    /// A probe file could be written and removed.
    pub writable: bool,
    pub keep: u32,
    pub cache_dir: String,
    pub local: Vec<SyncInstanceStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncRemoteView {
    pub instance_id: String,
    pub name: String,
    pub updated_at: Option<String>,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncPushResult {
    pub snapshot: Snapshot,
    pub removed: Vec<String>,
    pub deduplicated: bool,
    pub files: usize,
    pub archive_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncPullResult {
    pub snapshot: Snapshot,
    pub instance: InstanceConfig,
    pub files: usize,
}

fn stamp(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|time| time.to_rfc3339())
}

/// A device label for the snapshot. Only used inside the user's own remote.
fn origin_label() -> String {
    ["COMPUTERNAME", "HOSTNAME", "USERDOMAIN"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "this device".to_string())
}

fn provider(state: &AppState) -> Result<FolderProvider, String> {
    let Some(folder) = state.settings().sync_folder else {
        return Err(
            "Cloud sync is not configured — pick a folder in Settings → Cloud sync.".to_string(),
        );
    };
    FolderProvider::new(folder).map_err(|error| error.to_string())
}

fn cache_dir(state: &AppState) -> std::path::PathBuf {
    state.paths.cache_dir().join("sync")
}

fn prepare_cache(state: &AppState) -> Result<std::path::PathBuf, String> {
    let cache = cache_dir(state);
    std::fs::create_dir_all(&cache)
        .map_err(|error| format!("cannot create {}: {error}", cache.display()))?;
    Ok(cache)
}

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let settings = state.settings();
    let instances = list_instances(&state.paths).map_err(|error| error.to_string())?;
    let local_state = SyncState::load(state.paths.root());

    let folder = settings.sync_folder.clone();
    let mut reachable = false;
    let mut writable = false;
    if let Some(folder) = &folder {
        reachable = folder.is_dir();
        if reachable {
            let probe = folder.join(".spud-write-test");
            writable = std::fs::write(&probe, b"ok").is_ok();
            let _ = std::fs::remove_file(&probe);
        }
    }

    // One blocking hop for the remote manifests, so a slow network share never
    // stalls the async runtime the rest of the launcher runs on.
    let remote = if writable {
        match provider(&state) {
            Ok(provider) => {
                tauri::async_runtime::spawn_blocking(move || {
                    remotes(&provider).map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| format!("reading the sync folder failed: {error}"))?
                .unwrap_or_default()
            }
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let local = instances
        .into_iter()
        .map(|instance| {
            let entry = local_state.instances.get(&instance.id);
            let remote_updated_at = remote
                .iter()
                .find(|manifest| manifest.instance_id == instance.id)
                .and_then(|manifest| manifest.updated_at);
            SyncInstanceStatus {
                instance_id: instance.id.clone(),
                name: instance.name.clone(),
                last_push_at: stamp(entry.and_then(|entry| entry.last_push_at)),
                last_snapshot: entry.and_then(|entry| entry.last_snapshot.clone()),
                last_pull_at: stamp(entry.and_then(|entry| entry.last_pull_at)),
                remote_updated_at: stamp(remote_updated_at),
                remote_newer: local_state.remote_is_newer(&instance.id, remote_updated_at),
            }
        })
        .collect();

    Ok(SyncStatus {
        configured: folder.is_some(),
        backend: "folder".to_string(),
        folder: folder.map(|path| path.display().to_string()),
        reachable,
        writable,
        keep: settings.sync_keep,
        cache_dir: cache_dir(&state).display().to_string(),
        local,
    })
}

/// Every instance on the remote, newest manifest first.
#[tauri::command]
pub async fn sync_remotes(state: State<'_, AppState>) -> Result<Vec<SyncRemoteView>, String> {
    let provider = provider(&state)?;
    let manifests =
        tauri::async_runtime::spawn_blocking(move || remotes(&provider).map_err(|e| e.to_string()))
            .await
            .map_err(|error| format!("reading the sync folder failed: {error}"))??;

    Ok(manifests
        .into_iter()
        .map(|manifest| SyncRemoteView {
            instance_id: manifest.instance_id,
            name: manifest.name,
            updated_at: stamp(manifest.updated_at),
            snapshots: manifest.snapshots,
        })
        .collect())
}

/// Back one instance up. Export → hash → upload → manifest → retention.
#[tauri::command]
pub async fn sync_push(
    state: State<'_, AppState>,
    instance_id: String,
    note: Option<String>,
) -> Result<SyncPushResult, String> {
    validate_id(&instance_id).map_err(|error| error.to_string())?;
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let provider = provider(&state)?;
    let keep = state.settings().sync_keep as usize;
    let cache = prepare_cache(&state)?;

    let archive = cache.join(format!(
        "{}-{}.zip",
        instance_id,
        Utc::now().format("%Y%m%d-%H%M%S")
    ));

    let exported = match instance_export_to(&state, &instance_id, "zip", &archive).await {
        Ok(result) => result,
        Err(error) => {
            let _ = std::fs::remove_file(&archive);
            return Err(error);
        }
    };

    let meta = SnapshotMeta {
        name: config.name.clone(),
        origin: origin_label(),
        mc_version: config.game.version.clone(),
        loader: format!("{:?}", config.game.loader).to_lowercase(),
        loader_version: config.game.loader_version.clone(),
        note: note.unwrap_or_default().trim().to_string(),
    };

    let id = instance_id.clone();
    let archive_for_task = archive.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        push(&provider, &id, &archive_for_task, meta, keep).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("upload failed: {error}"))?;

    // The zip was only ever a transport; the remote now holds the bytes.
    let _ = std::fs::remove_file(&archive);

    let outcome = outcome?;
    let mut local_state = SyncState::load(state.paths.root());
    local_state.touch_push(&instance_id, &outcome.manifest, &outcome.snapshot.file);
    if let Err(error) = local_state.save(state.paths.root()) {
        // A lost timestamp is not a failed backup: the manifest is authoritative
        // about what is on the remote.
        tracing::warn!("could not save sync state: {error}");
    }

    Ok(SyncPushResult {
        snapshot: outcome.snapshot,
        removed: outcome.removed,
        deduplicated: outcome.deduplicated,
        files: exported.files,
        archive_bytes: exported.bytes,
    })
}

/// Download one snapshot, verify its hash and import it as a new instance.
#[tauri::command]
pub async fn sync_pull(
    state: State<'_, AppState>,
    instance_id: String,
    file: String,
) -> Result<SyncPullResult, String> {
    validate_id(&instance_id).map_err(|error| error.to_string())?;
    if file.contains("..") || file.starts_with('/') || file.contains('\\') {
        return Err("invalid snapshot name".to_string());
    }
    let provider = provider(&state)?;
    let cache = prepare_cache(&state)?;
    let target = cache.join(format!("pull-{}.zip", Utc::now().format("%Y%m%d-%H%M%S")));

    let id = instance_id.clone();
    let file_for_task = file.clone();
    let target_for_task = target.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        launcher_sync::pull(&provider, &id, &file_for_task, &target_for_task)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("download failed: {error}"))?;

    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = std::fs::remove_file(&target);
            return Err(error);
        }
    };

    let imported = import_archive(&state, target.clone()).await;
    let _ = std::fs::remove_file(&target);
    let imported = imported?;

    let mut local_state = SyncState::load(state.paths.root());
    local_state.touch_pull(&instance_id);
    if let Err(error) = local_state.save(state.paths.root()) {
        tracing::warn!("could not save sync state: {error}");
    }

    Ok(SyncPullResult {
        snapshot,
        instance: imported.instance,
        files: imported.files,
    })
}

/// Drop one snapshot from the remote manifest and delete its archive.
#[tauri::command]
pub async fn sync_forget_snapshot(
    state: State<'_, AppState>,
    instance_id: String,
    file: String,
) -> Result<SyncManifest, String> {
    validate_id(&instance_id).map_err(|error| error.to_string())?;
    let provider = provider(&state)?;
    let id = instance_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        launcher_sync::remove_snapshot(&provider, &id, &file).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("deleting the snapshot failed: {error}"))?
}

/// Reveal the sync folder in the OS file manager. Never takes a path argument:
/// it opens the folder from settings and nothing else.
#[tauri::command]
pub async fn sync_open_folder(state: State<'_, AppState>) -> Result<String, String> {
    let Some(dir) = state.settings().sync_folder else {
        return Err("Cloud sync is not configured.".to_string());
    };
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = if cfg!(windows) {
            let mut command = std::process::Command::new("explorer");
            command.arg(dir.as_os_str());
            command
        } else if cfg!(target_os = "macos") {
            let mut command = std::process::Command::new("open");
            command.arg(dir.as_os_str());
            command
        } else {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(dir.as_os_str());
            command
        };
        command
            .spawn()
            .map(|_| dir.display().to_string())
            .map_err(|error| format!("cannot open {}: {error}", dir.display()))
    })
    .await
    .map_err(|error| format!("opening the sync folder failed: {error}"))?
}
