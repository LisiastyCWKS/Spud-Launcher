use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use launcher_core::paths::LauncherPaths;
use launcher_core::version::{
    library_destination, load_merged, resolve_libraries, save_version_json, AssetIndex, OsInfo,
    VersionJson, VersionManifest,
};
use launcher_core::LoaderKind;
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::cache::safe_file_name;
use crate::download::{
    DownloadEvent, DownloadManager, DownloadSpec, DownloadSummary, ProgressTracker,
};
use crate::{IoContext, NetError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    pub version_id: String,
    pub files: usize,
    pub bytes: u64,
}

/// Installs vanilla Minecraft versions and loader profiles into the shared
/// meta/libraries/assets tree. Instances reference this tree; they never copy
/// game jars.
#[derive(Clone)]
pub struct GameInstaller {
    paths: LauncherPaths,
    manager: DownloadManager,
    meta_url: String,
}

impl GameInstaller {
    pub fn new(paths: LauncherPaths, manager: DownloadManager, meta_url: impl Into<String>) -> Self {
        Self {
            paths,
            manager,
            meta_url: meta_url.into(),
        }
    }

    pub fn paths(&self) -> &LauncherPaths {
        &self.paths
    }

    /// Fetch (and cache) the Mojang version manifest.
    pub async fn manifest(&self, force: bool) -> Result<VersionManifest> {
        let cache = self.paths.meta_dir().join("version_manifest_v2.json");
        if !force && cache.is_file() {
            if let Ok(raw) = std::fs::read(&cache) {
                if let Ok(manifest) = serde_json::from_slice::<VersionManifest>(&raw) {
                    return Ok(manifest);
                }
            }
        }

        let response = self.manager.client().get(&self.meta_url).send().await?;
        if !response.status().is_success() {
            return Err(NetError::HttpStatus {
                status: response.status().as_u16(),
                url: self.meta_url.clone(),
            });
        }
        let manifest: VersionManifest = response.json().await?;
        if manifest.versions.is_empty() {
            return Err(NetError::Manifest("version manifest is empty".into()));
        }
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        if let Some(parent) = cache.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        std::fs::write(&cache, bytes).at(&cache)?;
        Ok(manifest)
    }

