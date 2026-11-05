//! In-game overlay: a second, always-on-top, transparent window.
//!
//! Design notes that are not obvious from the code below:
//!
//! * The overlay is a **separate window** loading `overlay.html`, not a view
//!   inside the launcher window. A launcher window that is minimised behind a
//!   game cannot be brought forward without stealing focus from the game.
//! * On Windows the window gets `WS_EX_NOACTIVATE`, so showing it does not
//!   take focus away from the running game. The cost is that the overlay can
//!   never receive keyboard input there: it is dismissed with the same global
//!   shortcut (or the close button), not with Escape.
//! * The shortcut is claimed **system-wide**. That is what makes it work while
//!   Minecraft has focus, and it also means the combination no longer reaches
//!   other applications while the overlay is enabled.
//! * Metrics are sampled on demand while the overlay asks for them; there is no
//!   background sampler when the overlay is closed.

use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use launcher_core::metrics::{LiveMetrics, Sampler};
use launcher_core::InstanceConfig;
use launcher_modrinth::installed::InstalledMods;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

/// Label of the overlay window, as declared in `tauri.conf.json`.
pub const OVERLAY_LABEL: &str = "overlay";

/// How many mods a single payload may carry. The overlay shows a handful; the
/// cap keeps the IPC message small on a 400-mod pack.
const MAX_MODS: usize = 200;

/// Shortcut currently registered through this module. Only one exists at a
/// time, and it is compared against the handler's event so a future second
/// registration cannot toggle the overlay by accident.
static ACTIVE: Mutex<Option<Shortcut>> = Mutex::new(None);
/// Why the last registration attempt failed, surfaced in `overlay_status`.
static HOTKEY_ERROR: Mutex<Option<String>> = Mutex::new(None);
static SAMPLER: OnceLock<Sampler> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct OverlayStatus {
    pub enabled: bool,
    /// The configured combination, exactly as stored.
    pub hotkey: String,
    /// Whether the OS actually accepted it.
    pub registered: bool,
    pub error: Option<String>,
    pub window: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayInstance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub max_memory_mb: u32,
    pub uptime_secs: i64,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayMod {
    pub title: String,
    pub version: String,
    pub file_name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayPayload {
    pub instance: Option<OverlayInstance>,
    pub metrics: LiveMetrics,
    pub mods: Vec<OverlayMod>,
    pub mods_total: usize,
    pub captured_at: String,
}

/// Register the shortcut plugin with the overlay's toggle handler.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let active = ACTIVE.lock().ok().and_then(|guard| *guard);
            if active != Some(*shortcut) {
                return;
            }
            if let Err(error) = toggle(app, None) {
                tracing::warn!(%error, "overlay: toggle failed");
            }
        })
        .build()
}

/// Run once at startup and after every settings save.
pub fn init(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        // Hiding here also covers a window that a previous run left visible
        // (Tauri restores visibility from the config, which says `false`; this
        // is belt and braces for a mid-session reload).
        let _ = window.hide();
        #[cfg(windows)]
        if let Err(error) = make_non_activating(&window) {
            tracing::warn!(%error, "overlay: cannot mark the window non-activating");
        }
    }
    if let Err(error) = apply_hotkey(app) {
        tracing::warn!(%error, "overlay: shortcut not registered");
    }
}

/// Unregister the previous combination and register the configured one.
///
/// Disabled means "unregistered", not "registered but ignored": a global
/// shortcut that silently swallows Shift+Tab for a feature the user turned off
/// is worse than no shortcut at all.
pub fn apply_hotkey(app: &AppHandle) -> Result<(), String> {
    let settings = app.state::<AppState>().settings();
    let global = app.global_shortcut();

    if let Some(previous) = ACTIVE.lock().ok().and_then(|mut guard| guard.take()) {
        let _ = global.unregister(previous);
    }
    set_error(None);

    if !settings.overlay_enabled {
        if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
            let _ = window.hide();
        }
        return Ok(());
    }

    let raw = settings.overlay_hotkey.trim().to_string();
    let shortcut: Shortcut = raw.parse().map_err(|error| {
        let message = format!("{raw} is not a valid shortcut ({error})");
        set_error(Some(message.clone()));
        message
    })?;

    global.register(shortcut).map_err(|error| {
        let message = format!("The system refused {raw}: {error}");
        set_error(Some(message.clone()));
        message
    })?;

    if let Ok(mut guard) = ACTIVE.lock() {
        *guard = Some(shortcut);
    }
    Ok(())
}

fn set_error(message: Option<String>) {
    if let Ok(mut guard) = HOTKEY_ERROR.lock() {
        *guard = message;
    }
}

#[tauri::command]
pub fn overlay_status(app: AppHandle, state: State<'_, AppState>) -> OverlayStatus {
    let settings = state.settings();
    let window = app.get_webview_window(OVERLAY_LABEL);
    OverlayStatus {
        enabled: settings.overlay_enabled,
        hotkey: settings.overlay_hotkey,
        registered: ACTIVE.lock().map(|guard| guard.is_some()).unwrap_or(false),
        error: HOTKEY_ERROR.lock().ok().and_then(|guard| guard.clone()),
        window: window.is_some(),
        visible: window
            .as_ref()
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false),
    }
}

