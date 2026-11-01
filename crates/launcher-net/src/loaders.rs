use launcher_core::LoaderKind;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{user_agent, NetError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderBuild {
    pub version: String,
    #[serde(default)]
    pub stable: bool,
}

/// Queries loader metadata APIs for installable builds.
#[derive(Clone)]
pub struct LoaderService {
    client: Client,
}

impl LoaderService {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn versions(
        &self,
        loader: LoaderKind,
        mc_version: &str,
    ) -> Result<Vec<LoaderBuild>> {
        match loader {
            LoaderKind::Vanilla => Ok(vec![]),
            LoaderKind::Fabric => {
                let url = format!(
                    "https://meta.fabricmc.net/v2/versions/loader/{mc_version}"
                );
                self.fabric_like(&url).await
            }
            LoaderKind::Quilt => {
                let url = format!(
                    "https://meta.quiltmc.org/v3/versions/loader/{mc_version}"
                );
                self.fabric_like(&url).await
            }
            LoaderKind::Forge => self.forge_versions(mc_version).await,
            LoaderKind::NeoForge => self.neoforge_versions(mc_version).await,
        }
    }

    async fn fabric_like(&self, url: &str) -> Result<Vec<LoaderBuild>> {
        #[derive(Deserialize)]
        struct Entry {
            loader: Loader,
        }
        #[derive(Deserialize)]
        struct Loader {
            version: String,
            #[serde(default)]
            stable: bool,
        }

        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, user_agent())
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(NetError::HttpStatus {
                status: response.status().as_u16(),
                url: url.to_string(),
            });
        }
        let entries: Vec<Entry> = response.json().await?;
        let builds = entries
            .into_iter()
            .map(|entry| LoaderBuild {
                version: entry.loader.version,
                stable: entry.loader.stable,
            })
            .collect();
        Ok(builds)
    }

    async fn forge_versions(&self, mc_version: &str) -> Result<Vec<LoaderBuild>> {
        #[derive(Deserialize)]
        struct Promotions {
            promos: std::collections::HashMap<String, String>,
        }

        let url = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
        let promotions: Promotions = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, user_agent())
            .send()
            .await?
            .json()
            .await?;

        let mut builds = Vec::new();
        if let Some(recommended) = promotions.promos.get(&format!("{mc_version}-recommended")) {
            builds.push(LoaderBuild {
                version: recommended.clone(),
                stable: true,
            });
        }
        if let Some(latest) = promotions.promos.get(&format!("{mc_version}-latest")) {
            if !builds.iter().any(|b| &b.version == latest) {
                builds.push(LoaderBuild {
                    version: latest.clone(),
                    stable: false,
                });
            }
        }
        Ok(builds)
    }

    async fn neoforge_versions(&self, mc_version: &str) -> Result<Vec<LoaderBuild>> {
        #[derive(Deserialize)]
        struct MavenVersions {
            versions: Vec<String>,
        }

        let url = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, user_agent())
            .send()
            .await?;
        if response.status().is_success() {
            let listing: MavenVersions = response.json().await?;
            let prefix = mc_version
                .strip_prefix("1.")
                .map(|v| format!("{v}."))
                .unwrap_or_default();
            let mut versions: Vec<String> = listing
                .versions
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .collect();
            versions.sort();
            versions.reverse();
            return Ok(versions
                .into_iter()
                .map(|version| LoaderBuild {
                    version,
                    stable: true,
                })
                .collect());
        }

        // NeoForge 1.20.1 lives under the legacy `forge` artifact.
        if mc_version == "1.20.1" {
            let url = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge";
            let listing: MavenVersions = self
                .client
                .get(url)
                .header(reqwest::header::USER_AGENT, user_agent())
                .send()
                .await?
                .json()
                .await?;
            let prefix = format!("{mc_version}-");
            let mut versions: Vec<String> = listing
                .versions
                .into_iter()
                .filter_map(|v| v.strip_prefix(&prefix).map(str::to_owned))
                .collect();
            versions.sort();
            versions.reverse();
            return Ok(versions
                .into_iter()
                .map(|version| LoaderBuild {
                    version,
                    stable: true,
                })
                .collect());
        }

        Ok(vec![])
    }
}
