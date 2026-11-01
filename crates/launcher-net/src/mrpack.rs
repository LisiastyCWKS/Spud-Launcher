use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use launcher_core::instance::safe_join;
use launcher_core::paths::LauncherPaths;
use launcher_core::LoaderKind;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::download::{DownloadEvent, DownloadManager, DownloadSpec, ProgressTracker};
use crate::{IoContext, NetError, Result};

/// `modrinth.index.json` from a `.mrpack` archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrpackManifest {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    #[serde(default)]
    pub game: String,
    #[serde(rename = "versionId", default)]
    pub version_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub files: Vec<MrpackFile>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrpackFile {
    pub path: String,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    #[serde(default)]
    pub env: Option<MrpackEnv>,
    #[serde(default)]
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    pub file_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrpackEnv {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

/// Preflight information shown before installing a pack.
#[derive(Debug, Clone, Serialize)]
pub struct MrpackInfo {
    pub name: String,
    pub version_id: String,
    pub summary: String,
    pub minecraft: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub file_count: usize,
    pub download_bytes: u64,
}

impl MrpackManifest {
    pub fn minecraft(&self) -> Option<&str> {
        self.dependencies.get("minecraft").map(String::as_str)
    }

    /// Map the dependency keys used by Modrinth to our loader enum.
    pub fn loader(&self) -> (LoaderKind, Option<String>) {
        for (key, loader) in [
            ("fabric-loader", LoaderKind::Fabric),
            ("quilt-loader", LoaderKind::Quilt),
            ("neoforge", LoaderKind::NeoForge),
            ("forge", LoaderKind::Forge),
        ] {
            if let Some(version) = self.dependencies.get(key) {
                return (loader, Some(version.clone()));
            }
        }
        (LoaderKind::Vanilla, None)
    }

    pub fn info(&self) -> MrpackInfo {
        let (loader, loader_version) = self.loader();
        MrpackInfo {
            name: self.name.clone(),
            version_id: self.version_id.clone(),
            summary: self.summary.clone(),
            minecraft: self.minecraft().unwrap_or("1.21.1").to_string(),
            loader,
            loader_version,
            file_count: self.client_files().count(),
            download_bytes: self.client_files().map(|f| f.file_size).sum(),
        }
    }

    fn client_files(&self) -> impl Iterator<Item = &MrpackFile> {
        self.files.iter().filter(|file| {
            file.env
                .as_ref()
                .and_then(|env| env.client.as_deref())
                .map(|client| client != "unsupported")
                .unwrap_or(true)
        })
    }
}

pub fn read_manifest(archive: &Path) -> Result<MrpackManifest> {
    let file = std::fs::File::open(archive).at(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| NetError::Archive(format!("{}: {e}", archive.display())))?;
    let mut entry = zip
        .by_name("modrinth.index.json")
        .map_err(|e| NetError::Archive(format!("not a Modrinth pack: {e}")))?;
    let mut raw = String::new();
    entry.read_to_string(&mut raw).at(archive)?;
    let manifest: MrpackManifest = serde_json::from_str(&raw)?;
    if manifest.format_version != 1 {
        return Err(NetError::Manifest(format!(
            "unsupported .mrpack formatVersion {}",
            manifest.format_version
        )));
    }
    Ok(manifest)
}

pub fn inspect(archive: &Path) -> Result<MrpackInfo> {
    Ok(read_manifest(archive)?.info())
}

/// Install pack files and overrides into an existing instance. The caller is
/// responsible for creating the instance and installing the loader declared in
/// the manifest.
pub async fn install_mrpack(
    paths: &LauncherPaths,
    manager: &DownloadManager,
    instance_id: &str,
    archive: &Path,
    progress: Option<UnboundedSender<DownloadEvent>>,
) -> Result<MrpackInfo> {
    let manifest = read_manifest(archive)?;
    let info = manifest.info();
    let game_dir = paths.game_dir(instance_id);
    std::fs::create_dir_all(&game_dir).at(&game_dir)?;

    let mut specs = Vec::new();
    for file in manifest.client_files() {
        let dest = safe_join(&game_dir, &file.path)?;
        let Some(url) = file.downloads.first() else {
            continue;
        };
        specs.push(
            DownloadSpec::new(url.clone(), dest)
                .with_sha1(file.hashes.get("sha1").cloned())
                .with_sha512(file.hashes.get("sha512").cloned())
                .with_size(Some(file.file_size)),
        );
    }

    let tracker = ProgressTracker::new(specs.len(), progress);
    manager.fetch_all(specs, tracker).await?;

    // Overrides are copied last so they win over pack files.
    extract_overrides(archive, &game_dir)?;
    Ok(info)
}

fn extract_overrides(archive: &Path, game_dir: &Path) -> Result<usize> {
    let file = std::fs::File::open(archive).at(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| NetError::Archive(format!("{}: {e}", archive.display())))?;
    let mut written = 0;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| NetError::Archive(format!("{}: {e}", archive.display())))?;
        if entry.is_dir() {
            continue;
        }
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        // `overrides/` and `client-overrides/` apply to the client; the
        // `server-overrides/` tree is never unpacked.
        let mut parts = enclosed.components();
        let Some(std::path::Component::Normal(root)) = parts.next() else {
            continue;
        };
        if root != "overrides" && root != "client-overrides" {
            continue;
        }
        let relative: PathBuf = parts.collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let dest = safe_join(game_dir, &relative.to_string_lossy())?;
        let mut out = std::fs::File::create(&dest).at(&dest)?;
        std::io::copy(&mut entry, &mut out).at(&dest)?;
        written += 1;
    }
    Ok(written)
}

/// Resolve a path relative to the instance game dir, used by the mod manager.
pub fn game_path(game_dir: &Path, relative: &str) -> Result<PathBuf> {
    safe_join(game_dir, relative).map_err(NetError::Core)
}