    /// Ensure the vanilla client, libraries, logging config and all assets are
    /// present. Safe to call repeatedly; verified files are skipped.
    pub async fn install_vanilla(
        &self,
        version_id: &str,
        progress: Option<UnboundedSender<DownloadEvent>>,
    ) -> Result<InstallOutcome> {
        let mut manifest = self.manifest(false).await?;
        if !manifest.versions.iter().any(|v| v.id == version_id) {
            manifest = self.manifest(true).await?;
        }
        let summary = manifest
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .ok_or_else(|| NetError::Manifest(format!("unknown Minecraft version `{version_id}`")))?;

        let json_path = self.paths.version_json(version_id);
        if !json_path.is_file() {
            self.manager
                .fetch(
                    DownloadSpec::new(&summary.url, &json_path)
                        .with_sha1(summary.sha1.clone()),
                )
                .await?;
        }

        let raw = std::fs::read(&json_path).at(&json_path)?;
        let version: VersionJson = serde_json::from_slice(&raw)?;
        if version.id != version_id {
            return Err(NetError::Manifest(format!(
                "version JSON id mismatch: expected {version_id}, got {}",
                version.id
            )));
        }

        // --- core files: client jar, libraries, logging config -------------
        let os = launcher_core::version::OsInfo::current();
        let libraries = resolve_libraries(&version.libraries, &os, &HashMap::new());
        let mut specs: Vec<DownloadSpec> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let client = version
            .downloads
            .client
            .as_ref()
            .ok_or_else(|| NetError::Manifest("version has no client download".into()))?;
        specs.push(
            DownloadSpec::new(&client.url, self.paths.version_jar(version_id))
                .with_sha1(client.sha1.clone())
                .with_size(client.size),
        );

        for lib in &libraries {
            if lib.url.is_empty() {
                continue;
            }
            let dest = library_destination(&self.paths.libraries_dir(), &lib.relative_path)?;
            if !seen.insert(dest.clone()) {
                continue;
            }
            specs.push(
                DownloadSpec::new(&lib.url, dest)
                    .with_sha1(lib.sha1.clone())
                    .with_size(lib.size),
            );
        }

        let asset_index = version
            .asset_index
            .as_ref()
            .ok_or_else(|| NetError::Manifest("version has no asset index".into()))?;
        let index_path = self
            .paths
            .asset_indexes_dir()
            .join(format!("{}.json", asset_index.id));
        specs.push(
            DownloadSpec::new(&asset_index.url, &index_path)
                .with_sha1(asset_index.sha1.clone())
                .with_size(asset_index.size),
        );

        let logging_file = version
            .logging
            .as_ref()
            .and_then(|l| l.client.as_ref())
            .map(|client| {
                let name = client
                    .file
                    .id
                    .clone()
                    .or_else(|| client.file.path.clone())
                    .or_else(|| client.file.url.rsplit('/').next().map(str::to_owned))
                    .unwrap_or_else(|| format!("{version_id}-client.xml"));
                DownloadSpec::new(
                    &client.file.url,
                    self.paths.log_configs_dir().join(safe_file_name(&name)),
                )
                .with_sha1(client.file.sha1.clone())
                .with_size(client.file.size)
            });
        if let Some(spec) = logging_file {
            specs.push(spec);
        }

        // Download the index first if it is not on disk, so asset specs can be
        // queued in the same progress batch.
        let index_spec = specs
            .iter()
            .position(|s| s.dest == index_path)
            .map(|i| specs.remove(i));
        if !index_path.is_file() {
            if let Some(spec) = index_spec.clone() {
                self.manager.fetch(spec).await?;
            }
        }

        // --- assets --------------------------------------------------------
        let raw = std::fs::read(&index_path).at(&index_path)?;
        let index: AssetIndex = serde_json::from_slice(&raw)?;
        for object in index.objects.values() {
            if object.hash.len() < 2 {
                continue;
            }
            let prefix = &object.hash[..2];
            let dest = self
                .paths
                .asset_objects_dir()
                .join(prefix)
                .join(&object.hash);
            if !seen.insert(dest.clone()) {
                continue;
            }
            if dest.is_file() {
                continue; // assets are immutable, hash path is enough
            }
            specs.push(
                DownloadSpec::new(
                    format!("https://resources.download.minecraft.net/{prefix}/{}", object.hash),
                    dest,
                )
                .with_sha1(Some(object.hash.clone()))
                .with_size(Some(object.size)),
            );
        }

        let tracker = ProgressTracker::new(specs.len(), progress);
        let summary = self.manager.fetch_all(specs, tracker).await?;

        // Legacy versions read assets from a materialised tree.
        if index.is_virtual() {
            self.materialise_virtual_assets(&asset_index.id, &index)?;
        }

        Ok(InstallOutcome {
            version_id: version_id.to_string(),
            files: summary.files,
            bytes: summary.bytes,
        })
    }

    /// Ensure every library referenced by the merged profile exists locally.
    ///
    /// Vanilla installs only cover Mojang's own libraries. Fabric and Quilt
    /// profiles declare their loader and Mixin/ASM libraries from their Maven,
    /// and Forge installers can leave stragglers behind — without this step the
    /// launch fails with a bare "file(s) missing" list.
    pub async fn ensure_version_files(
        &self,
        version_id: &str,
        progress: Option<UnboundedSender<DownloadEvent>>,
    ) -> Result<DownloadSummary> {
        let merged = load_merged(&self.paths, version_id)?;
        let os = OsInfo::current();
        let libraries = merged.libraries_for(&os, &HashMap::new());

        let mut seen = std::collections::HashSet::new();
        let mut specs: Vec<DownloadSpec> = Vec::new();
        for lib in libraries {
            if lib.url.is_empty() {
                continue;
            }
            let dest = library_destination(&self.paths.libraries_dir(), &lib.relative_path)?;
            if !seen.insert(dest.clone()) || dest.is_file() {
                continue;
            }
            specs.push(
                DownloadSpec::new(&lib.url, dest)
                    .with_sha1(lib.sha1.clone())
                    .with_size(lib.size),
            );
        }

        if specs.is_empty() {
            return Ok(DownloadSummary { files: 0, bytes: 0 });
        }
        let tracker = ProgressTracker::new(specs.len(), progress);
        self.manager.fetch_all(specs, tracker).await
    }

