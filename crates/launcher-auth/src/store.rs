use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use launcher_core::paths::LauncherPaths;

use crate::microsoft::StoredTokens;
use crate::{AuthError, IoContext, Result};

pub const KEYRING_SERVICE: &str = "spud-launcher";

/// Tokens never touch `accounts.json`: they live in Windows Credential Manager,
/// macOS Keychain or the Secret Service.
#[derive(Clone)]
pub struct TokenVault {
    service: String,
}

impl Default for TokenVault {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenVault {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE.to_string(),
        }
    }

    pub fn save(&self, account_id: &str, tokens: &StoredTokens) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, account_id)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        let payload = serde_json::to_string(tokens)?;
        entry
            .set_password(&payload)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        Ok(())
    }

    pub fn load(&self, account_id: &str) -> Result<Option<StoredTokens>> {
        let entry = keyring::Entry::new(&self.service, account_id)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(payload) => {
                let tokens = serde_json::from_str(&payload)?;
                Ok(Some(tokens))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(AuthError::Keyring(err.to_string())),
        }
    }

    pub fn delete(&self, account_id: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, account_id)
            .map_err(|e| AuthError::Keyring(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(AuthError::Keyring(err.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    #[default]
    Microsoft,
    /// Nickname-only account for offline play. No tokens, no keyring entry.
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    /// Dashed UUID; also the keyring entry name.
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub added_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    #[serde(default)]
    pub xuid: Option<String>,
    #[serde(default)]
    pub kind: AccountKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsFile {
    #[serde(default = "schema")]
    pub schema: u32,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub accounts: Vec<AccountRecord>,
}

fn schema() -> u32 {
    1
}

impl Default for AccountsFile {
    fn default() -> Self {
        Self {
            schema: schema(),
            active: None,
            accounts: Vec::new(),
        }
    }
}

impl AccountsFile {
    pub fn active(&self) -> Option<&AccountRecord> {
        let id = self.active.as_deref()?;
        self.accounts.iter().find(|a| a.id == id)
    }
}

/// Metadata index for all signed-in accounts.
pub struct AccountStore {
    path: PathBuf,
}

impl AccountStore {
    pub fn new(paths: &LauncherPaths) -> Self {
        Self {
            path: paths.accounts_file(),
        }
    }

    pub fn load(&self) -> Result<AccountsFile> {
        if !self.path.is_file() {
            return Ok(AccountsFile::default());
        }
        let raw = std::fs::read(&self.path).at(&self.path)?;
        Ok(serde_json::from_slice(&raw)?)
    }

    pub fn save(&self, accounts: &AccountsFile) -> Result<()> {
        write_atomic(&self.path, accounts)
    }

    /// Insert or update by UUID and make it the active account.
    pub fn upsert(&self, username: &str, uuid_dashed: &str, xuid: Option<String>) -> Result<AccountRecord> {
        let mut file = self.load()?;
        let now = Utc::now();
        let record = match file.accounts.iter_mut().find(|a| a.id == uuid_dashed) {
            Some(existing) => {
                existing.username = username.to_string();
                existing.last_used_at = now;
                existing.xuid = xuid;
                existing.clone()
            }
            None => {
                let record = AccountRecord {
                    id: uuid_dashed.to_string(),
                    username: username.to_string(),
                    uuid: uuid_dashed.to_string(),
                    added_at: now,
                    last_used_at: now,
                    xuid,
                    kind: AccountKind::Microsoft,
                };
                file.accounts.push(record.clone());
                record
            }
        };
        file.active = Some(record.id.clone());
        self.save(&file)?;
        Ok(record)
    }

    /// Add or refresh a nickname-only account (offline mode). The UUID is the
    /// vanilla `OfflinePlayer` UUID so worlds and servers see a stable identity.
    pub fn add_offline(&self, username: &str) -> Result<AccountRecord> {
        let name = username.trim();
        validate_offline_name(name)?;
        let uuid = offline_uuid(name);
        let mut file = self.load()?;
        let now = Utc::now();

        let record = match file.accounts.iter_mut().find(|a| {
            a.kind == AccountKind::Offline && a.username.eq_ignore_ascii_case(name)
        }) {
            Some(existing) => {
                existing.username = name.to_string();
                existing.last_used_at = now;
                existing.clone()
            }
            None => {
                let record = AccountRecord {
                    id: uuid.clone(),
                    username: name.to_string(),
                    uuid,
                    added_at: now,
                    last_used_at: now,
                    xuid: None,
                    kind: AccountKind::Offline,
                };
                file.accounts.push(record.clone());
                record
            }
        };

        file.active = Some(record.id.clone());
        self.save(&file)?;
        Ok(record)
    }

    pub fn remove(&self, account_id: &str) -> Result<()> {
        let mut file = self.load()?;
        file.accounts.retain(|a| a.id != account_id);
        if file.active.as_deref() == Some(account_id) {
            file.active = file.accounts.first().map(|a| a.id.clone());
        }
        self.save(&file)
    }

    pub fn set_active(&self, account_id: &str) -> Result<()> {
        let mut file = self.load()?;
        if !file.accounts.iter().any(|a| a.id == account_id) {
            return Err(AuthError::Keyring(format!("unknown account {account_id}")));
        }
        file.active = Some(account_id.to_string());
        self.save(&file)
    }
}

/// Nickname rules follow the vanilla client: 3–16 characters, ASCII letters,
/// digits and underscore. Anything else is rejected before it reaches a world.
pub fn validate_offline_name(name: &str) -> Result<()> {
    let valid = (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(AuthError::OfflineName(name.to_string()))
    }
}

/// UUID v3 over `OfflinePlayer:<name>` — the exact algorithm the vanilla
/// server uses for offline-mode players, so the same name always maps to the
/// same UUID on every machine.
pub fn offline_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java_name_uuid_layout() {
        let uuid = offline_uuid("siurek");
        assert_eq!(uuid.len(), 36);
        // Java's UUID.nameUUIDFromBytes marks the digest as version 3 and
        // variant 2 (RFC 4122); the vanilla server does the same.
        assert_eq!(&uuid[14..15], "3");
        assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn offline_uuid_is_stable_per_name() {
        assert_eq!(offline_uuid("siurek"), offline_uuid("siurek"));
        assert_ne!(offline_uuid("siurek"), offline_uuid("Siurek"));
    }

    #[test]
    fn offline_name_rules_match_vanilla() {
        assert!(validate_offline_name("siurek").is_ok());
        assert!(validate_offline_name("a_b1234567890123").is_ok());
        assert!(validate_offline_name("ab").is_err());
        assert!(validate_offline_name("a b").is_err());
        assert!(validate_offline_name("siurek!").is_err());
        assert!(validate_offline_name("0123456789abcdefg").is_err());
    }
}

fn write_atomic(path: &Path, value: &AccountsFile) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AuthError::Keyring(format!("invalid path {}", path.display())))?;
    std::fs::create_dir_all(parent).at(parent)?;
    let tmp = parent.join(".accounts.json.tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(&tmp, &bytes).at(&tmp)?;
    std::fs::rename(&tmp, path).at(path)?;
    Ok(())
}
