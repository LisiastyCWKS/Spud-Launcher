use chrono::{Duration as ChronoDuration, Utc};
use launcher_auth::{AccountKind, AccountRecord, DeviceCode};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AccountView {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub active: bool,
    pub kind: AccountKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountsView {
    pub accounts: Vec<AccountView>,
    pub active: Option<String>,
}

fn view(record: &AccountRecord, active: Option<&str>) -> AccountView {
    AccountView {
        id: record.id.clone(),
        username: record.username.clone(),
        uuid: record.uuid.clone(),
        active: active == Some(record.id.as_str()),
        kind: record.kind,
    }
}

#[tauri::command]
pub fn accounts_list(state: State<'_, AppState>) -> Result<AccountsView, String> {
    let file = state.accounts.load().map_err(|e| e.to_string())?;
    let active = file.active.clone();
    Ok(AccountsView {
        accounts: file
            .accounts
            .iter()
            .map(|record| view(record, active.as_deref()))
            .collect(),
        active,
    })
}

#[tauri::command]
pub fn auth_status(state: State<'_, AppState>) -> AuthStatus {
    let (source, id) = state.client_id_source();
    AuthStatus {
        configured: !id.is_empty(),
        source,
        client_id: (!id.is_empty()).then_some(id),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub configured: bool,
    /// `env`, `settings` or `none`.
    pub source: &'static str,
    /// Public identifier, shown for debugging — never a secret.
    pub client_id: Option<String>,
}

#[tauri::command]
pub async fn account_begin_login(state: State<'_, AppState>) -> Result<DeviceCode, String> {
    state
        .auth()
        .start_device_code()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn account_finish_login(
    state: State<'_, AppState>,
    device: DeviceCode,
) -> Result<AccountView, String> {
    let result = state
        .auth()
        .poll_device_code(&device)
        .await
        .map_err(|e| e.to_string())?;

    let id = result.profile.uuid_dashed();
    state
        .vault
        .save(&id, &result.tokens)
        .map_err(|e| e.to_string())?;
    let record = state
        .accounts
        .upsert(&result.profile.name, &id, result.tokens.xuid.clone())
        .map_err(|e| e.to_string())?;
    Ok(view(&record, Some(&record.id)))
}

#[tauri::command]
pub fn account_add_offline(
    state: State<'_, AppState>,
    username: String,
) -> Result<AccountsView, String> {
    state
        .accounts
        .add_offline(&username)
        .map_err(|e| e.to_string())?;
    accounts_list(state)
}

#[tauri::command]
pub fn account_remove(state: State<'_, AppState>, id: String) -> Result<AccountsView, String> {
    let kind = state
        .accounts
        .load()
        .map_err(|e| e.to_string())?
        .accounts
        .iter()
        .find(|account| account.id == id)
        .map(|account| account.kind);
    state.accounts.remove(&id).map_err(|e| e.to_string())?;
    // Offline accounts never had a keyring entry.
    if kind == Some(AccountKind::Microsoft) {
        state.vault.delete(&id).map_err(|e| e.to_string())?;
    }
    accounts_list(state)
}

#[tauri::command]
pub fn account_set_active(
    state: State<'_, AppState>,
    id: String,
) -> Result<AccountsView, String> {
    state.accounts.set_active(&id).map_err(|e| e.to_string())?;
    accounts_list(state)
}

/// Active account plus tokens when the account is a Microsoft one. Offline
/// accounts carry no tokens by design.
pub(crate) struct ActiveSession {
    pub record: AccountRecord,
    pub tokens: Option<launcher_auth::StoredTokens>,
}

pub(crate) async fn active_session(state: &AppState) -> Result<ActiveSession, String> {
    let file = state.accounts.load().map_err(|e| e.to_string())?;
    let record = file
        .active()
        .cloned()
        .ok_or_else(|| "no active account — add a Microsoft or offline account first".to_string())?;

    if record.kind == AccountKind::Offline {
        return Ok(ActiveSession {
            record,
            tokens: None,
        });
    }

    let tokens = state
        .vault
        .load(&record.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "stored session is missing — sign in again".to_string())?;

    if tokens.expires_at <= Utc::now() + ChronoDuration::seconds(60) {
        let refreshed = state
            .auth()
            .refresh(&tokens)
            .await
            .map_err(|e| format!("could not refresh the session: {e}"))?;
        state
            .vault
            .save(&record.id, &refreshed.tokens)
            .map_err(|e| e.to_string())?;
        if refreshed.profile.name != record.username {
            state
                .accounts
                .upsert(&refreshed.profile.name, &record.id, refreshed.tokens.xuid.clone())
                .map_err(|e| e.to_string())?;
        }
        return Ok(ActiveSession {
            record,
            tokens: Some(refreshed.tokens),
        });
    }

    Ok(ActiveSession {
        record,
        tokens: Some(tokens),
    })
}
