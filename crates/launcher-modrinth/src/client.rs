use std::collections::{HashSet, VecDeque};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{user_agent, ModrinthError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthSearch {
    pub hits: Vec<SearchHit>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub project_id: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub author: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub display_categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub date_modified: String,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthProject {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<VersionDependency>,
    #[serde(default)]
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub date_published: String,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    /// `required`, `optional`, `incompatible`, `embedded`.
    #[serde(default)]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionFile {
    #[serde(default)]
    pub hashes: std::collections::HashMap<String, String>,
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub file_type: Option<String>,
}

impl ModrinthVersion {
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
}

/// Search filters mirrored by the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub project_type: Option<String>,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub limit: u32,
    pub offset: u32,
    /// relevance | downloads | follows | newest | updated
    pub index: Option<String>,
}

#[derive(Clone)]
pub struct ModrinthClient {
    client: Client,
    base: String,
}

impl ModrinthClient {
    pub fn new(client: Client, base: impl Into<String>) -> Self {
        Self {
            client,
            base: base.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn search(&self, query: SearchQuery) -> Result<ModrinthSearch> {
        let mut facets: Vec<Vec<String>> = Vec::new();
        if let Some(kind) = &query.project_type {
            facets.push(vec![format!("project_type:{kind}")]);
        }
        if let Some(version) = &query.game_version {
            facets.push(vec![format!("versions:{version}")]);
        }
        if let Some(loader) = &query.loader {
            facets.push(vec![format!("categories:{loader}")]);
        }

        let mut request = self
            .client
            .get(format!("{}/search", self.base))
            .header(reqwest::header::USER_AGENT, user_agent())
            .query(&[
                ("query", query.text.as_str()),
                ("limit", &query.limit.clamp(1, 50).to_string()),
                ("offset", &query.offset.to_string()),
            ]);

        if !facets.is_empty() {
            let facets = serde_json::to_string(&facets)?;
            request = request.query(&[("facets", facets)]);
        }
        if let Some(index) = &query.index {
            request = request.query(&[("index", index)]);
        }

        let response = request.send().await?;
        let response = ensure_success(response, &format!("{}/search", self.base)).await?;
        Ok(response.json().await?)
    }

    pub async fn project(&self, id_or_slug: &str) -> Result<ModrinthProject> {
        let url = format!("{}/project/{}", self.base, id_or_slug);
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, user_agent())
            .send()
            .await?;
        let response = ensure_success(response, &url).await?;
        Ok(response.json().await?)
    }

    pub async fn versions(
        &self,
        project_id: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<Vec<ModrinthVersion>> {
        let url = format!("{}/project/{}/version", self.base, project_id);
        let mut request = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, user_agent());

        if let Some(game_version) = game_version {
            request = request.query(&[(
                "game_versions",
                serde_json::to_string(&[game_version])?,
            )]);
        }
        if let Some(loader) = loader {
            request = request.query(&[("loaders", serde_json::to_string(&[loader])?)]);
        }

        let response = request.send().await?;
        let response = ensure_success(response, &url).await?;
        Ok(response.json().await?)
    }

    pub async fn version(&self, version_id: &str) -> Result<ModrinthVersion> {
        let url = format!("{}/version/{}", self.base, version_id);
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, user_agent())
            .send()
            .await?;
        let response = ensure_success(response, &url).await?;
        Ok(response.json().await?)
    }

    pub async fn latest_version(
        &self,
        project_id: &str,
        game_version: &str,
        loader: Option<&str>,
    ) -> Result<ModrinthVersion> {
        let mut versions = self
            .versions(project_id, Some(game_version), loader)
            .await?;
        if versions.is_empty() {
            return Err(ModrinthError::NoCompatibleVersion(
                project_id.to_string(),
                game_version.to_string(),
                loader.unwrap_or("any").to_string(),
            ));
        }
        // The API returns newest first; sort defensively.
        versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));
        Ok(versions.remove(0))
    }

    /// Resolve required dependencies transitively. Optional and embedded
    /// dependencies are skipped; incompatible ones are ignored (the UI can warn).
    pub async fn resolve_dependencies(
        &self,
        root: &ModrinthVersion,
        game_version: &str,
        loader: Option<&str>,
    ) -> Result<Vec<ModrinthVersion>> {
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        for dep in &root.dependencies {
            if dep.dependency_type != "required" {
                continue;
            }
            if let Some(project_id) = &dep.project_id {
                queue.push_back((project_id.clone(), 0));
            } else if let Some(version_id) = &dep.version_id {
                if let Ok(version) = self.version(version_id).await {
                    queue.push_back((version.project_id.clone(), 0));
                }
            }
        }

        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(root.project_id.clone());
        let mut resolved = Vec::new();

        while let Some((project_id, depth)) = queue.pop_front() {
            if depth > 8 || !visited.insert(project_id.clone()) {
                continue;
            }
            let Ok(version) = self
                .latest_version(&project_id, game_version, loader)
                .await
            else {
                tracing::warn!(%project_id, "dependency has no compatible version");
                continue;
            };
            for dep in &version.dependencies {
                if dep.dependency_type == "required" {
                    if let Some(next) = &dep.project_id {
                        queue.push_back((next.clone(), depth + 1));
                    }
                }
            }
            resolved.push(version);
        }

        Ok(resolved)
    }

    /// Look up a file by its SHA-1 — used to detect installed mod versions and
    /// offer updates.
    pub async fn version_by_hash(&self, sha1: &str) -> Result<Option<ModrinthVersion>> {
        let url = format!("{}/version_file/{}?algorithm=sha1", self.base, sha1);
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, user_agent())
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = ensure_success(response, &url).await?;
        Ok(Some(response.json().await?))
    }
}

async fn ensure_success(response: reqwest::Response, url: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Err(ModrinthError::Status {
        status,
        url: url.to_string(),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_file_prefers_the_flagged_one() {
        let version = ModrinthVersion {
            id: "v".into(),
            project_id: "p".into(),
            name: String::new(),
            version_number: String::new(),
            version_type: String::new(),
            game_versions: vec![],
            loaders: vec![],
            dependencies: vec![],
            files: vec![
                VersionFile {
                    hashes: Default::default(),
                    url: "a".into(),
                    filename: "a.jar".into(),
                    primary: false,
                    size: 0,
                    file_type: None,
                },
                VersionFile {
                    hashes: Default::default(),
                    url: "b".into(),
                    filename: "b.jar".into(),
                    primary: true,
                    size: 0,
                    file_type: None,
                },
            ],
            date_published: String::new(),
            downloads: 0,
        };
        assert_eq!(version.primary_file().unwrap().filename, "b.jar");
    }
}
