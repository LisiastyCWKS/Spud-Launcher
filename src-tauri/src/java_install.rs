//! Temurin JRE downloads for the launcher-managed runtime directory.
//!
//! Everything lives under `<root>/java/temurin-<major>/`, which the regular
//! `java::discover` scan already walks — so an auto-installed runtime is picked
//! up exactly like one the user installed by hand, with no separate registry.
//!
//! The Adoptium API is the source of truth for "latest JRE for this feature
//! release on this OS/arch"; the archive checksum it hands back is verified
//! before anything is unpacked.

use std::io::Write;
use std::path::{Path, PathBuf};

use launcher_core::java::JavaRuntime;
use launcher_core::LauncherPaths;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const ADOPTIUM_API: &str = "https://api.adoptium.net/v3/assets/latest";

/// One release of the Adoptium asset API — only the fields this launcher uses.
#[derive(Debug, Deserialize)]
struct AssetRelease {
    binary: AssetBinary,
    version: AssetVersion,
}

#[derive(Debug, Deserialize)]
struct AssetBinary {
    package: AssetPackage,
}

#[derive(Debug, Deserialize)]
struct AssetPackage {
    checksum: String,
    link: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct AssetVersion {
    semver: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JavaInstallResult {
    pub major: u32,
    pub version: String,
    pub path: PathBuf,
    pub bytes: u64,
}

fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

fn arch_name() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

fn java_binary() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
}

/// Ask Adoptium for the newest JRE of `major`, download it, verify the SHA-256
/// from the API response, and unpack it under the managed runtime directory.
pub async fn install(
    http: &reqwest::Client,
    paths: &LauncherPaths,
    major: u32,
) -> Result<JavaInstallResult, String> {
    let url = format!(
        "{ADOPTIUM_API}/{major}/hotspot?architecture={}&image_type=jre&os={}&vendor=eclipse",
        arch_name(),
        os_name()
    );

    let releases: Vec<AssetRelease> = http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("cannot query Adoptium: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Adoptium rejected the request: {e}"))?
        .json()
        .await
        .map_err(|e| format!("unexpected Adoptium response: {e}"))?;

    let release = releases
        .into_iter()
        .next()
        .ok_or_else(|| format!("Adoptium has no JRE {major} build for this platform"))?;

    let bytes = http
        .get(&release.binary.package.link)
        .send()
        .await
        .map_err(|e| format!("cannot download the JRE: {e}"))?
        .error_for_status()
        .map_err(|e| format!("the JRE download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("the JRE download was interrupted: {e}"))?;

    let digest = hex::encode(Sha256::digest(&bytes));
    if !digest.eq_ignore_ascii_case(release.binary.package.checksum.trim()) {
        return Err(format!(
            "checksum mismatch for {}: expected {}, got {digest}",
            release.binary.package.name, release.binary.package.checksum
        ));
    }

    let archive_size = bytes.len() as u64;
    let target = paths.root().join("java").join(format!("temurin-{major}"));
    let staging = paths
        .root()
        .join("java")
        .join(format!("temurin-{major}.unpacking"));

    // A previous run may have died mid-unpack; the staging tree is disposable.
    for dir in [&staging, &target] {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| format!("cannot clear {}: {e}", dir.display()))?;
        }
    }
    std::fs::create_dir_all(&staging).map_err(|e| format!("cannot create the runtime folder: {e}"))?;

    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| format!("corrupt JRE archive: {e}"))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("cannot read the JRE archive: {e}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue; // refuses absolute paths and `..` — the archive is untrusted
        };
        let out = staging.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("cannot unpack: {e}"))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot unpack: {e}"))?;
        }
        let mut file =
            std::fs::File::create(&out).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
        std::io::copy(&mut entry, &mut file).map_err(|e| format!("cannot write the JRE: {e}"))?;
        file.flush().ok();
    }

    // Archives normally wrap everything in one `jdk-21.0.5+11-jre/` folder;
    // flatten it so the runtime path is predictable.
    let source = single_top_dir(&staging).unwrap_or_else(|| staging.clone());
    std::fs::create_dir_all(target.parent().unwrap_or(Path::new("."))).ok();
    std::fs::rename(&source, &target).map_err(|e| {
        format!(
            "cannot move the unpacked JRE into {}: {e}",
            target.display()
        )
    })?;
    if staging.exists() {
        std::fs::remove_dir_all(&staging).ok();
    }

    let binary = find_binary(&target).ok_or_else(|| {
        format!(
            "the unpacked JRE has no {} in {}",
            java_binary(),
            target.display()
        )
    })?;

    Ok(JavaInstallResult {
        major,
        version: release.version.semver,
        path: binary,
        bytes: archive_size,
    })
}

/// Probe an already-unpacked managed runtime without touching the network.
pub async fn installed(paths: &LauncherPaths, major: u32) -> Option<JavaRuntime> {
    let root = paths.root().join("java").join(format!("temurin-{major}"));
    let binary = find_binary(&root)?;
    launcher_core::java::inspect(&binary).await
}

fn single_top_dir(root: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(root).ok()?.flatten();
    let first = entries.next()?.path();
    if first.is_dir() && entries.next().is_none() {
        Some(first)
    } else {
        None
    }
}

fn find_binary(root: &Path) -> Option<PathBuf> {
    let direct = root.join("bin").join(java_binary());
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin").join(java_binary());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
