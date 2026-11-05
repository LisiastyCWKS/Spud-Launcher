//! Apparence commands: the background image folder.
//!
//! The launcher never serves arbitrary paths. Every name handed to these
//! commands is re-validated by `launcher_core::appearance`, and the only
//! directory they read is `<root>/backgrounds/`.

use launcher_core::appearance::{self, BackgroundFile};
use tauri::State;

use crate::state::AppState;

/// Images the user dropped into `<root>/backgrounds/`, sorted by name.
///
/// Read-only and cheap: a settings panel asks for this once per open, not once
/// per frame. A missing folder is an empty list.
#[tauri::command]
pub async fn backgrounds_list(state: State<'_, AppState>) -> Result<Vec<BackgroundFile>, String> {
    let dir = state.paths.backgrounds_dir();
    tauri::async_runtime::spawn_blocking(move || appearance::list_backgrounds(&dir))
        .await
        .map_err(|error| format!("background scan failed: {error}"))
}

/// One background as a `data:` URL, ready for CSS.
///
/// Base64 rather than the asset protocol: the data root can live anywhere
/// (including a USB stick in portable mode), so an asset scope would have to be
/// as wide as the filesystem. A 4K JPEG costs a few megabytes over IPC once,
/// which is cheaper than that.
#[tauri::command]
pub async fn background_data(
    state: State<'_, AppState>,
    name: String,
) -> Result<Option<String>, String> {
    let dir = state.paths.backgrounds_dir();
    tauri::async_runtime::spawn_blocking(move || appearance::background_data_url(&dir, &name))
        .await
        .map_err(|error| format!("background read failed: {error}"))
}

/// Reveal `<root>/backgrounds/` in the OS file manager so the user can drop
/// images in without hunting for the data directory.
#[tauri::command]
pub async fn backgrounds_open_folder(state: State<'_, AppState>) -> Result<String, String> {
    let dir = state.paths.backgrounds_dir();
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
        // The file manager's own exit code is not the launcher's problem; only
        // a failure to spawn is reported.
        command
            .spawn()
            .map(|_| dir.display().to_string())
            .map_err(|error| format!("cannot open {}: {error}", dir.display()))
    })
    .await
    .map_err(|error| format!("background folder task failed: {error}"))?
}
