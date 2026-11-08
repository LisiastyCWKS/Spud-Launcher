use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// Latest version string, e.g. "0.3.0".
    pub latest: String,
    /// URL to the GitHub Release page.
    pub url: String,
    /// Whether the latest release is newer than the running binary.
    pub available: bool,
    /// Release notes (first 1 KB of the body).
    pub notes: String,
}

/// Check GitHub Releases for a newer version.
///
/// Returns `available: false` when the network is unreachable or the check
/// fails — a dead internet connection should never break the launcher UI.
#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> Result<UpdateInfo, String> {
    let client = state.http();
    fetch_update_info(&client).await.map_err(|e| e.to_string())
}

async fn fetch_update_info(client: &Client) -> Result<UpdateInfo, String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        body: Option<String>,
    }

    let release: Release = client
        .get("https://api.github.com/repos/LisiastyCWKS/Spud-Launcher/releases/latest")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::USER_AGENT, "spud-launcher")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let latest = release.tag_name.trim_start_matches('v').to_string();
    let current = env!("CARGO_PKG_VERSION");
    let available = is_newer(&latest, current);

    let notes = release
        .body
        .unwrap_or_default()
        .chars()
        .take(1024)
        .collect();

    Ok(UpdateInfo {
        latest,
        url: release.html_url,
        available,
        notes,
    })
}

/// Compares two `major.minor.patch` version strings.
/// Returns `true` when `candidate` is strictly newer than `current`.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(s: &str) -> [u32; 3] {
        let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        [
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        ]
    }
    parse(candidate) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }
}
