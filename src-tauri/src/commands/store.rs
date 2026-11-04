use launcher_core::modstore::{DedupeReport, GcReport, ModStore, StoreStats};
use tauri::State;

use crate::state::AppState;

fn store(state: &AppState) -> ModStore {
    ModStore::new(&state.paths)
}

#[tauri::command]
pub async fn store_stats(state: State<'_, AppState>) -> Result<StoreStats, String> {
    let store = store(&state);
    tauri::async_runtime::spawn_blocking(move || store.stats())
        .await
        .map_err(|e| format!("store task failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Route one instance's mods through the content-addressed store and replace
/// them with hard links. Safe to run repeatedly: every file that is already in
/// the store becomes a link instead of a second copy.
#[tauri::command]
pub async fn store_dedupe(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<DedupeReport, String> {
    let store = store(&state);
    let paths = state.paths.clone();
    let lock = state.store_lock.lock().await;
    let result = tauri::async_runtime::spawn_blocking(move || {
        store.dedupe_instance(&paths, &instance_id)
    })
    .await
    .map_err(|e| format!("dedupe task failed: {e}"))?;
    drop(lock);
    result.map_err(|e| e.to_string())
}

/// Reclaim blobs no instance points at any more.
#[tauri::command]
pub async fn store_gc(
    state: State<'_, AppState>,
    dry_run: bool,
) -> Result<GcReport, String> {
    let store = store(&state);
    let paths = state.paths.clone();
    let lock = state.store_lock.lock().await;
    let result =
        tauri::async_runtime::spawn_blocking(move || store.gc(&paths, dry_run)).await;
    drop(lock);
    result
        .map_err(|e| format!("gc task failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// Forget a single file without scheduling a full GC sweep.
#[tauri::command]
pub async fn store_forget(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> Result<(), String> {
    let store = store(&state);
    let paths = state.paths.clone();
    let lock = state.store_lock.lock().await;
    let result = tauri::async_runtime::spawn_blocking(move || {
        store.forget_named(&paths, &instance_id, &file_name)
    })
    .await;
    drop(lock);
    result
        .map_err(|e| format!("store task failed: {e}"))?
        .map_err(|e| e.to_string())
}
