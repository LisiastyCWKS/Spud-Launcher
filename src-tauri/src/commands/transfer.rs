//! Instance transfer: one archive out, one instance in.
//!
//! Two shapes are produced:
//!
//! * `zip` — the whole `.minecraft` tree plus a `spud-instance.json` manifest.
//!   Restores byte-for-byte, including hand-installed jars and worlds.
//! * `mrpack` — a Modrinth pack index. Modrinth-tracked jars are referenced by
//!   their CDN URL, so the pack stays small; everything else (configs, worlds,
//!   hand-installed jars, `options.txt`) travels as `overrides/`.
//!
//! Import accepts both shapes plus a bare `.minecraft` archive without a
//! manifest — it gets default settings. A `.mrpack` brings its `overrides/` in
//! the archive and pulls the linked jars from the CDN, verifying each hash.
//! Everything is written through a staging directory first, so a bad archive
//! cannot leave a half-built instance behind.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use launcher_core::instance::{validate_id, InstanceConfig, NewInstance};
use launcher_core::LoaderKind;
use launcher_modrinth::installed::{target_dir, InstalledMods};
use launcher_net::download::{DownloadSpec, ProgressTracker};
use serde::{Deserialize, Serialize};
use tauri::State;
use zip::write::SimpleFileOptions;

use crate::state::AppState;

/// Level 6 keeps archives noticeably smaller than stored files without the
/// CPU bill of level 9 — a 400 MB instance packs in a few seconds.
const COMPRESSION: i64 = 6;

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub path: String,
    pub format: String,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub instance: InstanceConfig,
    pub files: usize,
}

/// Portable copy of `instance.json` — same fields, but the id and timestamps
/// are re-derived on import so the archive can be opened twice.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortableManifest {
    name: String,
    version: String,
    #[serde(default = "vanilla_loader")]
    loader: LoaderKind,
    #[serde(default)]
    loader_version: Option<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    max_memory_mb: Option<u32>,
    #[serde(default)]
    jvm_args: Vec<String>,
    #[serde(default)]
    game_args: Vec<String>,
    #[serde(default)]
    env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    exported_at: Option<String>,
    #[serde(default)]
    launcher: Option<String>,
}

fn vanilla_loader() -> LoaderKind {
    LoaderKind::Vanilla
}

#[tauri::command]
pub async fn instance_export(
    state: State<'_, AppState>,
    id: String,
    format: Option<String>,
) -> Result<ExportResult, String> {
    validate_id(&id).map_err(|e| e.to_string())?;
    let config = InstanceConfig::load(&state.paths, &id).map_err(|e| e.to_string())?;
    let format = match format.as_deref().unwrap_or("zip") {
        "mrpack" => "mrpack",
        _ => "zip",
    };

    let exports = state.paths.root().join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| format!("cannot create the export folder: {e}"))?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let target = exports.join(format!("{}-{stamp}.{format}", config.id));

    write_to(&state, &config, format, &target)
}

/// Export to a specific path — used by the friends folder, which needs a
/// predictable `<nick>__<instance>.zip` name instead of a timestamped one.
pub(crate) async fn instance_export_to(
    state: &AppState,
    id: &str,
    format: &str,
    target: &Path,
) -> Result<ExportResult, String> {
    validate_id(id).map_err(|e| e.to_string())?;
    let config = InstanceConfig::load(&state.paths, id).map_err(|e| e.to_string())?;
    let format = if format == "mrpack" { "mrpack" } else { "zip" };
    write_to(state, &config, format, target)
}

fn write_to(
    state: &AppState,
    config: &InstanceConfig,
    format: &str,
    target: &Path,
) -> Result<ExportResult, String> {
    let result = if format == "mrpack" {
        write_mrpack(state, config, target)?
    } else {
        write_bundle(state, config, target)?
    };

    Ok(ExportResult {
        path: target.display().to_string(),
        format: format.to_string(),
        files: result.0,
        bytes: result.1,
    })
}