#[tauri::command]
pub fn overlay_payload(state: State<'_, AppState>) -> OverlayPayload {
    let running = state.running.list();
    // The most recently started game is the one the user is looking at; a
    // second instance in the background should not win the panel.
    let game = running.last();

    let instance = game.and_then(|game| {
        let config = InstanceConfig::load(&state.paths, &game.instance_id).ok()?;
        Some(OverlayInstance {
            id: config.id.clone(),
            name: config.name.clone(),
            mc_version: config.game.version.clone(),
            loader: config.game.loader.as_str().to_string(),
            loader_version: config.game.loader_version.clone(),
            max_memory_mb: config.java.max_memory_mb,
            uptime_secs: (Utc::now() - game.started_at).num_seconds().max(0),
            pid: game.pid,
        })
    });

    let (mods, mods_total) = match game {
        Some(game) => {
            let game_dir = state.paths.game_dir(&game.instance_id);
            match InstalledMods::load(&game_dir) {
                Ok(manifest) => {
                    let total = manifest.mods.len();
                    let mut mods: Vec<OverlayMod> = manifest
                        .mods
                        .into_iter()
                        .map(|entry| OverlayMod {
                            title: entry.project_title,
                            version: entry.version_number,
                            file_name: entry.file_name,
                            bytes: entry.size,
                        })
                        .collect();
                    mods.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
                    mods.truncate(MAX_MODS);
                    (mods, total)
                }
                // A missing manifest is not an error here: the overlay is a
                // read-only glance, so it shows an empty list rather than a
                // failure banner over the game.
                Err(_) => (Vec::new(), 0),
            }
        }
        None => (Vec::new(), 0),
    };

    let sampler = SAMPLER.get_or_init(Sampler::new);
    OverlayPayload {
        instance,
        metrics: sampler.sample(game.map(|game| game.pid)),
        mods,
        mods_total,
        captured_at: Utc::now().to_rfc3339(),
    }
}

/// Show or hide the overlay. `show: None` toggles.
///
/// An explicit `show: true` works even when the feature is disabled in the
/// settings — that is the "Test overlay" button, and it must be able to show
/// the user what they are about to enable.
#[tauri::command]
pub fn overlay_toggle(app: AppHandle, show: Option<bool>) -> Result<bool, String> {
    toggle(&app, show)
}

#[tauri::command]
pub fn overlay_close(app: AppHandle) -> Result<bool, String> {
    toggle(&app, Some(false))
}

fn toggle(app: &AppHandle, show: Option<bool>) -> Result<bool, String> {
    let window = app
        .get_webview_window(OVERLAY_LABEL)
        .ok_or_else(|| "the overlay window is missing from the configuration".to_string())?;

    let visible = window.is_visible().unwrap_or(false);
    let next = show.unwrap_or(!visible);

    if !next {
        if visible {
            window.hide().map_err(|error| error.to_string())?;
        }
        return Ok(false);
    }

    place(&window)?;
    window.show().map_err(|error| error.to_string())?;
    // Re-asserted on every show: a fullscreen game can reset the topmost flag.
    let _ = window.set_always_on_top(true);
    #[cfg(windows)]
    make_non_activating(&window)?;
    let _ = app.emit_to(OVERLAY_LABEL, "overlay://shown", ());
    Ok(true)
}

/// Top-right corner of the monitor the launcher is on, with a margin.
fn place(window: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or(window
            .primary_monitor()
            .map_err(|error| error.to_string())?);
    let Some(monitor) = monitor else {
        return Ok(());
    };

    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let origin = monitor.position();
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let margin = (24.0 * scale).round() as i32;

    // Clamp the left edge so a window wider than the screen still starts on it
    // instead of off the top-left corner.
    let x = (origin.x + screen.width as i32 - size.width as i32 - margin).max(origin.x + margin);
    let y = origin.y + margin;

    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())
}

/// Keep the overlay from ever becoming the foreground window on Windows.
///
/// `WS_EX_NOACTIVATE` means `ShowWindow` does not activate; the game keeps
/// keyboard focus while the overlay is visible. Mouse clicks on the overlay
/// still work, which is enough for the close button.
#[cfg(all(windows, target_pointer_width = "64"))]
fn make_non_activating(window: &tauri::WebviewWindow) -> Result<(), String> {
    use std::ffi::c_void;

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(hwnd: *mut c_void, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: *mut c_void, index: i32, value: isize) -> isize;
    }

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;

    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    if hwnd.is_null() {
        return Err("the overlay window has no handle yet".to_string());
    }
    // SAFETY: `hwnd` is a live window handle owned by Tauri; both calls are
    // documented to read and write one style word.
    let styles = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let wanted = styles | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
    if wanted != styles {
        // SAFETY: same handle; `SetWindowLongPtrW` only changes the style word.
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted) };
    }
    Ok(())
}

#[cfg(not(all(windows, target_pointer_width = "64")))]
fn make_non_activating(_window: &tauri::WebviewWindow) -> Result<(), String> {
    // macOS and Linux rely on the window config (`focus: false`,
    // `alwaysOnTop`) and never call `set_focus` on the overlay.
    Ok(())
}
