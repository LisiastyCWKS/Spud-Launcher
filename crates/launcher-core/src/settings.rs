use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{read_json, write_json_atomic, Result};
use crate::paths::LauncherPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Dark,
    Light,
}

impl Default for Theme {
    fn default() -> Self {
        Self::System
    }
}

/// Global preferences. Instance-specific settings live in `instance.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherSettings {
    pub schema: u32,
    pub theme: Theme,
    /// BCP-47 tag; empty means "follow the OS".
    pub locale: String,
    /// Parallel HTTP downloads. 8 is a good balance for Mojang CDNs.
    pub concurrency: usize,
    pub default_memory_mb: u32,
    /// Preferred JVM for new instances, if the user picked one explicitly.
    pub default_java: Option<PathBuf>,
    /// Minimize the launcher window once a game starts. The process keeps
    /// running and stays stoppable from the restored window.
    pub close_on_launch: bool,
    /// Bring the launcher back — unminimised and focused — when the last game
    /// exits. Pairs with `close_on_launch`; either can be used alone.
    pub restore_on_exit: bool,
    pub show_snapshots: bool,
    pub mojang_meta: String,
    pub modrinth_api: String,
    /// Optional; CurseForge requires an API key issued per application.
    pub curseforge_api_key: Option<String>,
    /// Azure application (client) id for the Microsoft device-code flow.
    /// `SPUD_MS_CLIENT_ID` still wins when both are set.
    pub ms_client_id: Option<String>,
    /// Download a matching Temurin JRE into `<root>/java/` when no installed
    /// runtime satisfies the version. Turn off to keep everything manual.
    pub auto_install_java: bool,
    /// Show "playing Spud Launcher" on Discord through the local IPC pipe.
    pub discord_rpc: bool,
    /// Discord application id the presence is published under. Empty disables
    /// the connection even when `discord_rpc` is on.
    pub discord_client_id: Option<String>,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            schema: 1,
            theme: Theme::System,
            locale: String::new(),
            concurrency: 8,
            default_memory_mb: 3072,
            default_java: None,
            close_on_launch: false,
            restore_on_exit: true,
            show_snapshots: false,
            mojang_meta: "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
                .to_string(),
            modrinth_api: "https://api.modrinth.com/v2".to_string(),
            curseforge_api_key: None,
            ms_client_id: None,
            auto_install_java: true,
            discord_rpc: true,
            discord_client_id: None,
        }
    }
}

impl LauncherSettings {
    pub fn load(paths: &LauncherPaths) -> Result<Self> {
        let path = paths.settings_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut settings: Self = read_json(&path)?;
        settings.sanitise();
        Ok(settings)
    }

    pub fn save(&self, paths: &LauncherPaths) -> Result<()> {
        write_json_atomic(&paths.settings_file(), self)
    }

    /// Keep values inside ranges the downloader and JVM can survive.
    pub fn sanitise(&mut self) {
        self.concurrency = self.concurrency.clamp(1, 32);
        self.default_memory_mb = self.default_memory_mb.clamp(512, 65536);
        if self.mojang_meta.is_empty() {
            self.mojang_meta = Self::default().mojang_meta;
        }
        if self.modrinth_api.is_empty() {
            self.modrinth_api = Self::default().modrinth_api;
        }
        // A whitespace-only id is the same as no id at all; anything else is
        // trimmed so a copy-pasted value does not fail the OAuth request.
        self.ms_client_id = self
            .ms_client_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        // Same treatment for the Discord app id: Discord rejects a malformed
        // handshake and there is no point dialling the pipe for one.
        self.discord_client_id = self
            .discord_client_id
            .as_deref()
            .map(str::trim)
            .filter(|value| value.chars().all(|c| c.is_ascii_digit()) && !value.is_empty())
            .map(str::to_string);
        self.schema = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_is_trimmed_and_blank_becomes_none() {
        let mut settings = LauncherSettings::default();
        settings.ms_client_id = Some("   ".into());
        settings.sanitise();
        assert_eq!(settings.ms_client_id, None);

        settings.ms_client_id = Some("  00000000-0000-0000-0000-000000000000  ".into());
        settings.sanitise();
        assert_eq!(
            settings.ms_client_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000000")
        );
    }
}
