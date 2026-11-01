use launcher_core::instance::{
    instance_path, list_instances, InstanceConfig, InstanceSummary, NewInstance,
};
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;

#[tauri::command]
pub fn instances_list(state: State<'_, AppState>) -> Result<Vec<InstanceSummary>, String> {
    list_instances(&state.paths).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn instance_get(state: State<'_, AppState>, id: String) -> Result<InstanceConfig, String> {
    InstanceConfig::load(&state.paths, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn instance_create(
    state: State<'_, AppState>,
    draft: NewInstance,
) -> Result<InstanceConfig, String> {
    InstanceConfig::create(&state.paths, draft).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn instance_update(
    state: State<'_, AppState>,
    mut config: InstanceConfig,
) -> Result<InstanceConfig, String> {
    instance_path(&state.paths, &config.id).map_err(|e| e.to_string())?;
    config.save_touched(&state.paths).map_err(|e| e.to_string())?;
    Ok(config)
}

#[tauri::command]
pub async fn instance_delete(app: AppHandle, id: String) -> Result<(), String> {
    // A running JVM holds the game jar, assets and mod files open. Stopping
    // first is the only way the delete can finish on Windows.
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        state.running.stop(&id)?;
        let config = InstanceConfig::load(&state.paths, &id).map_err(|e| e.to_string())?;
        config.delete(&state.paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("delete task failed: {e}"))?
}

#[tauri::command]
pub fn instance_folder(state: State<'_, AppState>, id: String) -> Result<String, String> {
    instance_path(&state.paths, &id)
        .map(|p| {
            p.join(".minecraft")
                .to_string_lossy()
                .to_string()
        })
        .map_err(|e| e.to_string())
}
