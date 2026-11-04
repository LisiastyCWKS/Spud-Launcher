use chrono::Utc;
use launcher_core::hardware::{self, HardwareProfile, Tuning};
use launcher_core::instance::{InstanceConfig, TuningState};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// The machine this launcher is running on, as far as it can be read.
#[tauri::command]
pub async fn hardware_profile() -> Result<HardwareProfile, String> {
    Ok(tauri::async_runtime::spawn_blocking(HardwareProfile::detect)
        .await
        .map_err(|e| format!("hardware scan failed: {e}"))?)
}

#[derive(Debug, Clone, Serialize)]
pub struct TuningPreview {
    pub instance_id: String,
    pub profile: HardwareProfile,
    pub tuning: Tuning,
    pub modded: bool,
    pub java_major: u32,
    /// True when a previous run wrote a plan and the machine has not changed.
    pub current: bool,
    pub applied_at: Option<chrono::DateTime<Utc>>,
    /// Existing flags, so the UI can show "recommended" next to "current".
    pub current_jvm_args: Vec<String>,
}

/// Compute what the tuner *would* apply. Read-only: nothing is written until
/// the user presses Apply.
#[tauri::command]
pub async fn tuning_preview(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<TuningPreview, String> {
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let profile = tauri::async_runtime::spawn_blocking(HardwareProfile::detect)
        .await
        .map_err(|e| format!("hardware scan failed: {e}"))?;
    let java_major = crate::commands::required_java_major(&state.paths, &config.game.version);
    let modded = config.game.loader.is_modded();
    let tuning = hardware::recommend(&profile, modded, java_major);

    let current = config
        .tuning
        .as_ref()
        .map(|applied| {
            applied.cpu_threads == profile.cpu_threads
                && applied.memory_total_mb == profile.memory_total_mb
                && applied.modded == modded
                && applied.java_major == java_major
                && applied.max_memory_mb == tuning.max_memory_mb
        })
        .unwrap_or(false);

    Ok(TuningPreview {
        instance_id,
        profile,
        tuning,
        modded,
        java_major,
        current,
        applied_at: config.tuning.as_ref().map(|applied| applied.applied_at),
        current_jvm_args: config.jvm_args.clone(),
    })
}

/// Write the plan into `instance.json` and refresh the flags.
///
/// Heap goes into the launcher-owned `-Xms`/`-Xmx` slots; GC flags go into
/// `jvm_args`, which the UI keeps editable afterwards. Existing user flags that
/// are not part of the plan are preserved — the tuner adds, never erases.
#[tauri::command]
pub async fn tuning_apply(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<InstanceConfig, String> {
    let mut config =
        InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let profile = tauri::async_runtime::spawn_blocking(HardwareProfile::detect)
        .await
        .map_err(|e| format!("hardware scan failed: {e}"))?;
    let java_major = crate::commands::required_java_major(&state.paths, &config.game.version);
    let modded = config.game.loader.is_modded();
    let plan = hardware::recommend(&profile, modded, java_major);

    config.java.min_memory_mb = plan.min_memory_mb;
    config.java.max_memory_mb = plan.max_memory_mb;
    config.jvm_args = hardware::merge_jvm_args(&config.jvm_args, &plan.jvm_args);
    config.tuning = Some(TuningState {
        applied_at: Utc::now(),
        cpu_threads: profile.cpu_threads,
        memory_total_mb: profile.memory_total_mb,
        modded,
        java_major,
        max_memory_mb: plan.max_memory_mb,
    });

    config.save_touched(&state.paths).map_err(|e| e.to_string())?;
    Ok(config)
}
