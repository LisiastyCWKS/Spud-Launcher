//! Optional AI crash analysis.
//!
//! Three gates sit between a crash report and the network, in this order:
//!
//! 1. the feature is off until the user turns it on (`ai_enabled`);
//! 2. every single request carries an explicit `consent = true` from the UI —
//!    there is no "remember my choice", because a log analysed today is not a
//!    log the user agreed to send tomorrow;
//! 3. the text is redacted before it is handed to `launcher_ai`, so paths,
//!    tokens, UUIDs and e-mails never reach the provider.
//!
//! The offline layer always runs. A user with no provider configured still gets
//! the rule-based diagnoses; the AI text is an extra paragraph, never the only
//! answer.
//!
//! The API key never touches `settings.json`: it lives in the OS keyring under
//! the same service name as the Microsoft tokens, so a user has one entry to
//! revoke.

use std::time::Duration;

use launcher_ai::{analyze, redact, AiConfig, LocalAnalysis, RedactionReport};
use launcher_core::settings::AiProvider;
use serde::Serialize;
use tauri::State;

use crate::commands::console::read_capped;
use crate::state::AppState;

/// Must match `launcher_auth::store::KEYRING_SERVICE`; duplicated as a literal
/// so this module does not depend on the auth crate just for a string.
const KEYRING_SERVICE: &str = "spud-launcher";
const KEYRING_USER: &str = "ai-api-key";
/// Ceiling for the file read. The analyser itself only looks at the tail, but
/// the redactor needs the whole text to strip paths from the middle of a log.
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
/// A gateway that hangs must not hold the diagnostics panel hostage.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
/// Characters of the redacted log shown as the consent preview.
const PREVIEW_CHARS: usize = 1800;

#[derive(Debug, Clone, Serialize)]
pub struct AiStatus {
    pub enabled: bool,
    pub provider: AiProvider,
    pub endpoint: String,
    pub model: String,
    pub max_log_kb: u32,
    pub has_key: bool,
    /// `false` when the OS has no credential store at all (headless Linux
    /// without a Secret Service). The panel explains it instead of pretending
    /// the key was saved.
    pub keyring: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrashAnalysis {
    /// Rule-based result. Always present.
    pub local: LocalAnalysis,
    /// Exactly what the provider would receive, trimmed for display. Only built
    /// when the feature is on and consent has not been given yet.
    pub preview: Option<String>,
    /// What the redactor removed, so the consent screen can say "3 tokens".
    pub redaction: Option<RedactionReport>,
    /// Provider answer, Polish, plain text.
    pub ai: Option<String>,
    /// Why the AI part did not happen. The local part is still valid.
    pub ai_error: Option<String>,
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| format!("no OS keyring available: {error}"))
}

fn stored_key() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            Ok((!trimmed.is_empty()).then_some(trimmed))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn status_for(state: &AppState) -> AiStatus {
    let settings = state.settings();
    let (has_key, keyring) = match stored_key() {
        Ok(Some(_)) => (true, true),
        Ok(None) => (false, true),
        Err(_) => (false, false),
    };
    AiStatus {
        enabled: settings.ai_enabled,
        provider: settings.ai_provider,
        endpoint: settings.ai_endpoint,
        model: settings.ai_model,
        max_log_kb: settings.ai_max_log_kb,
        has_key,
        keyring,
    }
}

#[tauri::command]
pub fn ai_status(state: State<'_, AppState>) -> AiStatus {
    status_for(&state)
}

/// Store the provider key in the OS keyring. The value is never echoed back,
/// never logged and never written to `settings.json`.
#[tauri::command]
pub fn ai_set_key(state: State<'_, AppState>, key: String) -> Result<AiStatus, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("the API key is empty".to_string());
    }
    if key.len() > 4096 {
        return Err("that does not look like an API key".to_string());
    }
    entry()?
        .set_password(key)
        .map_err(|error| format!("cannot write to the OS keyring: {error}"))?;
    Ok(status_for(&state))
}

#[tauri::command]
pub fn ai_clear_key(state: State<'_, AppState>) -> Result<AiStatus, String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(status_for(&state)),
        Err(error) => Err(format!(
            "cannot remove the key from the OS keyring: {error}"
        )),
    }
}

