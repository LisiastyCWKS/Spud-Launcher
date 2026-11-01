//! Friends: presence and instance sharing between people running Spud.
//!
//! There is no Spud server behind this — presence is a small JSON file the
//! launcher rewrites while it runs. Point both sides at a folder that syncs
//! (OneDrive, Syncthing, a network share) and each launcher publishes
//! `<nick>.json` there while reading everyone else's.
//!
//! The file carries what a friend actually wants to know: what you are playing,
//! which world/server you are on, and the instance list you are willing to
//! hand over. Instances themselves travel as the same `zip` the transfer
//! module already produces, dropped into the shared `archives/` folder.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use launcher_core::instance::InstanceConfig;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::transfer::ImportResult;
use crate::state::AppState;

/// A friend counts as online when their file was rewritten within this window.
const ONLINE_WINDOW_MINUTES: i64 = 5;
const PUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(45);

const PRESENCE_APP: &str = concat!("spud-launcher/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FriendsConfig {
    /// Present as this nick. Empty means "not sharing yet".
    pub nick: String,
    /// Synced folder both sides can read and write.
    pub sync_folder: Option<String>,
    /// Registry shared by every friend you added.
    pub friends: Vec<String>,
}

impl Default for FriendsConfig {
    fn default() -> Self {
        Self {
            nick: String::new(),
            sync_folder: None,
            friends: Vec::new(),
        }
    }
}

