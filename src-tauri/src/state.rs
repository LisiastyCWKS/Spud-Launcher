use std::sync::RwLock;
use std::time::Duration;

use launcher_auth::{AccountStore, MicrosoftAuth, TokenVault};
use launcher_core::settings::LauncherSettings;
use launcher_core::LauncherPaths;
use launcher_modrinth::ModrinthClient;
use launcher_net::{DownloadManager, GameInstaller, LoaderService};
use reqwest::Client;

use crate::commands::console::LogTailers;
use crate::discord::DiscordPresence;
use crate::running::RunningGames;

pub struct AppState {
    pub paths: LauncherPaths,
    pub settings: RwLock<LauncherSettings>,
    pub downloads: DownloadManager,
    pub installer: GameInstaller,
    pub loaders: LoaderService,
    /// Rebuilt whenever the settings are saved, so a client id pasted into the
    /// UI works without restarting the launcher.
    auth: RwLock<MicrosoftAuth>,
    pub accounts: AccountStore,
    pub vault: TokenVault,
    pub modrinth: ModrinthClient,
    pub running: RunningGames,
    pub logs: LogTailers,
    /// Serialises mod-store sweeps: two concurrent dedupes would race on
    /// `index.json` and could delete the same duplicate twice.
    pub store_lock: tokio::sync::Mutex<()>,
    /// Discord presence worker; replaced when the settings change.
    discord: RwLock<DiscordPresence>,
    http: Client,
}

impl AppState {
    pub fn bootstrap() -> Result<Self, String> {
        let paths = LauncherPaths::discover().map_err(|e| e.to_string())?;
        let settings = LauncherSettings::load(&paths).map_err(|e| e.to_string())?;

        let http = Client::builder()
            .user_agent(format!(
                "spud-launcher/{} (https://github.com/spud-launcher/spud)",
                env!("CARGO_PKG_VERSION")
            ))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;

        let downloads = DownloadManager::new(http.clone(), settings.concurrency);
        let installer = GameInstaller::new(
            paths.clone(),
            downloads.clone(),
            settings.mojang_meta.clone(),
        );
        let loaders = LoaderService::new(http.clone());
        let auth = MicrosoftAuth::new(http.clone(), resolve_client_id(&settings));
        let accounts = AccountStore::new(&paths);
        let modrinth = ModrinthClient::new(http.clone(), settings.modrinth_api.clone());
        let discord = DiscordPresence::start(discord_client_id(&settings));

        Ok(Self {
            paths,
            settings: RwLock::new(settings),
            downloads,
            installer,
            loaders,
            auth: RwLock::new(auth),
            accounts,
            vault: TokenVault::new(),
            modrinth,
            running: RunningGames::new(),
            logs: LogTailers::default(),
            store_lock: tokio::sync::Mutex::new(()),
            discord: RwLock::new(discord),
            http,
        })
    }

    /// Snapshot of settings; never hold the lock across an await.
    pub fn settings(&self) -> LauncherSettings {
        self.settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Shared HTTP client. Adoptium downloads and the modrinth calls already
    /// share one connection pool; handing it out keeps that true.
    pub fn http(&self) -> Client {
        self.http.clone()
    }

    /// Clone of the auth client. `MicrosoftAuth` is a `reqwest::Client` plus a
    /// short string, so cloning is free compared to holding a read guard.
    pub fn auth(&self) -> MicrosoftAuth {
        self.auth
            .read()
            .map(|auth| auth.clone())
            .unwrap_or_else(|_| MicrosoftAuth::new(self.http.clone(), String::new()))
    }

    /// Apply a client-id change made in the settings panel.
    pub fn refresh_auth(&self) {
        let auth = MicrosoftAuth::new(self.http.clone(), resolve_client_id(&self.settings()));
        if let Ok(mut guard) = self.auth.write() {
            *guard = auth;
        }
    }

    /// Where the effective Microsoft client id comes from.
    pub fn client_id_source(&self) -> (&'static str, String) {
        if let Some(id) = env_client_id() {
            return ("env", id);
        }
        match self
            .settings()
            .ms_client_id
            .filter(|value| !value.trim().is_empty())
        {
            Some(id) => ("settings", id),
            None => ("none", String::new()),
        }
    }

    /// Publish the launcher state on Discord. Cheap and non-blocking; a closed
    /// pipe is the worker's problem, never the caller's.
    pub fn presence(&self, activity: Option<crate::discord::Activity>) {
        if let Ok(discord) = self.discord.read() {
            discord.set(activity);
        }
    }

    /// Rebuild the presence worker after a settings change.
    pub fn refresh_presence(&self) {
        let settings = self.settings();
        if let Ok(mut guard) = self.discord.write() {
            *guard = DiscordPresence::start(discord_client_id(&settings));
        }
        self.presence(Some(crate::discord::idle()));
    }
}

/// `SPUD_MS_CLIENT_ID` overrides the stored value, so a developer shell can
/// point the launcher at a test app registration without touching settings.
fn env_client_id() -> Option<String> {
    std::env::var("SPUD_MS_CLIENT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_client_id(settings: &LauncherSettings) -> String {
    env_client_id()
        .or_else(|| {
            settings
                .ms_client_id
                .clone()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_default()
}

/// Presence runs only when the toggle is on *and* an application id exists;
/// without an id Discord has nothing to attribute the activity to.
///
/// The built-in fallback id (`1568705509882728518`) is the Spud Launcher
/// Discord application — it works out of the box for every user. A custom id
/// in settings overrides it.
const DEFAULT_DISCORD_CLIENT_ID: &str = "1568705509882728518";

fn discord_client_id(settings: &LauncherSettings) -> Option<String> {
    if !settings.discord_rpc {
        return None;
    }
    // Custom id in settings wins; fall back to the built-in Spud Launcher app.
    let id = settings
        .discord_client_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_DISCORD_CLIENT_ID.to_string());
    Some(id)
}
