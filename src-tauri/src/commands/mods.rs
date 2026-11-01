use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, Utc};
use launcher_core::instance::{InstanceConfig, NewInstance};
use launcher_modrinth::installed::{target_dir, InstalledMod, InstalledMods};
use launcher_modrinth::{ModrinthProject, ModrinthSearch, ModrinthVersion, SearchQuery};
use launcher_net::download::{DownloadSpec, ProgressTracker};
use launcher_net::mrpack::{self, MrpackInfo};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::game::install_inner;
use crate::commands::progress_channel;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ModUpdate {
    pub project_id: String,
    pub title: String,
    pub installed_version: String,
    pub latest_version: String,
    pub latest_version_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModpackInstallResult {
    pub instance: InstanceConfig,
    pub pack: MrpackInfo,
}

#[tauri::command]
pub async fn modrinth_search(
    state: State<'_, AppState>,
    query: SearchQuery,
) -> Result<ModrinthSearch, String> {
    state
        .modrinth
        .search(query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn modrinth_project(
    state: State<'_, AppState>,
    id_or_slug: String,
) -> Result<ModrinthProject, String> {
    state
        .modrinth
        .project(&id_or_slug)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn modrinth_versions(
    state: State<'_, AppState>,
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<ModrinthVersion>, String> {
    state
        .modrinth
        .versions(&project_id, game_version.as_deref(), loader.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mods_installed(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<InstalledMod>, String> {
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let game_dir = state.paths.game_dir(&config.id);
    let manifest = InstalledMods::load(&game_dir).map_err(|e| e.to_string())?;
    Ok(with_untracked(&game_dir, manifest.mods))
}

/// Jars dropped in by hand, copied from another launcher or unpacked from a
/// pack never appear in the Modrinth manifest, but they are still installed —
/// the Mods tab lists them so "32 jars on disk" and "0 mods installed" cannot
/// disagree.
fn with_untracked(game_dir: &Path, mut mods: Vec<InstalledMod>) -> Vec<InstalledMod> {
    let known: std::collections::HashSet<String> = mods
        .iter()
        .map(|entry| entry.file_name.to_ascii_lowercase())
        .collect();

    let dir = game_dir.join("mods");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        mods.sort_by(|a, b| a.project_title.to_lowercase().cmp(&b.project_title.to_lowercase()));
        return mods;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().map(|value| value.to_string_lossy().to_string()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".jar") || known.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        let stem = name.trim_end_matches(".jar").to_string();
        mods.push(InstalledMod {
            project_id: String::new(),
            version_id: String::new(),
            project_title: stem,
            version_number: String::new(),
            file_name: name,
            sha1: None,
            size: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            project_type: "mod".to_string(),
            installed_at: entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(Utc::now),
        });
    }

    mods.sort_by(|a, b| a.project_title.to_lowercase().cmp(&b.project_title.to_lowercase()));
    mods
}

/// Install a project (mod, resource pack or shader) plus required dependencies.
#[tauri::command]
pub async fn mods_install(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
    version_id: Option<String>,
) -> Result<Vec<InstalledMod>, String> {
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let project = state
        .modrinth
        .project(&project_id)
        .await
        .map_err(|e| e.to_string())?;

    if project.project_type == "mod" && !config.game.loader.is_modded() {
        return Err("this instance is vanilla — install Fabric, Quilt, Forge or NeoForge first".into());
    }

    let loader = loader_slug(config.game.loader);
    let version = match version_id {
        Some(version_id) => {
            let version = state
                .modrinth
                .version(&version_id)
                .await
                .map_err(|e| e.to_string())?;
            if version.project_id != project.id {
                return Err("selected version does not belong to this project".into());
            }
            version
        }
        None => state
            .modrinth
            .latest_version(&project.id, &config.game.version, loader)
            .await
            .map_err(|e| e.to_string())?,
    };

    let mut versions = vec![version.clone()];
    let mut titles = HashMap::new();
    titles.insert(version.project_id.clone(), project.title.clone());

    if project.project_type == "mod" {
        let deps = state
            .modrinth
            .resolve_dependencies(&version, &config.game.version, loader)
            .await
            .map_err(|e| e.to_string())?;
        let mut seen: HashSet<String> = HashSet::from([version.project_id.clone()]);
        for dep in deps {
            if seen.insert(dep.project_id.clone()) {
                titles
                    .entry(dep.project_id.clone())
                    .or_insert_with(|| dep_title(&dep));
                versions.push(dep);
            }
        }
    }

    install_versions(&app, &state, &config, &project.project_type, versions, titles).await
}

/// Shared download + manifest step for a batch of resolved versions.
async fn install_versions(
    app: &AppHandle,
    state: &AppState,
    config: &InstanceConfig,
    project_type: &str,
    versions: Vec<ModrinthVersion>,
    titles: HashMap<String, String>,
) -> Result<Vec<InstalledMod>, String> {
    let game_dir = state.paths.game_dir(&config.id);
    let mut specs = Vec::new();
    let mut planned: Vec<(ModrinthVersion, String)> = Vec::new();

    for candidate in &versions {
        let Some(file) = candidate.primary_file() else {
            continue;
        };
        let folder = target_dir(&game_dir, project_type);
        let dest = folder.join(launcher_net::cache::safe_file_name(&file.filename));
        specs.push(
            DownloadSpec::new(file.url.clone(), dest)
                .with_sha1(file.hashes.get("sha1").cloned())
                .with_sha512(file.hashes.get("sha512").cloned())
                .with_size(Some(file.size)),
        );
        planned.push((candidate.clone(), file.filename.clone()));
    }

    let tx = progress_channel(app, "mods://progress");
    let tracker = ProgressTracker::new(specs.len(), Some(tx));
    state
        .downloads
        .fetch_all(specs, tracker)
        .await
        .map_err(|e| e.to_string())?;

    let mut manifest = InstalledMods::load(&game_dir).map_err(|e| e.to_string())?;
    for (version, file_name) in planned {
        let title = titles
            .get(&version.project_id)
            .cloned()
            .unwrap_or_else(|| dep_title(&version));
        manifest.upsert(InstalledMod {
            project_id: version.project_id.clone(),
            version_id: version.id.clone(),
            project_title: title,
            version_number: version.version_number.clone(),
            file_name,
            sha1: version
                .primary_file()
                .and_then(|f| f.hashes.get("sha1").cloned()),
            size: version.primary_file().map(|f| f.size).unwrap_or(0),
            project_type: project_type.to_string(),
            installed_at: Utc::now(),
        });
    }
    manifest.save(&game_dir).map_err(|e| e.to_string())?;
    Ok(manifest.mods)
}

fn dep_title(version: &ModrinthVersion) -> String {
    if version.name.is_empty() {
        version.project_id.clone()
    } else {
        version.name.clone()
    }
}

/// Curated client-side performance sets. Slugs are Modrinth project slugs; a
/// project without a build for this version is reported and skipped.
const FABRIC_STACK: &[&str] = &[
    "sodium",
    "lithium",
    "ferrite-core",
    "krypton",
    "immediatelyfast",
    "entityculling",
    "modernfix",
    "sodium-extra",
    "iris",
];

const FORGE_STACK: &[&str] = &[
    "embeddium",
    "oculus",
    "ferrite-core",
    "modernfix",
    "entityculling",
    "canary",
];

const NEOFORGE_STACK: &[&str] = &[
    "sodium",
    "lithium",
    "ferrite-core",
    "modernfix",
    "entityculling",
    "immediatelyfast",
    "iris",
];

fn optimization_stack(loader: launcher_core::LoaderKind) -> Option<&'static [&'static str]> {
    match loader {
        launcher_core::LoaderKind::Vanilla => None,
        launcher_core::LoaderKind::Fabric | launcher_core::LoaderKind::Quilt => Some(FABRIC_STACK),
        launcher_core::LoaderKind::Forge => Some(FORGE_STACK),
        launcher_core::LoaderKind::NeoForge => Some(NEOFORGE_STACK),
    }
}

fn at_least_1_20_2(version: &str) -> bool {
    let Some(rest) = version.strip_prefix("1.") else {
        return false;
    };
    let mut parts = rest.split('.');
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch: u32 = parts
        .next()
        .and_then(|s| s.split(['-', '+']).next()?.parse().ok())
        .unwrap_or(0);
    (minor, patch) >= (20, 2)
}

#[derive(Debug, Clone, Serialize)]
pub struct OptimizationResult {
    pub installed: Vec<InstalledMod>,
    pub skipped: Vec<String>,
}

/// Install the recommended performance stack — Sodium/Iris family on
/// Fabric/Quilt/NeoForge, Embeddium/Oculus on Forge — matching the instance's
/// Minecraft version and loader.
#[tauri::command]
pub async fn optimization_install(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<OptimizationResult, String> {
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let Some(loader) = loader_slug(config.game.loader) else {
        return Err(
            "this instance is vanilla — install Fabric, Quilt, Forge or NeoForge first".into(),
        );
    };
    let stack = optimization_stack(config.game.loader).unwrap_or_default();

    let mut versions: Vec<ModrinthVersion> = Vec::new();
    let mut titles: HashMap<String, String> = HashMap::new();
    let mut skipped = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for slug in stack {
        let project = match state.modrinth.project(slug).await {
            Ok(project) => project,
            Err(error) => {
                skipped.push(format!("{slug}: {error}"));
                continue;
            }
        };

        let version = match state
            .modrinth
            .latest_version(&project.id, &config.game.version, Some(loader))
            .await
        {
            Ok(version) => version,
            Err(error) => {
                skipped.push(format!("{}: {error}", project.title));
                continue;
            }
        };

        if seen.insert(project.id.clone()) {
            titles.insert(project.id.clone(), project.title.clone());
            versions.push(version.clone());
        }

        match state
            .modrinth
            .resolve_dependencies(&version, &config.game.version, Some(loader))
            .await
        {
            Ok(deps) => {
                for dep in deps {
                    if seen.insert(dep.project_id.clone()) {
                        titles
                            .entry(dep.project_id.clone())
                            .or_insert_with(|| dep_title(&dep));
                        versions.push(dep);
                    }
                }
            }
            Err(error) => skipped.push(format!("{} deps: {error}", project.title)),
        }
    }

    if versions.is_empty() {
        let hint = if config.game.loader == launcher_core::LoaderKind::Forge
            && at_least_1_20_2(&config.game.version)
        {
            " — most Forge performance mods moved to NeoForge on 1.20.2+, try a NeoForge instance"
        } else {
            ""
        };
        return Err(format!(
            "no optimization build for Minecraft {} on {}{hint}",
            config.game.version, config.game.loader
        ));
    }

    let installed = install_versions(&app, &state, &config, "mod", versions, titles).await?;
    Ok(OptimizationResult { installed, skipped })
}

#[tauri::command]
pub fn mods_remove(
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
) -> Result<Vec<InstalledMod>, String> {
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let game_dir = state.paths.game_dir(&config.id);
    let mut manifest = InstalledMods::load(&game_dir).map_err(|e| e.to_string())?;

    if let Some(entry) = manifest.remove(&project_id) {
        let path = target_dir(&game_dir, &entry.project_type).join(&entry.file_name);
        if path.is_file() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("cannot delete {}: {e}", path.display()))?;
        }
        manifest.save(&game_dir).map_err(|e| e.to_string())?;
    }
    Ok(manifest.mods)
}

#[tauri::command]
pub async fn mods_check_updates(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<ModUpdate>, String> {
    let config = InstanceConfig::load(&state.paths, &instance_id).map_err(|e| e.to_string())?;
    let manifest = InstalledMods::load(&state.paths.game_dir(&config.id))
        .map_err(|e| e.to_string())?;
    let loader = loader_slug(config.game.loader);
    let mut updates = Vec::new();

    for installed in manifest.mods.iter().filter(|m| m.project_type == "mod") {
        let latest = state
            .modrinth
            .latest_version(&installed.project_id, &config.game.version, loader)
            .await;
        let Ok(latest) = latest else {
            continue;
        };
        if latest.id != installed.version_id {
            updates.push(ModUpdate {
                project_id: installed.project_id.clone(),
                title: installed.project_title.clone(),
                installed_version: installed.version_number.clone(),
                latest_version: latest.version_number.clone(),
                latest_version_id: latest.id.clone(),
            });
        }
    }
    Ok(updates)
}

/// Download a `.mrpack`, create an instance matching its manifest, install the
/// loader, then materialise every pack file and override.
#[tauri::command]
pub async fn modpack_install(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: String,
) -> Result<ModpackInstallResult, String> {
    let version = state
        .modrinth
        .version(&version_id)
        .await
        .map_err(|e| e.to_string())?;
    let file = version
        .primary_file()
        .ok_or_else(|| "modpack version has no downloadable file".to_string())?;

    let packs_dir = state.paths.cache_dir().join("packs");
    std::fs::create_dir_all(&packs_dir).map_err(|e| e.to_string())?;
    let archive = packs_dir.join(launcher_net::cache::safe_file_name(&file.filename));
    state
        .downloads
        .fetch(
            DownloadSpec::new(file.url.clone(), &archive)
                .with_sha1(file.hashes.get("sha1").cloned())
                .with_sha512(file.hashes.get("sha512").cloned())
                .with_size(Some(file.size)),
        )
        .await
        .map_err(|e| e.to_string())?;

    let pack = mrpack::inspect(&archive).map_err(|e| e.to_string())?;
    let draft = NewInstance {
        name: pack.name.clone(),
        mc_version: pack.minecraft.clone(),
        loader: pack.loader,
        loader_version: pack.loader_version.clone(),
        max_memory_mb: None,
    };
    let created = InstanceConfig::create(&state.paths, draft).map_err(|e| e.to_string())?;
    let created_id = created.id.clone();

    let tx = progress_channel(&app, "install://progress");
    let installed = install_inner(&state, &created_id, Some(tx)).await;

    if let Err(error) = installed {
        // A half-installed pack is useless; remove only the instance just made.
        let _ = created.delete(&state.paths);
        return Err(error);
    }

    mrpack::install_mrpack(
        &state.paths,
        &state.downloads,
        &created_id,
        &archive,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let instance = InstanceConfig::load(&state.paths, &created_id).map_err(|e| e.to_string())?;
    Ok(ModpackInstallResult { instance, pack })
}

fn loader_slug(loader: launcher_core::LoaderKind) -> Option<&'static str> {
    match loader {
        launcher_core::LoaderKind::Vanilla => None,
        launcher_core::LoaderKind::Fabric => Some("fabric"),
        launcher_core::LoaderKind::Quilt => Some("quilt"),
        launcher_core::LoaderKind::Forge => Some("forge"),
        launcher_core::LoaderKind::NeoForge => Some("neoforge"),
    }
}