fn write_bundle(
    state: &AppState,
    config: &InstanceConfig,
    target: &Path,
) -> Result<(usize, u64), String> {
    let file = std::fs::File::create(target).map_err(|e| format!("cannot create the archive: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_level(Some(COMPRESSION));

    let manifest = PortableManifest {
        name: config.name.clone(),
        version: config.game.version.clone(),
        loader: config.game.loader,
        loader_version: config.game.loader_version.clone(),
        notes: config.notes.clone(),
        max_memory_mb: Some(config.java.max_memory_mb),
        jvm_args: config.jvm_args.clone(),
        game_args: config.game_args.clone(),
        env: config.env.clone(),
        exported_at: Some(Utc::now().to_rfc3339()),
        launcher: Some(format!("spud-launcher/{}", env!("CARGO_PKG_VERSION"))),
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.start_file("spud-instance.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;

    let game_dir = state.paths.game_dir(&config.id);
    let mut files = 0usize;
    let mut bytes = 0u64;

    for entry in walk(&game_dir)? {
        let relative = entry
            .strip_prefix(&game_dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        // Session locks and the launcher's own scratch data have no business in
        // a shareable archive.
        if relative.ends_with("session.lock") || relative.starts_with(".spud/tmp/") {
            continue;
        }
        let name = format!(".minecraft/{relative}");
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        let mut source =
            std::fs::File::open(&entry).map_err(|e| format!("cannot read {}: {e}", entry.display()))?;
        let copied = std::io::copy(&mut source, &mut zip).map_err(|e| e.to_string())?;
        bytes += copied;
        files += 1;
    }

    zip.finish().map_err(|e| e.to_string())?;
    let archive_bytes = std::fs::metadata(target).map(|m| m.len()).map_err(|e| e.to_string())?;
    Ok((files, archive_bytes.max(bytes)))
}

fn write_mrpack(
    state: &AppState,
    config: &InstanceConfig,
    target: &Path,
) -> Result<(usize, u64), String> {
    let game_dir = state.paths.game_dir(&config.id);
    let manifest = InstalledMods::load(&game_dir).map_err(|e| e.to_string())?;

    // Jars that came from Modrinth travel as CDN links so the pack stays small.
    // Everything else - hand-dropped jars, imported profiles with no manifest,
    // worlds, options - travels as `overrides/`, which is what makes an .mrpack
    // work for an instance the launcher never installed from Modrinth.
    let mut files = Vec::new();
    let mut linked: std::collections::HashSet<String> = std::collections::HashSet::new();
    // The index spec wants at least one hash per file, so an entry without a
    // known SHA-1 goes out as an override instead of an unverifiable download.
    for entry in &manifest.mods {
        let Some(sha1) = entry.sha1.clone() else {
            continue;
        };
        if entry.project_id.is_empty() || entry.version_id.is_empty() {
            continue;
        }
        let folder = target_dir(Path::new(""), &entry.project_type)
            .to_string_lossy()
            .replace('\\', "/");
        let path = format!("{folder}/{}", entry.file_name);
        if !game_dir.join(&path).is_file() {
            continue;
        }
        files.push(serde_json::json!({
            "path": path.clone(),
            "hashes": { "sha1": sha1 },
            "env": { "client": "required", "server": "optional" },
            "downloads": [format!(
                "https://cdn.modrinth.com/data/{}/versions/{}/{}",
                entry.project_id, entry.version_id, entry.file_name
            )],
            "fileSize": entry.size,
        }));
        linked.insert(path);
    }

    let mut dependencies = serde_json::Map::new();
    dependencies.insert(
        "minecraft".into(),
        serde_json::Value::String(config.game.version.clone()),
    );
    // The index spec keys loaders as `fabric-loader` / `quilt-loader`, not the
    // short names the launcher uses internally; anything else is ignored by
    // other launchers reading the pack.
    if let (Some(key), Some(loader_version)) = (
        mrpack_loader_key(config.game.loader),
        config.game.loader_version.clone(),
    ) {
        dependencies.insert(key.into(), serde_json::Value::String(loader_version));
    }

    let index = serde_json::json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": config.game.version,
        "name": config.name,
        "summary": format!("{} — exported from Spud Launcher", config.name),
        "files": files,
        "dependencies": dependencies,
    });

    let file = std::fs::File::create(target).map_err(|e| format!("cannot create the pack: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_level(Some(COMPRESSION));
    zip.start_file("modrinth.index.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    // Everything the index does not link travels as `overrides/` - configs,
    // world saves, resource packs, options.txt and any jar that never came
    // from Modrinth. Logs are scratch data and stay out.
    let mut overrides = 0usize;
    let mut candidates = walk(&game_dir)?;
    candidates.sort();
    for entry in candidates {
        let relative = entry
            .strip_prefix(&game_dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if linked.contains(&relative) || is_overrides_excluded(&relative) {
            continue;
        }
        zip.start_file(format!("overrides/{relative}"), options)
            .map_err(|e| e.to_string())?;
        let mut source =
            std::fs::File::open(&entry).map_err(|e| format!("cannot read {}: {e}", entry.display()))?;
        std::io::copy(&mut source, &mut zip).map_err(|e| e.to_string())?;
        overrides += 1;
    }

    zip.finish().map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(target).map(|m| m.len()).map_err(|e| e.to_string())?;
    Ok((files.len() + overrides, bytes))
}

/// Scratch data that must never enter an export: a session lock is a live
/// mutex, logs are machine-specific, and `.spud/` records what this launcher
/// installed rather than what the pack contains.
fn is_overrides_excluded(relative: &str) -> bool {
    relative.eq_ignore_ascii_case("session.lock")
        || relative.starts_with(".spud/")
        || relative.starts_with("logs/")
        || relative.starts_with("crash-reports/")
        || relative.ends_with("/session.lock")
}

/// `fabric-loader` / `quilt-loader` / `forge` / `neoforge` — the loader keys
/// the Modrinth index format defines. A vanilla instance contributes none.
fn mrpack_loader_key(loader: LoaderKind) -> Option<&'static str> {
    match loader {
        LoaderKind::Fabric => Some("fabric-loader"),
        LoaderKind::Quilt => Some("quilt-loader"),
        LoaderKind::Forge => Some("forge"),
        LoaderKind::NeoForge => Some("neoforge"),
        LoaderKind::Vanilla => None,
    }
}

#[tauri::command]
pub async fn instance_import(
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportResult, String> {
    import_archive(&state, PathBuf::from(path.trim())).await
}

pub(crate) async fn import_archive(
    state: &AppState,
    archive: PathBuf,
) -> Result<ImportResult, String> {
    if !archive.is_file() {
        return Err(format!("{} is not a file", archive.display()));
    }

    let file = std::fs::File::open(&archive).map_err(|e| format!("cannot open the archive: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a zip archive: {e}"))?;

    let manifest = read_manifest(&mut zip)?;
    let index = read_index(&mut zip)?;
    let name = manifest
        .as_ref()
        .map(|entry| entry.name.clone())
        .or_else(|| index.as_ref().and_then(|entry| entry.name.clone()))
        .unwrap_or_else(|| {
            archive
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| "Imported instance".to_string())
        });

    // A half-extracted archive is worse than a failed import: unpack into a
    // staging folder, then move the finished tree into place.
    let staging = state
        .paths
        .root()
        .join("instances")
        .join(format!(".import-{}", std::process::id()));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging).map_err(|e| format!("cannot stage the import: {e}"))?;

    let mut files = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("cannot read the archive: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(raw_name) = entry.enclosed_name() else {
            continue; // rejects absolute paths and `..`
        };
        let name = raw_name.to_string_lossy().replace('\\', "/");
        if name == "spud-instance.json" {
            continue;
        }
        let relative = name
            .strip_prefix(".minecraft/")
            .or_else(|| name.strip_prefix("overrides/"))
            .unwrap_or(name.as_str())
            .to_string();
        if relative.is_empty() || relative == "modrinth.index.json" {
            continue;
        }
        let out = launcher_core::instance::safe_join(&staging, &relative)
            .map_err(|e| e.to_string())?;
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot stage: {e}"))?;
        }
        let mut handle = std::fs::File::create(&out)
            .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .map_err(|e| format!("cannot read {}: {e}", name))?;
        handle.write_all(&buffer).map_err(|e| e.to_string())?;
        files += 1;
    }

    let draft = NewInstance {
        name,
        mc_version: manifest
            .as_ref()
            .map(|entry| entry.version.clone())
            .or_else(|| index.as_ref().and_then(|entry| entry.game_version()))
            .unwrap_or_else(|| "1.21.1".to_string()),
        loader: manifest
            .as_ref()
            .map(|entry| entry.loader)
            .or_else(|| index.as_ref().map(|entry| entry.loader()))
            .unwrap_or(LoaderKind::Vanilla),
        loader_version: manifest
            .as_ref()
            .and_then(|entry| entry.loader_version.clone())
            .or_else(|| {
                index
                    .as_ref()
                    .and_then(|entry| entry.loader_version().map(str::to_string))
            }),
        max_memory_mb: manifest.as_ref().and_then(|entry| entry.max_memory_mb),
    };

    let mut config = InstanceConfig::create(&state.paths, draft).map_err(|e| e.to_string())?;
    if let Some(manifest) = &manifest {
        config.notes = manifest.notes.clone();
        config.jvm_args = manifest.jvm_args.clone();
        config.game_args = manifest.game_args.clone();
        config.env = manifest.env.clone();
    }

    let game_dir = state.paths.game_dir(&config.id);
    std::fs::create_dir_all(&game_dir).map_err(|e| format!("cannot create the game folder: {e}"))?;
    for entry in walk(&staging)?.into_iter().filter(|path| path.is_file()) {
        let relative = entry.strip_prefix(&staging).map_err(|e| e.to_string())?;
        let destination = game_dir.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&entry, &destination)
            .or_else(|_| std::fs::copy(&entry, &destination).map(|_| ()))
            .map_err(|e| format!("cannot move {}: {e}", destination.display()))?;
    }
    let _ = std::fs::remove_dir_all(&staging);

    // A Modrinth index lists its jars as CDN links instead of shipping them in
    // `overrides/`. Without this step an imported .mrpack would arrive with its
    // configs but none of its mods.
    if let Some(index) = &index {
        let mut specs = Vec::new();
        for entry in &index.files {
            let Some(url) = entry.downloads.first() else {
                continue;
            };
            let Ok(destination) = launcher_core::instance::safe_join(&game_dir, &entry.path) else {
                continue; // `../` in a pack index is refused, not repaired
            };
            if destination.is_file() {
                continue;
            }
            specs.push(
                DownloadSpec::new(url.clone(), destination)
                    .with_sha1(entry.hashes.get("sha1").cloned())
                    .with_sha512(entry.hashes.get("sha512").cloned())
                    .with_size(entry.file_size),
            );
        }
        if !specs.is_empty() {
            let count = specs.len();
            let tracker = ProgressTracker::new(count, None);
            let summary = state
                .downloads
                .fetch_all(specs, tracker)
                .await
                .map_err(|e| format!("cannot download the pack contents: {e}"))?;
            files += summary.files;
        }
    }

    config.save_touched(&state.paths).map_err(|e| e.to_string())?;
    Ok(ImportResult {
        instance: config,
        files,
    })
}

fn read_manifest(zip: &mut zip::ZipArchive<std::fs::File>) -> Result<Option<PortableManifest>, String> {
    let Some(raw) = read_json_entry(zip, "spud-instance.json")? else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| format!("spud-instance.json is malformed: {e}"))
}

/// Read one JSON member, tolerating a UTF-8 BOM. Packs authored on Windows are
/// routinely written with one, and serde refuses a document that starts with it.
fn read_json_entry(
    zip: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> Result<Option<String>, String> {
    let mut entry = match zip.by_name(name) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    let mut raw = String::new();
    entry.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    Ok(Some(raw.trim_start_matches('\u{feff}').to_string()))
}

/// The subset of `modrinth.index.json` this launcher consumes. A pack written
/// by another launcher has no `spud-instance.json`, so the index is also the
/// only place the game version and loader can come from.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrpackIndex {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    files: Vec<MrpackIndexFile>,
}

impl MrpackIndex {
    fn loader(&self) -> LoaderKind {
        for key in ["fabric-loader", "quilt-loader", "neoforge", "forge"] {
            if self.dependencies.contains_key(key) {
                return match key {
                    "fabric-loader" => LoaderKind::Fabric,
                    "quilt-loader" => LoaderKind::Quilt,
                    "neoforge" => LoaderKind::NeoForge,
                    _ => LoaderKind::Forge,
                };
            }
        }
        LoaderKind::Vanilla
    }

    fn loader_version(&self) -> Option<&str> {
        let key = mrpack_loader_key(self.loader())?;
        self.dependencies
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// `versionId` is optional in the spec; `dependencies.minecraft` is the
    /// authoritative game version.
    fn game_version(&self) -> Option<String> {
        self.dependencies
            .get("minecraft")
            .cloned()
            .or_else(|| self.version_id.clone())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrpackIndexFile {
    path: String,
    #[serde(default)]
    hashes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    file_size: Option<u64>,
}

fn read_index(zip: &mut zip::ZipArchive<std::fs::File>) -> Result<Option<MrpackIndex>, String> {
    let Some(raw) = read_json_entry(zip, "modrinth.index.json")? else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| format!("modrinth.index.json is malformed: {e}"))
}

/// Every regular file below `root`, depth-first. Symlinks are skipped so an
/// archive can never point outside the instance.
fn walk(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                out.push(entry.path());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_survives_a_round_trip() {
        let manifest = PortableManifest {
            name: "Test pack".into(),
            version: "1.21.1".into(),
            loader: LoaderKind::Fabric,
            loader_version: Some("0.16.5".into()),
            notes: "note".into(),
            max_memory_mb: Some(4096),
            jvm_args: vec!["-XX:+UseG1GC".into()],
            game_args: vec![],
            env: Default::default(),
            exported_at: None,
            launcher: None,
        };
        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: PortableManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, manifest.name);
        assert_eq!(back.loader, LoaderKind::Fabric);
    }

    #[test]
    fn walk_lists_regular_files_only() {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(dir.path().join("mods")).expect("mkdir");
        std::fs::write(dir.path().join("mods/a.jar"), b"x").expect("write");
        std::fs::write(dir.path().join("options.txt"), b"y").expect("write");
        let files = walk(dir.path()).expect("walk");
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn index_loader_keys_match_the_modrinth_format() {
        // Export has to write `fabric-loader`; a pack written as `fabric` is
        // ignored by every other launcher.
        assert_eq!(mrpack_loader_key(LoaderKind::Fabric), Some("fabric-loader"));
        assert_eq!(mrpack_loader_key(LoaderKind::Quilt), Some("quilt-loader"));
        assert_eq!(mrpack_loader_key(LoaderKind::NeoForge), Some("neoforge"));
        assert_eq!(mrpack_loader_key(LoaderKind::Forge), Some("forge"));
        assert_eq!(mrpack_loader_key(LoaderKind::Vanilla), None);
    }

    #[test]
    fn index_from_another_launcher_supplies_version_and_loader() {
        let raw = r#"{
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.21.1",
            "name": "Someone else's pack",
            "files": [],
            "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.5" }
        }"#;
        let index: MrpackIndex = serde_json::from_str(raw).expect("parse");
        assert_eq!(index.loader(), LoaderKind::Fabric);
        assert_eq!(index.loader_version(), Some("0.16.5"));
        assert_eq!(index.game_version().as_deref(), Some("1.21.1"));
    }

    #[test]
    fn index_without_a_loader_is_vanilla() {
        let raw = r#"{
            "dependencies": { "minecraft": "1.20.1" },
            "files": [{ "path": "mods/a.jar", "downloads": ["https://cdn/modrinth/x.jar"],
                        "hashes": { "sha1": "abc" }, "fileSize": 12 }]
        }"#;
        let index: MrpackIndex = serde_json::from_str(raw).expect("parse");
        assert_eq!(index.loader(), LoaderKind::Vanilla);
        assert!(index.loader_version().is_none());
        assert_eq!(index.files[0].file_size, Some(12));
        assert_eq!(index.files[0].hashes.get("sha1"), Some(&"abc".to_string()));
    }

    #[test]
    fn overrides_never_carry_scratch_data() {
        assert!(is_overrides_excluded("session.lock"));
        assert!(is_overrides_excluded("saves/world/session.lock"));
        assert!(is_overrides_excluded("logs/game.log"));
        assert!(is_overrides_excluded("crash-reports/crash.txt"));
        assert!(is_overrides_excluded(".spud/mods.json"));
        assert!(!is_overrides_excluded("mods/sodium.jar"));
        assert!(!is_overrides_excluded("saves/world/level.dat"));
        assert!(!is_overrides_excluded("options.txt"));
    }
}
