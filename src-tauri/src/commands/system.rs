use std::path::PathBuf;

use launcher_core::java::{self, JavaRuntime};
use launcher_core::settings::LauncherSettings;
use launcher_core::version::VersionSummary;
use launcher_core::LoaderKind;
use launcher_net::loaders::LoaderBuild;
use serde::Serialize;
use tauri::{Manager, State};

use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> LauncherSettings {
    state.settings()
}

#[tauri::command]
pub fn settings_save(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mut settings: LauncherSettings,
) -> Result<LauncherSettings, String> {
    settings.sanitise();
    settings.save(&state.paths).map_err(|e| e.to_string())?;
    if let Ok(mut guard) = state.settings.write() {
        *guard = settings.clone();
    }
    // A client id pasted into the UI must work without a restart.
    state.refresh_auth();
    state.refresh_presence();
    // Same for the overlay shortcut: register/unregister without a restart. A
    // rejected combination is not a save error — the settings stay as typed and
    // `overlay_status` reports why the OS refused it.
    if let Err(error) = crate::commands::overlay::apply_hotkey(&app) {
        tracing::warn!(%error, "overlay: shortcut not applied");
    }
    Ok(settings)
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowState {
    pub fullscreen: bool,
    pub maximized: bool,
    pub minimized: bool,
}

/// Hide the launcher behind a running game, or bring it back after one exits.
///
/// Exposed separately from the launch hand-off so the UI can test the window
/// machinery and so the game transition has one primitive to lean on.
#[tauri::command]
pub fn window_minimize(app: tauri::AppHandle, minimize: bool) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "the main window is gone".to_string())?;
    if minimize {
        window.minimize().map_err(|error| error.to_string())?;
    } else {
        window.unminimize().map_err(|error| error.to_string())?;
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(minimize)
}

#[tauri::command]
pub fn window_state(app: tauri::AppHandle) -> Result<WindowState, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "the main window is gone".to_string())?;
    Ok(WindowState {
        fullscreen: window.is_fullscreen().unwrap_or(false),
        maximized: window.is_maximized().unwrap_or(false),
        minimized: window.is_minimized().unwrap_or(false),
    })
}

/// Drive the launcher window between windowed and fullscreen.
///
/// Kept in Rust rather than the JS window API: the launcher already owns the
/// window handle for minimize/restore, and one owner means no fight between the
/// game transition (which drops fullscreen) and the keybind.
#[tauri::command]
pub fn window_fullscreen(app: tauri::AppHandle, fullscreen: bool) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "the main window is gone".to_string())?;
    window
        .set_fullscreen(fullscreen)
        .map_err(|error| error.to_string())?;
    Ok(fullscreen)
}

#[tauri::command]
pub async fn java_detect(state: State<'_, AppState>) -> Result<Vec<JavaRuntime>, String> {
    Ok(java::discover(&state.paths).await)
}

#[tauri::command]
pub async fn java_inspect(path: PathBuf) -> Result<Option<JavaRuntime>, String> {
    Ok(java::inspect(&path).await)
}

/// Download and unpack a Temurin JRE into the managed runtime folder.
///
/// Used by the Settings button and by the launch path when no installed runtime
/// satisfies the version — `<root>/java/` is scanned by `java::discover`, so the
/// fresh JRE is picked up without a restart.
#[tauri::command]
pub async fn java_install(
    state: State<'_, AppState>,
    major: u32,
) -> Result<crate::java_install::JavaInstallResult, String> {
    if !(8..=30).contains(&major) {
        return Err(format!("Java {major} is not a release this launcher installs"));
    }
    // A previous run may already have unpacked this release; probing the disk
    // first turns a repeat click into an instant answer instead of a download.
    if let Some(runtime) = crate::java_install::installed(&state.paths, major).await {
        return Ok(crate::java_install::JavaInstallResult {
            major,
            version: runtime.version,
            path: runtime.path,
            bytes: 0,
        });
    }
    crate::java_install::install(&state.http(), &state.paths, major).await
}

#[tauri::command]
pub async fn minecraft_versions(
    state: State<'_, AppState>,
    include_snapshots: bool,
) -> Result<Vec<VersionSummary>, String> {
    let manifest = state
        .installer
        .manifest(false)
        .await
        .map_err(|e| e.to_string())?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| include_snapshots || v.kind == "release")
        .collect())
}

#[tauri::command]
pub async fn loader_versions(
    state: State<'_, AppState>,
    loader: LoaderKind,
    mc_version: String,
) -> Result<Vec<LoaderBuild>, String> {
    state
        .loaders
        .versions(loader, &mc_version)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn data_root(state: State<'_, AppState>) -> String {
    state.paths.root().display().to_string()
}