    /// Install a loader profile. Vanilla must already be installed.
    /// Returns the version id that `instance.json` should reference.
    pub async fn install_loader(
        &self,
        mc_version: &str,
        loader: LoaderKind,
        loader_version: Option<&str>,
        java: &Path,
        progress: Option<UnboundedSender<DownloadEvent>>,
    ) -> Result<String> {
        match loader {
            LoaderKind::Vanilla => Ok(mc_version.to_string()),
            LoaderKind::Fabric | LoaderKind::Quilt => {
                let loader_version = loader_version.ok_or_else(|| {
                    NetError::Manifest(format!("{loader} requires an explicit loader version"))
                })?;
                let url = match loader {
                    LoaderKind::Fabric => format!(
                        "https://meta.fabricmc.net/v2/versions/loader/{mc_version}/{loader_version}/profile/json"
                    ),
                    _ => format!(
                        "https://meta.quiltmc.org/v3/versions/loader/{mc_version}/{loader_version}/profile/json"
                    ),
                };
                let response = self.manager.client().get(&url).send().await?;
                if !response.status().is_success() {
                    return Err(NetError::HttpStatus {
                        status: response.status().as_u16(),
                        url,
                    });
                }
                let mut profile: VersionJson = response.json().await?;
                if profile.id.is_empty() {
                    profile.id = match loader {
                        LoaderKind::Fabric => format!("fabric-loader-{loader_version}-{mc_version}"),
                        _ => format!("quilt-loader-{loader_version}-{mc_version}"),
                    };
                }
                profile.inherits_from = Some(mc_version.to_string());
                save_version_json(&self.paths, &profile)?;
                // Loader libraries live on Fabric's/Quilt's Maven, not Mojang's.
                self.ensure_version_files(&profile.id, progress).await?;
                Ok(profile.id)
            }
            LoaderKind::Forge | LoaderKind::NeoForge => {
                let loader_version = loader_version.ok_or_else(|| {
                    NetError::Manifest(format!("{loader} requires an explicit loader version"))
                })?;
                self.install_forge_like(
                    mc_version,
                    loader,
                    loader_version,
                    java,
                    progress,
                )
                .await
            }
        }
    }

    /// Forge and NeoForge installers are official jars. They run against a
    /// disposable staging directory, then their version JSON and libraries are
    /// copied into the shared tree.
    async fn install_forge_like(
        &self,
        mc_version: &str,
        loader: LoaderKind,
        loader_version: &str,
        java: &Path,
        progress: Option<UnboundedSender<DownloadEvent>>,
    ) -> Result<String> {
        let staging = tempfile::Builder::new()
            .prefix("spud-forge-")
            .tempdir_in(self.paths.cache_dir())
            .at(self.paths.cache_dir())?;
        let mc_dir = staging.path().join("minecraft");
        let versions_dir = mc_dir.join("versions").join(mc_version);
        std::fs::create_dir_all(&versions_dir).at(&versions_dir)?;

        // The installer expects an official launcher profile and the vanilla
        // version JSON+jar to exist in the target directory.
        std::fs::write(
            mc_dir.join("launcher_profiles.json"),
            br#"{"profiles":{},"settings":{"language":"en_us"},"version":3}"#,
        )
        .at(mc_dir.join("launcher_profiles.json"))?;
        let src_json = self.paths.version_json(mc_version);
        let src_jar = self.paths.version_jar(mc_version);
        if !src_json.is_file() || !src_jar.is_file() {
            return Err(NetError::Manifest(format!(
                "install vanilla {mc_version} before the loader"
            )));
        }
        std::fs::copy(&src_json, versions_dir.join(format!("{mc_version}.json")))
            .at(versions_dir.join(format!("{mc_version}.json")))?;
        std::fs::copy(&src_jar, versions_dir.join(format!("{mc_version}.jar")))
            .at(versions_dir.join(format!("{mc_version}.jar")))?;

        let installer_url = forge_installer_url(loader, mc_version, loader_version);
        let installer_name = installer_url.rsplit('/').next().unwrap_or("installer.jar");
        let installer_path = self
            .paths
            .download_cache_dir()
            .join(safe_file_name(installer_name));
        self.manager
            .fetch(DownloadSpec::new(&installer_url, &installer_path))
            .await?;

        run_installer(java, &installer_path, &mc_dir).await?;

        let mut installed = Vec::new();
        let installed_root = mc_dir.join("versions");
        for entry in std::fs::read_dir(&installed_root).at(&installed_root)? {
            let entry = entry.at(&installed_root)?;
            if !entry.file_type().at(entry.path())?.is_dir() {
                continue;
            }
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if id == mc_version {
                continue;
            }
            let json_path = entry.path().join(format!("{id}.json"));
            if !json_path.is_file() {
                continue;
            }
            let dest_dir = self.paths.meta_dir().join("versions").join(&id);
            std::fs::create_dir_all(&dest_dir).at(&dest_dir)?;
            std::fs::copy(&json_path, dest_dir.join(format!("{id}.json")))
                .at(dest_dir.join(format!("{id}.json")))?;
            let jar = entry.path().join(format!("{id}.jar"));
            if jar.is_file() {
                std::fs::copy(&jar, dest_dir.join(format!("{id}.jar")))
                    .at(dest_dir.join(format!("{id}.jar")))?;
            }
            installed.push(id);
        }

        if installed.is_empty() {
            return Err(NetError::Manifest(
                "loader installer produced no version profile".into(),
            ));
        }

        let copied = copy_dir_all(&mc_dir.join("libraries"), &self.paths.libraries_dir())?;
        tracing::info!(libraries = copied, "copied loader libraries into the shared store");

        installed.sort();
        let version_id = installed.remove(0);
        if let Some(tx) = progress {
            let _ = tx.send(DownloadEvent::Finished {
                files: copied,
                bytes: 0,
            });
        }
        Ok(version_id)
    }

