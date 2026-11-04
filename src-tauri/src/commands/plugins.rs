use launcher_core::plugin::{PluginEntry, PluginHost};
use tauri::State;

use crate::state::AppState;

fn host(state: &AppState) -> PluginHost {
    PluginHost::new(&state.paths)
}

/// Everything under `plugins/`, with enable state and any load error.
///
/// Read-only: this never executes plugin code. The webview only receives source
/// for plugins the user has explicitly turned on.
#[tauri::command]
pub async fn plugins_list(state: State<'_, AppState>) -> Result<Vec<PluginEntry>, String> {
    let host = host(&state);
    tauri::async_runtime::spawn_blocking(move || host.list())
        .await
        .map_err(|e| format!("plugin scan failed: {e}"))
}

#[tauri::command]
pub async fn plugins_set_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Vec<PluginEntry>, String> {
    let host = host(&state);
    tauri::async_runtime::spawn_blocking(move || {
        host.set_enabled(&id, enabled)?;
        Ok::<_, launcher_core::CoreError>(host.list())
    })
    .await
    .map_err(|e| format!("plugin update failed: {e}"))?
    .map_err(|e| e.to_string())
}

/// Reveal the plugin folder in the OS file manager. Opens exactly one path —
/// the launcher's own `plugins/` directory — and does not accept a user path.
#[tauri::command]
pub async fn plugins_open_folder(state: State<'_, AppState>) -> Result<String, String> {
    let dir = state.paths.plugins_dir();
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
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
        // These launchers return the file manager's own exit code, which is not
        // the launcher's problem; only a failure to spawn is reported.
        command
            .spawn()
            .map(|_| dir.display().to_string())
            .map_err(|e| format!("cannot open {}: {e}", dir.display()))
    })
    .await
    .map_err(|e| format!("plugin folder task failed: {e}"))?
}