impl FriendsConfig {
    fn sanitise(&mut self) {
        self.nick = self.nick.trim().to_string();
        self.sync_folder = self
            .sync_folder
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.friends = self
            .friends
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value.len() <= 32)
            .collect();
        self.friends.sort();
        self.friends.dedup();
    }

    fn folder(&self) -> Option<PathBuf> {
        self.sync_folder.clone().map(PathBuf::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SharedInstance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub mods: usize,
}

impl Default for SharedInstance {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            mc_version: String::new(),
            loader: "vanilla".to_string(),
            loader_version: None,
            mods: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PresenceFile {
    pub nick: String,
    pub app: String,
    pub updated_at: String,
    /// `playing` while a game is live, `idle` in the launcher.
    pub status: String,
    /// Server address parsed out of the game log, when the player is on one.
    pub server: Option<String>,
    pub instance: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    pub instances: Vec<SharedInstance>,
}

impl Default for PresenceFile {
    fn default() -> Self {
        Self {
            nick: String::new(),
            app: PRESENCE_APP.to_string(),
            updated_at: Utc::now().to_rfc3339(),
            status: "idle".to_string(),
            server: None,
            instance: None,
            mc_version: None,
            loader: None,
            instances: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]

pub struct FriendView {
    pub nick: String,
    pub online: bool,
    pub status: String,
    pub server: Option<String>,
    pub instance: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    pub updated_at: Option<String>,
    pub instances: Vec<SharedInstance>,
    pub archives: Vec<SharedArchive>,
}

#[derive(Debug, Clone, Serialize)]

pub struct SharedArchive {
    pub nick: String,
    pub instance: String,
    pub file_name: String,
    pub path: String,
    pub bytes: u64,
}

fn config_path(state: &AppState) -> PathBuf {
    state.paths.root().join("friends.json")
}

fn load_config(state: &AppState) -> FriendsConfig {
    let path = config_path(state);
    let mut config: FriendsConfig = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    config.sanitise();
    config
}

fn save_config(state: &AppState, config: &FriendsConfig) -> Result<(), String> {
    let raw = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    let path = config_path(state);
    std::fs::write(&path, raw).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[tauri::command]
pub fn friends_get(state: State<'_, AppState>) -> FriendsConfig {
    load_config(&state)
}

#[tauri::command]
pub fn friends_save(
    state: State<'_, AppState>,
    config: FriendsConfig,
) -> Result<FriendsConfig, String> {
    let mut config = config;
    config.sanitise();
    if config.nick.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("a nick cannot contain filesystem characters".to_string());
    }
    save_config(&state, &config)?;
    Ok(config)
}

#[tauri::command]
pub fn friends_add(state: State<'_, AppState>, nick: String) -> Result<FriendsConfig, String> {
    let mut config = load_config(&state);
    let nick = nick.trim();
    if nick.is_empty() {
        return Err("type a nick first".to_string());
    }
    if nick.eq_ignore_ascii_case(&config.nick) {
        return Err("that is your own nick".to_string());
    }
    if !config.friends.iter().any(|value| value.eq_ignore_ascii_case(nick)) {
        config.friends.push(nick.to_string());
    }
    config.sanitise();
    save_config(&state, &config)?;
    Ok(config)
}

#[tauri::command]
pub fn friends_remove(state: State<'_, AppState>, nick: String) -> Result<FriendsConfig, String> {
    let mut config = load_config(&state);
    config.friends.retain(|value| !value.eq_ignore_ascii_case(nick.trim()));
    save_config(&state, &config)?;
    Ok(config)
}

/// Read one friend's presence file, if the shared folder has one.
fn read_presence(folder: &Path, nick: &str) -> Option<PresenceFile> {
    let path = folder.join(format!("{nick}.json"));
    let raw = std::fs::read_to_string(path).ok()?;
    let mut presence: PresenceFile = serde_json::from_str(&raw).ok()?;
    presence.instances.retain(|entry| !entry.name.is_empty());
    Some(presence)
}

fn archives_for(folder: &Path, nick: &str) -> Vec<SharedArchive> {
    let dir = folder.join("archives");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let prefix = format!("{nick}__");
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        let Some(rest) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        let instance = rest
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_string())
            .unwrap_or_else(|| rest.to_string());
        out.push(SharedArchive {
            nick: nick.to_string(),
            instance,
            file_name,
            path: path.display().to_string(),
            bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
        });
    }
    out.sort_by(|a, b| a.instance.cmp(&b.instance));
    out
}

#[tauri::command]
pub fn friends_list(state: State<'_, AppState>) -> Vec<FriendView> {
    let config = load_config(&state);
    let Some(folder) = config.folder() else {
        return Vec::new();
    };

    let now = Utc::now();
    config
        .friends
        .iter()
        .map(|nick| {
            let presence = read_presence(&folder, nick);
            let updated_at = presence
                .as_ref()
                .and_then(|entry| DateTime::parse_from_rfc3339(&entry.updated_at).ok())
                .map(|value| value.with_timezone(&Utc));
            let online = updated_at
                .map(|value| now - value < Duration::minutes(ONLINE_WINDOW_MINUTES))
                .unwrap_or(false);

            FriendView {
                nick: nick.clone(),
                online,
                status: presence
                    .as_ref()
                    .map(|entry| entry.status.clone())
                    .unwrap_or_else(|| "offline".to_string()),
                server: presence.as_ref().and_then(|entry| entry.server.clone()),
                instance: presence.as_ref().and_then(|entry| entry.instance.clone()),
                mc_version: presence.as_ref().and_then(|entry| entry.mc_version.clone()),
                loader: presence.as_ref().and_then(|entry| entry.loader.clone()),
                updated_at: updated_at.map(|value| value.to_rfc3339()),
                instances: presence
                    .as_ref()
                    .map(|entry| entry.instances.clone())
                    .unwrap_or_default(),
                archives: archives_for(&folder, nick),
            }
        })
        .collect()
}

/// Every archive currently sitting in the shared folder.
#[tauri::command]
pub fn friends_archives(state: State<'_, AppState>) -> Vec<SharedArchive> {
    let config = load_config(&state);
    let Some(folder) = config.folder() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for nick in &config.friends {
        out.extend(archives_for(&folder, nick));
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

/// Publish the current state. Called on a timer, after a launch, and when the
/// settings change — cheap enough that doing it often costs nothing.
#[tauri::command]
pub fn friends_publish(state: State<'_, AppState>) -> Result<PresenceFile, String> {
    publish(&state)
}

pub(crate) fn publish(state: &AppState) -> Result<PresenceFile, String> {
    let config = load_config(state);
    let Some(folder) = config.folder() else {
        return Ok(PresenceFile::default());
    };
    if config.nick.is_empty() {
        return Ok(PresenceFile::default());
    }

    let mut presence = PresenceFile {
        nick: config.nick.clone(),
        app: PRESENCE_APP.to_string(),
        updated_at: Utc::now().to_rfc3339(),
        status: "idle".to_string(),
        ..PresenceFile::default()
    };

    if let Ok(summaries) = launcher_core::instance::list_instances(&state.paths) {
        presence.instances = summaries
            .into_iter()
            .map(|summary| SharedInstance {
                id: summary.id,
                name: summary.name,
                mc_version: summary.mc_version,
                loader: summary.loader.as_str().to_string(),
                loader_version: summary.loader_version,
                mods: 0,
            })
            .collect();
    }

    // A live game turns presence into "Playing X on server Y".
    if let Some(game) = state.running.list().first() {
        if let Ok(instance) = InstanceConfig::load(&state.paths, &game.instance_id) {
            presence.status = "playing".to_string();
            presence.instance = Some(instance.name.clone());
            presence.mc_version = Some(instance.game.version.clone());
            presence.loader = Some(instance.game.loader.as_str().to_string());
            presence.server = server_from_log(&state.paths.game_dir(&instance.id));
            if let Some(entry) = presence
                .instances
                .iter_mut()
                .find(|entry| entry.id == instance.id)
            {
                if let Ok(manifest) = launcher_modrinth::installed::InstalledMods::load(
                    &state.paths.game_dir(&instance.id),
                ) {
                    entry.mods = manifest.mods.len();
                }
            }
        }
    }

    std::fs::create_dir_all(&folder).map_err(|e| {
        format!("cannot create the shared folder {}: {e}", folder.display())
    })?;
    let path = folder.join(format!("{}.json", presence.nick));
    let raw = serde_json::to_vec_pretty(&presence).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("cannot publish presence: {e}"))?;

    Ok(presence)
}

/// Last `Connecting to <host>` in the game log — the one line Minecraft always
/// writes when a player joins a server.
fn server_from_log(game_dir: &Path) -> Option<String> {
    for name in ["logs/game.log", "logs/latest.log"] {
        let path = game_dir.join(name);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let window = 256 * 1024u64;
        let start = metadata.len().saturating_sub(window);
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        use std::io::{Read, Seek, SeekFrom};
        if file.seek(SeekFrom::Start(start)).is_err() {
            continue;
        }
        let mut buffer = Vec::new();
        if file.read_to_end(&mut buffer).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&buffer);
        let mut found = None;
        for line in text.lines() {
            if let Some(index) = line.find("Connecting to ") {
                let rest = &line[index + "Connecting to ".len()..];
                let host = rest
                    .split([',', ' '])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches('.');
                if !host.is_empty() {
                    found = Some(host.to_string());
                }
            }
            if line.contains("Stopping!") || line.contains("Disconnecting") {
                found = None;
            }
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Export one of your instances into the shared folder so a friend can take it.
#[tauri::command]
pub async fn friends_share_instance(
    state: State<'_, AppState>,
    id: String,
) -> Result<SharedArchive, String> {
    let config = load_config(&state);
    let Some(folder) = config.folder() else {
        return Err("set a shared folder in Friends first".to_string());
    };
    if config.nick.is_empty() {
        return Err("set your nick in Friends first".to_string());
    }
    launcher_core::instance::validate_id(&id).map_err(|e| e.to_string())?;

    let dir = folder.join("archives");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let target = dir.join(format!("{}__{id}.zip", config.nick));

    let bundle = crate::commands::transfer::instance_export_to(&state, &id, "zip", &target).await?;
    Ok(SharedArchive {
        nick: config.nick.clone(),
        instance: id,
        file_name: target
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: bundle.path,
        bytes: bundle.bytes,
    })
}

#[tauri::command]
pub async fn friends_import(
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportResult, String> {
    crate::commands::transfer::import_archive(&state, PathBuf::from(path.trim())).await
}

/// Publish on a timer for as long as the app runs. A missing or unset sync
/// folder makes this a cheap no-op, so it stays running unconditionally.
pub fn spawn_publisher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(PUBLISH_INTERVAL).await;
            let state = app.state::<AppState>();
            if let Err(error) = publish(&state) {
                tracing::debug!(%error, "friend presence not published");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn config_round_trips_and_sorts_friends() {
        let mut config = FriendsConfig {
            nick: "  olivia  ".into(),
            sync_folder: Some("  C:/sync  ".into()),
            friends: vec!["zed".into(), "abe".into(), "abe".into(), "  ".into()],
        };
        config.sanitise();
        assert_eq!(config.nick, "olivia");
        assert_eq!(config.sync_folder.as_deref(), Some("C:/sync"));
        assert_eq!(config.friends, vec!["abe".to_string(), "zed".to_string()]);
    }

    #[test]
    fn server_is_read_from_the_last_connecting_line() {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(dir.path().join("logs")).expect("mkdir");
        let mut file = std::fs::File::create(dir.path().join("logs/game.log")).expect("create");
        writeln!(file, "[12:00:00] [main/INFO]: Connecting to hypixel.net, 25565").expect("write");
        writeln!(file, "[12:00:01] [main/INFO]: Connecting to play.example.com, 25565")
            .expect("write");
        drop(file);
        assert_eq!(
            server_from_log(dir.path()).as_deref(),
            Some("play.example.com")
        );
    }
}