    /// Legacy (<1.7) asset layout: every object also exists under its display
    /// name in `assets/virtual/<index>/`.
    fn materialise_virtual_assets(&self, index_id: &str, index: &AssetIndex) -> Result<()> {
        let virtual_root = self.paths.asset_virtual_dir(index_id);
        std::fs::create_dir_all(&virtual_root).at(&virtual_root)?;
        for (name, object) in &index.objects {
            if object.hash.len() < 2 {
                continue;
            }
            let source = self
                .paths
                .asset_objects_dir()
                .join(&object.hash[..2])
                .join(&object.hash);
            if !source.is_file() {
                continue;
            }
            // Asset names contain subdirectories and must not escape the root.
            let relative = Path::new(name);
            if relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                continue;
            }
            let dest = virtual_root.join(relative);
            if dest.exists() {
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            if std::fs::hard_link(&source, &dest).is_err() {
                std::fs::copy(&source, &dest).at(&dest)?;
            }
        }
        Ok(())
    }
}

fn forge_installer_url(loader: LoaderKind, mc_version: &str, loader_version: &str) -> String {
    match loader {
        LoaderKind::NeoForge if mc_version == "1.20.1" => format!(
            "https://maven.neoforged.net/releases/net/neoforged/forge/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        ),
        LoaderKind::NeoForge => format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
        ),
        _ => format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc_version}-{loader_version}/forge-{mc_version}-{loader_version}-installer.jar"
        ),
    }
}

async fn run_installer(java: &Path, installer: &Path, target: &Path) -> Result<()> {
    let mut command = tokio::process::Command::new(java);
    command
        .arg("-jar")
        .arg(installer)
        .arg("--installClient")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(Duration::from_secs(900), command.output())
        .await
        .map_err(|_| NetError::Process("loader installer timed out after 15 minutes".into()))?
        .map_err(|source| NetError::Io {
            path: installer.to_path_buf(),
            source,
        })?;

    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        let tail = tail.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(NetError::Process(format!(
            "loader installer exited with {}: {tail}",
            output.status
        )));
    }
    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<usize> {
    if !source.is_dir() {
        return Ok(0);
    }
    let mut copied = 0;
    std::fs::create_dir_all(destination).at(destination)?;
    for entry in std::fs::read_dir(source).at(source)? {
        let entry = entry.at(source)?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if entry.file_type().at(&path)?.is_dir() {
            copied += copy_dir_all(&path, &target)?;
        } else if entry.file_type().at(&path)?.is_file() && !target.exists() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            std::fs::copy(&path, &target).at(&target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Path where a loader version JSON is stored, for diagnostics.
pub fn loader_profile_path(paths: &LauncherPaths, id: &str) -> PathBuf {
    paths.version_json(id)
}
