use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Public types (mirrored in ui/src/lib/types.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeHit {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub summary: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub author: String,
    pub game_versions: Vec<String>,
    pub mod_loaders: Vec<String>,
    pub date_modified: String,
    pub project_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurseForgeSearch {
    pub hits: Vec<CurseForgeHit>,
    pub total: usize,
    pub page_size: usize,
    pub index: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CurseForgeSearchQuery {
    pub text: String,
    pub project_type: String,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

// ---------------------------------------------------------------------------
// CurseForge class IDs
// ---------------------------------------------------------------------------

fn class_id_for(project_type: &str) -> u32 {
    match project_type {
        "mod" => 6,
        "modpack" => 4471,
        "resourcepack" => 12,
        "shader" => 6552,
        _ => 6,
    }
}

// ---------------------------------------------------------------------------
// CurseForge mod loader IDs
// ---------------------------------------------------------------------------

fn loader_id_for(loader: &str) -> Option<u32> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// CurseForge project-type slug
// ---------------------------------------------------------------------------

fn project_type_slug(class_id: u32) -> &'static str {
    match class_id {
        6 => "mc-mods",
        4471 => "modpacks",
        12 => "texture-packs",
        6552 => "shaders",
        _ => "mc-mods",
    }
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn curseforge_search(
    state: State<'_, AppState>,
    query: CurseForgeSearchQuery,
) -> Result<CurseForgeSearch, String> {
    let settings = state.settings();
    let api_key = settings
        .curseforge_api_key
        .clone()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "CurseForge API key not set — add it in Settings → Endpoints".to_string())?;

    let client = state.http();
    search(&client, &api_key, &query)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// HTTP logic (separated for testability)
// ---------------------------------------------------------------------------

async fn search(
    client: &Client,
    api_key: &str,
    query: &CurseForgeSearchQuery,
) -> Result<CurseForgeSearch, String> {
    #[derive(Deserialize)]
    struct ApiResponse {
        data: Vec<ApiMod>,
        pagination: Pagination,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Pagination {
        total_count: usize,
        page_size: usize,
        index: usize,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ApiMod {
        id: u64,
        name: String,
        slug: String,
        summary: String,
        download_count: f64,
        logo: Option<Logo>,
        authors: Vec<Author>,
        latest_files: Vec<LatestFile>,
        date_modified: String,
        class_id: Option<u32>,
        links: Links,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Logo {
        url: String,
    }

    #[derive(Deserialize)]
    struct Author {
        name: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LatestFile {
        game_versions: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Links {
        website_url: String,
    }

    let class_id = class_id_for(&query.project_type);

    let mut params: Vec<(&str, String)> = vec![
        ("gameId", "432".to_string()),
        ("classId", class_id.to_string()),
        ("searchFilter", query.text.clone()),
        ("pageSize", query.limit.to_string()),
        ("index", query.offset.to_string()),
        ("sortField", "2".to_string()), // 2 = Popularity
        ("sortOrder", "desc".to_string()),
    ];

    if let Some(version) = &query.game_version {
        if !version.is_empty() {
            params.push(("gameVersion", version.clone()));
        }
    }

    if let Some(loader_str) = &query.loader {
        if let Some(lid) = loader_id_for(loader_str) {
            params.push(("modLoaderType", lid.to_string()));
        }
    }

    let response = client
        .get("https://api.curseforge.com/v1/mods/search")
        .header("x-api-key", api_key)
        .header(reqwest::header::USER_AGENT, "spud-launcher")
        .query(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("CurseForge API returned {status}: {body}"));
    }

    let api: ApiResponse = response.json().await.map_err(|e| e.to_string())?;

    let hits = api
        .data
        .into_iter()
        .map(|m| {
            let game_versions: Vec<String> = m
                .latest_files
                .iter()
                .flat_map(|f| f.game_versions.iter().cloned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            let mod_loaders: Vec<String> = m
                .latest_files
                .iter()
                .flat_map(|f| f.game_versions.iter().cloned())
                .filter(|v| {
                    matches!(
                        v.to_lowercase().as_str(),
                        "forge" | "fabric" | "quilt" | "neoforge"
                    )
                })
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            CurseForgeHit {
                id: m.id,
                name: m.name,
                slug: m.slug,
                summary: m.summary,
                downloads: m.download_count as u64,
                icon_url: m.logo.map(|l| l.url),
                author: m.authors.into_iter().map(|a| a.name).next().unwrap_or_default(),
                game_versions,
                mod_loaders,
                date_modified: m.date_modified,
                project_type: project_type_slug(m.class_id.unwrap_or(class_id)).to_string(),
                url: m.links.website_url,
            }
        })
        .collect();

    Ok(CurseForgeSearch {
        hits,
        total: api.pagination.total_count,
        page_size: api.pagination.page_size,
        index: api.pagination.index,
    })
}
