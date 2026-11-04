use std::path::Path;

use chrono::Utc;
use launcher_core::instance::InstanceConfig;
use launcher_core::java::{self, JavaRuntime};
use launcher_core::JavaMode;
use launcher_core::launch::{build_plan, merged_for_instance, Account, LaunchRequest};
use launcher_core::version::load_version_json;
use launcher_core::LoaderKind;
use launcher_net::download::DownloadEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc::UnboundedSender;

use crate::commands::progress_channel;
use crate::running::RunningGame;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub version_id: String,
    pub files: usize,
    pub bytes: u64,
    pub instance: InstanceConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchResult {
    pub instance_id: String,
    pub pid: u32,
    pub log_file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopResult {
    pub instance_id: String,
    /// `None` when the instance was not running.
    pub pid: Option<u32>,
}

#[tauri::command]
pub async fn instance_install(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<InstallResult, String> {
    let tx = progress_channel(&app, "install://progress");
    install_inner(&state, &id, Some(tx)).await
}

/// Shared by "install instance" and "install modpack".
pub(crate) async fn install_inner(
    state: &AppState,
    id: &str,
    tx: Option<UnboundedSender<DownloadEvent>>,
) -> Result<InstallResult, String> {
    let mut config = InstanceConfig::load(&state.paths, id).map_err(|e| e.to_string())?;

    // Repair rewrites files the running JVM has mapped open; refuse instead of
    // producing a half-updated install.
    if let Some(game) = state.running.find(id) {
        return Err(format!(
            "{} is running (pid {}) — stop the game before installing",
            config.name, game.pid
        ));
    }

    // 1. Vanilla files are shared by every instance and verified by hash.
    let vanilla = state
        .installer
        .install_vanilla(&config.game.version, tx.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 2. Loader profile, if this instance uses one.
    let version_id = if config.game.loader.is_modded() {
        let required = required_java_major(&state.paths, &config.game.version);
        let runtime = resolve_java(state, &config, required).await?;
        state
            .installer
            .install_loader(
                &config.game.version,
                config.game.loader,
                config.game.loader_version.as_deref(),
                &runtime.path,
                tx.clone(),
            )
            .await
            .map_err(|e| e.to_string())?
    } else {
        config.game.version.clone()
    };

    // 3. Anything the merged profile still references is fetched here. This is
    //    the repair path for loader libraries, which vanilla installs never see.
    let repaired = state
        .installer
        .ensure_version_files(&version_id, tx)
        .await
        .map_err(|e| e.to_string())?;

    if matches!(
        config.game.loader,
        LoaderKind::Forge | LoaderKind::NeoForge
    ) {
        tracing::info!(loader = %config.game.loader, "forge-like loader installed");
    }

    config.game.version_id = Some(version_id.clone());
    config.save_touched(&state.paths).map_err(|e| e.to_string())?;

    Ok(InstallResult {
        version_id,
        files: vanilla.files + repaired.files,
        bytes: vanilla.bytes + repaired.bytes,
        instance: config,
    })
}

#[tauri::command]
pub async fn instance_launch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<LaunchResult, String> {
    let mut config = InstanceConfig::load(&state.paths, &id).map_err(|e| e.to_string())?;
    if let Some(game) = state.running.find(&id) {
        return Err(format!(
            "{} is already running (pid {}) — press Stop first",
            config.name, game.pid
        ));
    }
    let version = merged_for_instance(&state.paths, &config).map_err(|e| e.to_string())?;
    // Self-heal: a profile can be installed while its loader libraries are not
    // (Fabric/Quilt Maven artifacts, Forge stragglers). Fetch before planning.
    let resolved_id = config
        .game
        .version_id
        .clone()
        .unwrap_or_else(|| config.game.version.clone());
    state
        .installer
        .ensure_version_files(&resolved_id, None)
        .await
        .map_err(|e| e.to_string())?;
    let runtime = resolve_java(&state, &config, version.java_major).await?;

    let session = crate::commands::active_session(&state).await?;
    let offline = session.tokens.is_none();
    let account = Account {
        username: session.record.username.clone(),
        uuid: session.record.uuid.clone(),
        // Offline launches still need a token-shaped string; the client does
        // not validate it, and no session server is contacted.
        access_token: session
            .tokens
            .as_ref()
            .map(|tokens| tokens.access_token.clone())
            .unwrap_or_else(|| "0".to_string()),
        xuid: session
            .record
            .xuid
            .clone()
            .or_else(|| session.tokens.as_ref().and_then(|tokens| tokens.xuid.clone())),
        user_type: if offline { "legacy" } else { "msa" }.to_string(),
    };
    let version_id = config
        .game
        .version_id
        .clone()
        .ok_or_else(|| "instance is not installed yet".to_string())?;

    let plan = build_plan(LaunchRequest {
        paths: &state.paths,
        instance: &config,
        account: &account,
        java: &runtime,
        version: &version,
        version_id: &version_id,
    })
    .map_err(|e| e.to_string())?;

    let log = std::fs::File::create(&plan.log_file)
        .map_err(|e| format!("cannot create game log: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| format!("cannot clone game log handle: {e}"))?;

    let mut command = std::process::Command::new(&plan.program);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(log_err));
    for (key, value) in &plan.env {
        command.env(key, value);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the game owns its window, the launcher stays quiet.
        command.creation_flags(0x0800_0000);
    }

    let child = command
        .spawn()
        .map_err(|e| format!("cannot start Java ({}): {e}", plan.program.display()))?;
    let log_file = plan.log_file.display().to_string();
    let running = state
        .running
        .register(&id, child, log_file.clone())
        .map_err(|e| e.to_string())?;

    config.last_played_at = Some(Utc::now());
    config.save_touched(&state.paths).map_err(|e| e.to_string())?;

    // The game owns its window; the launcher gets out of the way when asked,
    // but stays alive so Stop is one click away.
    hand_off_display(app.clone(), state.settings().close_on_launch);

    watch_for_exit(app.clone(), id.clone());
    // Tail from the first frame: the launch banner is exactly what a crash
    // report needs, and it is already written by the time the UI can ask.
    let _ = crate::commands::console::start(&app, &state, &id);

    state.presence(Some(crate::discord::playing(
        &config.name,
        config.game.loader.as_str(),
        &config.game.version,
    )));
    // Friends see the launch immediately instead of waiting for the 45 s tick.
    let _ = crate::commands::friends::publish(&state);

    let _ = app.emit("game://changed", ());

    Ok(LaunchResult {
        instance_id: id,
        pid: running.pid,
        log_file,
    })
}

/// Live games, including ones that have already exited on their own — those
/// are reaped before the answer is built, so the UI never shows a dead `Stop`.
#[tauri::command]
pub fn instances_running(state: State<'_, AppState>) -> Vec<RunningGame> {
    state.running.list()
}

/// Move the launcher out of the game's way: leave fullscreen (two fullscreen
/// windows on one display is how a launcher ends up painted over Minecraft's
/// first frame), then minimize once the window manager has finished the
/// restyle.
///
/// The two steps are split across a short delay on purpose: `set_fullscreen`
/// only *schedules* a style change, and a `minimize` issued in the same tick
/// gets dropped while that restyle is pending — the window stays up behind the
/// game instead of stepping aside.
fn hand_off_display(app: AppHandle, minimize: bool) {
    tauri::async_runtime::spawn(async move {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };

        if window.is_fullscreen().unwrap_or(false) {
            let _ = window.set_fullscreen(false);
            let _ = app.emit("window://fullscreen", false);
            tokio::time::sleep(std::time::Duration::from_millis(240)).await;
        }

        if minimize {
            let _ = window.minimize();
        }
    });
}

/// Watch one game until it leaves the registry, then hand the display back.
///
/// The registry only reaps on poll, so without this the window would come back
/// up to a second and a half late and the UI would show a stale `Stop`. The
/// watcher owns no child handle — it just asks the registry, which is what
/// makes it safe to run alongside `instance_stop`.
fn watch_for_exit(app: AppHandle, instance_id: String) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if app.state::<AppState>().running.find(&instance_id).is_some() {
                continue;
            }

            if app.state::<AppState>().settings().restore_on_exit {
                if let Some(window) = app.get_webview_window("main") {
                    // Unconditional: `is_minimized` can still report the state
                    // from before the hand-off, and a stray `unminimize` on an
                    // already-visible window is a no-op.
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            let _ = app.emit("game://changed", ());
            app.state::<AppState>()
                .presence(Some(crate::discord::idle()));
            // Back to idle in the friends folder right away, not on the next tick.
            let _ = crate::commands::friends::publish(&app.state::<AppState>());
            break;
        }
    });
}

/// Stop a game. The blocking part posts `WM_CLOSE` first so Minecraft saves,
/// then force-kills after ten seconds; a blocking pool keeps the UI thread free.
#[tauri::command]
pub async fn instance_stop(app: AppHandle, id: String) -> Result<StopResult, String> {
    let handle = app.clone();
    let target = id.clone();
    let pid = tauri::async_runtime::spawn_blocking(move || {
        handle.state::<AppState>().running.stop(&target)
    })
    .await
    .map_err(|e| format!("stop task failed: {e}"))??;

    let _ = app.emit("game://changed", ());
    Ok(StopResult {
        instance_id: id,
        pid,
    })
}

#[tauri::command]
pub fn instance_required_java(
    state: State<'_, AppState>,
    mc_version: String,
) -> u32 {
    required_java_major(&state.paths, &mc_version)
}

pub(crate) fn required_java_major(paths: &launcher_core::LauncherPaths, mc_version: &str) -> u32 {
    load_version_json(paths, mc_version)
        .ok()
        .and_then(|version| version.java_version.map(|j| j.major_version))
        .unwrap_or_else(|| java::guess_major(mc_version))
}

async fn resolve_java(
    state: &AppState,
    config: &InstanceConfig,
    required: u32,
) -> Result<JavaRuntime, String> {
    let mut runtimes = java::discover(&state.paths).await;

    if config.java.mode == JavaMode::Manual {
        if let Some(path) = config.java.path.as_ref() {
            let known = runtimes.iter().any(|r| same_path(&r.path, path));
            if !known {
                if let Some(runtime) = java::inspect(path).await {
                    runtimes.push(runtime);
                }
            }
        }
    }

    if let Some(found) = java::select(&runtimes, required, &config.java).cloned() {
        return Ok(found);
    }

    // Nothing usable is installed. Fetching the exact feature release is far
    // friendlier than bouncing the user to a vendor page mid-click, and the
    // managed runtime is invisible afterwards: `java::discover` scans it like
    // any other JRE.
    if config.java.mode == JavaMode::Auto && state.settings().auto_install_java {
        tracing::info!(required, "no matching Java runtime — downloading Temurin JRE");
        let installed = crate::java_install::install(&state.http(), &state.paths, required)
            .await
            .map_err(|e| {
                format!(
                    "no Java {required}+ runtime found and the automatic download failed: {e}. \
                     Install one manually or set a path in instance settings."
                )
            })?;
        if let Some(runtime) = java::inspect(&installed.path).await {
            runtimes.push(runtime);
            if let Some(found) = java::select(&runtimes, required, &config.java).cloned() {
                return Ok(found);
            }
        }
        return Err(format!(
            "the downloaded Java {} could not be started ({})",
            installed.major,
            installed.path.display()
        ));
    }

    Err(format!(
        "no Java {required}+ runtime found. Install one or set a manual path in instance settings."
    ))
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