/// Analyse one log or crash report.
///
/// `consent` is the whole point of this command's shape: the UI calls it twice.
/// The first call (`consent: false`) returns the offline diagnoses plus a
/// preview of the redacted text; the second call, after the user presses the
/// confirm button, repeats the work and adds the provider answer.
#[tauri::command]
pub async fn ai_analyze_crash(
    state: State<'_, AppState>,
    instance_id: String,
    name: String,
    consent: bool,
) -> Result<CrashAnalysis, String> {
    launcher_core::instance::validate_id(&instance_id).map_err(|error| error.to_string())?;
    let game_dir = state.paths.game_dir(&instance_id);
    let relative = name.trim_start_matches(".minecraft/");
    let path = launcher_core::instance::safe_join(&game_dir, relative)
        .map_err(|error| error.to_string())?;
    // Same fence as `instance_log_read`: only the two folders Minecraft writes
    // evidence into can be analysed, whatever path the UI hands over.
    if !path.starts_with(game_dir.join("logs"))
        && !path.starts_with(game_dir.join("crash-reports"))
    {
        return Err("only logs and crash reports can be analysed".to_string());
    }

    let log = tauri::async_runtime::spawn_blocking(move || read_capped(&path, MAX_LOG_BYTES))
        .await
        .map_err(|error| format!("reading the log failed: {error}"))??;

    let local = analyze(&log);
    let settings = state.settings();

    if !settings.ai_enabled {
        return Ok(CrashAnalysis {
            local,
            preview: None,
            redaction: None,
            ai: None,
            ai_error: Some(
                "Analiza AI jest wyłączona. Włącz ją w Settings → AI, żeby dostać wyjaśnienie modelu."
                    .to_string(),
            ),
        });
    }

    let (clean, report) = redact(&log);

    // The key is read once, before the consent branch: without it the preview
    // would promise a request that can never happen.
    let key_state = stored_key();

    if !consent {
        let preview: String = clean.chars().take(PREVIEW_CHARS).collect();
        let ai_error = match &key_state {
            Ok(Some(_)) => None,
            Ok(None) => Some(
                "Brak klucza API dla tego providera — wklej go w Settings → AI.".to_string(),
            ),
            Err(error) => Some(format!("Nie udało się odczytać klucza API: {error}")),
        };
        let ready = ai_error.is_none();
        return Ok(CrashAnalysis {
            local,
            preview: ready.then_some(preview),
            redaction: Some(report),
            ai: None,
            ai_error,
        });
    }

    let api_key = match key_state {
        Ok(Some(key)) => key,
        Ok(None) => {
            return Ok(CrashAnalysis {
                local,
                preview: None,
                redaction: Some(report),
                ai: None,
                ai_error: Some(
                    "Brak klucza API dla tego providera — wklej go w Settings → AI.".to_string(),
                ),
            })
        }
        Err(error) => {
            return Ok(CrashAnalysis {
                local,
                preview: None,
                redaction: Some(report),
                ai: None,
                ai_error: Some(format!("Nie udało się odczytać klucza API: {error}")),
            })
        }
    };

    let config = AiConfig {
        provider: settings.ai_provider,
        endpoint: settings.ai_endpoint.clone(),
        model: settings.ai_model.clone(),
        api_key: Some(api_key),
        max_log_bytes: (settings.ai_max_log_kb as usize) * 1024,
    };
    let http = state.http();

    match tokio::time::timeout(
        REQUEST_TIMEOUT,
        launcher_ai::provider::explain(&http, &config, &clean, &local.diagnoses),
    )
    .await
    {
        Ok(Ok(text)) => Ok(CrashAnalysis {
            local,
            preview: None,
            redaction: Some(report),
            ai: Some(text),
            ai_error: None,
        }),
        Ok(Err(error)) => Ok(CrashAnalysis {
            local,
            preview: None,
            redaction: Some(report),
            ai: None,
            ai_error: Some(format!("Provider nie odpowiedział: {error}")),
        }),
        Err(_) => Ok(CrashAnalysis {
            local,
            preview: None,
            redaction: Some(report),
            ai: None,
            ai_error: Some(format!(
                "Provider nie odpowiedział w {} s — analiza offline powyżej nadal obowiązuje.",
                REQUEST_TIMEOUT.as_secs()
            )),
        }),
    }
}
