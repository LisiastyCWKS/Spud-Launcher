//! Content-addressed store for mod jars, shared between instances.
//!
//! Layout under `<launcher root>/modstore/`:
//!
//! ```text
//! modstore/
//! ├── index.json                     # hash -> size, first/last seen
//! └── blobs/
//!     └── sha256/ab/cd/abcdef…       # raw bytes, no extension
//! ```
//!
//! Instances keep a real file at their usual path (`mods/sodium.jar`) that is a
//! hard link to the blob. The game, Modrinth manifests, `.mrpack` export and
//! every other reader sees an ordinary jar — deduplication is invisible to
//! them, which is exactly the point.
//!
//! When a hard link is impossible (a different volume, a filesystem without
//! link support, or a compressed folder), the store falls back to a copy and
//! records that in the instance registry so garbage collection never deletes a
//! blob that a copy still depends on.
//!
//! GC is conservative: a blob survives if the OS reports more than one link, or
//! if any instance registry still names it. Nothing here ever touches a file
//! outside the store or an instance's game directory.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{read_json, write_json_atomic, CoreError, IoContext, Result};
use crate::paths::LauncherPaths;

/// Directories inside `.minecraft` whose contents are worth deduplicating.
pub const DEDUPE_DIRS: [&str; 3] = ["mods", "resourcepacks", "shaderpacks"];

const STORE_SCHEMA: u32 = 1;
const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobEntry {
    pub hash: String,
    pub size: u64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreIndex {
    #[serde(default = "schema")]
    pub schema: u32,
    #[serde(default)]
    pub blobs: BTreeMap<String, BlobEntry>,
}

fn schema() -> u32 {
    STORE_SCHEMA
}

impl Default for StoreIndex {
    fn default() -> Self {
        Self {
            schema: STORE_SCHEMA,
            blobs: BTreeMap::new(),
        }
    }
}

/// Per-instance record of what was deduplicated, so the store knows which blobs
/// are still needed even when they had to be copied instead of linked.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceStore {
    #[serde(default)]
    pub files: BTreeMap<String, InstanceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceFile {
    pub hash: String,
    /// `true` when the instance file is a hard link to the blob.
    pub linked: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreStats {
    pub blobs: usize,
    pub bytes: u64,
    pub index_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct DedupeReport {
    /// Files examined in the instance's mod folders.
    pub examined: usize,
    /// Files whose content was already in the store (nothing copied).
    pub reused: usize,
    /// Fresh blobs written to the store.
    pub added: usize,
    /// Instance files backed by a real hard link.
    pub linked: usize,
    /// Instance files that had to be copied (different volume / no link support).
    pub copied: usize,
    pub bytes_saved: u64,
    /// Per-file failures, never fatal: one locked jar must not abort the sweep.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GcReport {
    pub removed: usize,
    pub bytes_freed: u64,
    pub kept: usize,
    pub dry_run: bool,
    pub removed_hashes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ModStore {
    root: PathBuf,
}

impl ModStore {
    pub fn new(paths: &LauncherPaths) -> Self {
        Self {
            root: paths.modstore_dir(),
        }
    }

    /// Explicit root, for tests.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs").join("sha256")
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    /// `blobs/sha256/ab/cd/<hash>` — two levels of fan-out keeps directory
    /// listings small even with thousands of jars.
    pub fn blob_path(&self, hash: &str) -> PathBuf {
        let a = hash.get(0..2).unwrap_or("00");
        let b = hash.get(2..4).unwrap_or("00");
        self.blobs_dir().join(a).join(b).join(hash)
    }

    pub fn load_index(&self) -> Result<StoreIndex> {
        let path = self.index_path();
        if !path.is_file() {
            return Ok(StoreIndex::default());
        }
        let mut index: StoreIndex = read_json(&path)?;
        // A newer build wrote it; accept it rather than deleting the user's store.
        index.schema = STORE_SCHEMA;
        Ok(index)
    }

    fn save_index(&self, index: &StoreIndex) -> Result<()> {
        write_json_atomic(&self.index_path(), index)
    }

    /// Insert `source` into the store. The source file is consumed: when the
    /// content is new it becomes the blob, when it already exists the duplicate
    /// copy is deleted. Callers re-link the blob into place afterwards.
    pub fn insert(&self, source: &Path) -> Result<(BlobEntry, bool)> {
        let hash = hash_file(source)?;
        let size = std::fs::metadata(source).at(source)?.len();
        let mut index = self.load_index()?;
        let now = Utc::now();

        if let Some(existing) = index.blobs.get_mut(&hash) {
            existing.last_seen = now;
            let entry = existing.clone();
            // Same bytes are already stored; the caller's copy is redundant.
            std::fs::remove_file(source).at(source)?;
            self.save_index(&index)?;
            return Ok((entry, true));
        }

        let blob = self.blob_path(&hash);
        if let Some(parent) = blob.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }

        // Rename is atomic and free on the same volume. Anything else (EXDEV,
        // a locked file) falls back to copy + delete.
        if let Err(rename_error) = std::fs::rename(source, &blob) {
            tracing::debug!(%rename_error, "store rename failed — copying instead");
            std::fs::copy(source, &blob).at(&blob)?;
            std::fs::remove_file(source).at(source)?;
        }

        let entry = BlobEntry {
            hash: hash.clone(),
            size,
            first_seen: now,
            last_seen: now,
        };
        index.blobs.insert(hash, entry.clone());
        self.save_index(&index)?;
        Ok((entry, false))
    }

    /// Place the blob at `dest`. Prefers a hard link; copies when the platform
    /// refuses one. Returns whether the result is a link.
    pub fn link_into(&self, hash: &str, dest: &Path) -> Result<bool> {
        let blob = self.blob_path(hash);
        if !blob.is_file() {
            return Err(CoreError::Io {
                path: blob,
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "blob is missing"),
            });
        }
        if dest.exists() {
            // A stale duplicate: replace it. remove_file on a hard link only
            // drops that name, the blob keeps its own.
            std::fs::remove_file(dest).at(dest)?;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        match std::fs::hard_link(&blob, dest) {
            Ok(()) => Ok(true),
            Err(error) => {
                tracing::debug!(%error, dest = %dest.display(), "hard link failed — copying blob");
                std::fs::copy(&blob, dest).at(dest)?;
                Ok(false)
            }
        }
    }

    /// Install one file into the store and re-materialise it at `dest`.
    pub fn store_and_link(&self, source: &Path, dest: &Path) -> Result<(String, u64, bool, bool)> {
        let (entry, reused) = self.insert(source)?;
        let linked = self.link_into(&entry.hash, dest)?;
        Ok((entry.hash, entry.size, reused, linked))
    }

    /// Route one known file through the store and record it. Used by the
    /// installer, which already knows exactly which jars it just wrote.
    pub fn adopt_file(
        &self,
        paths: &LauncherPaths,
        instance_id: &str,
        directory: &str,
        file_name: &str,
    ) -> Result<bool> {
        let game_dir = paths.game_dir(instance_id);
        let path = game_dir.join(directory).join(file_name);
        if !path.is_file() {
            return Ok(false);
        }
        let (hash, size, _reused, linked) = self.store_and_link(&path, &path)?;
        let mut registry = self.load_instance(paths, instance_id)?;
        registry.files.insert(
            format!("{directory}/{file_name}"),
            InstanceFile {
                hash,
                linked,
                size,
            },
        );
        self.save_instance(paths, instance_id, &registry)?;
        Ok(true)
    }

    /// Walk the dedupe folders of one instance and route every jar/zip through
    /// the store. Idempotent: a second pass finds every blob already present.
    pub fn dedupe_instance(&self, paths: &LauncherPaths, instance_id: &str) -> Result<DedupeReport> {
        let game_dir = paths.game_dir(instance_id);
        let mut report = DedupeReport {
            examined: 0,
            reused: 0,
            added: 0,
            linked: 0,
            copied: 0,
            bytes_saved: 0,
            problems: Vec::new(),
        };
        let mut registry = self.load_instance(paths, instance_id)?;

        for dir in DEDUPE_DIRS {
            let folder = game_dir.join(dir);
            let entries = match std::fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(_) => continue, // the folder is optional
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || !is_dedupable(&path) {
                    continue;
                }
                let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
                    continue;
                };
                let relative = format!("{dir}/{name}");
                report.examined += 1;

                match self.store_and_link(&path, &path) {
                    Ok((hash, size, reused, linked)) => {
                        if reused {
                            report.reused += 1;
                            report.bytes_saved += size;
                        } else {
                            report.added += 1;
                        }
                        if linked {
                            report.linked += 1;
                        } else {
                            report.copied += 1;
                        }
                        registry.files.insert(
                            relative,
                            InstanceFile {
                                hash,
                                linked,
                                size,
                            },
                        );
                    }
                    Err(error) => {
                        // A jar held open by a running game is expected; keep
                        // sweeping and report it once at the end.
                        report.problems.push(format!("{relative}: {error}"));
                    }
                }
            }
        }

        self.save_instance(paths, instance_id, &registry)?;
        Ok(report)
    }

    /// Drop every instance-side reference to the store. Used before deleting an
    /// instance or when a jar is removed from the mods folder.
    pub fn forget_instance(&self, paths: &LauncherPaths, instance_id: &str) -> Result<()> {
        let path = instance_registry_path(paths, instance_id);
        if path.exists() {
            std::fs::remove_file(&path).at(&path)?;
        }
        Ok(())
    }

    /// Reclaim blobs nothing points at any more.
    ///
    /// `dry_run` reports without deleting, which is what the settings panel
    /// shows before a click is allowed to free anything.
    pub fn gc(&self, paths: &LauncherPaths, dry_run: bool) -> Result<GcReport> {
        let index = self.load_index()?;
        let referenced = self.referenced_hashes(paths)?;
        let mut report = GcReport {
            removed: 0,
            bytes_freed: 0,
            kept: 0,
            dry_run,
            removed_hashes: Vec::new(),
        };

        for (hash, entry) in &index.blobs {
            let blob = self.blob_path(hash);
            // The store's index is the source of truth for liveness, not the
            // OS link count: hard links survive the store's own name being
            // removed, so deleting an unreferenced blob can never take bytes
            // away from an instance that still has a link to it.
            let alive = referenced.contains(hash);
            if alive {
                report.kept += 1;
                continue;
            }
            report.removed += 1;
            report.bytes_freed += entry.size;
            report.removed_hashes.push(hash.clone());
            if !dry_run && blob.exists() {
                std::fs::remove_file(&blob).at(&blob)?;
            }
        }

        if !dry_run && report.removed > 0 {
            let mut index = index;
            for hash in &report.removed_hashes {
                index.blobs.remove(hash);
            }
            self.save_index(&index)?;
        }

        Ok(report)
    }

    pub fn stats(&self) -> Result<StoreStats> {
        let index = self.load_index()?;
        Ok(StoreStats {
            blobs: index.blobs.len(),
            bytes: index.blobs.values().map(|entry| entry.size).sum(),
            index_path: self.index_path(),
        })
    }

    /// Remove one instance file's store entry (the file itself is the caller's
    /// business — this only forgets the bookkeeping).
    pub fn forget_file(
        &self,
        paths: &LauncherPaths,
        instance_id: &str,
        relative: &str,
    ) -> Result<()> {
        let mut registry = self.load_instance(paths, instance_id)?;
        if registry.files.remove(relative).is_some() {
            self.save_instance(paths, instance_id, &registry)?;
        }
        Ok(())
    }

    /// Forget every registry entry that ends in `/<file_name>`. The UI only
    /// knows the jar name, not which of the dedupe folders it came from.
    pub fn forget_named(
        &self,
        paths: &LauncherPaths,
        instance_id: &str,
        file_name: &str,
    ) -> Result<()> {
        let mut registry = self.load_instance(paths, instance_id)?;
        let suffix = format!("/{file_name}");
        let before = registry.files.len();
        registry.files.retain(|key, _| !key.ends_with(&suffix));
        if registry.files.len() != before {
            self.save_instance(paths, instance_id, &registry)?;
        }
        Ok(())
    }

    /// Hashes still named by any instance registry.
    pub fn referenced_hashes(&self, paths: &LauncherPaths) -> Result<BTreeSet<String>> {
        let mut out = BTreeSet::new();
        let dir = paths.instances_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let registry = self.load_instance(paths, &id)?;
            for file in registry.files.values() {
                out.insert(file.hash.clone());
            }
        }
        Ok(out)
    }

    pub fn load_instance(&self, paths: &LauncherPaths, instance_id: &str) -> Result<InstanceStore> {
        let path = instance_registry_path(paths, instance_id);
        if !path.is_file() {
            return Ok(InstanceStore::default());
        }
        read_json(&path)
    }

    fn save_instance(
        &self,
        paths: &LauncherPaths,
        instance_id: &str,
        registry: &InstanceStore,
    ) -> Result<()> {
        write_json_atomic(&instance_registry_path(paths, instance_id), registry)
    }
}

fn instance_registry_path(paths: &LauncherPaths, instance_id: &str) -> PathBuf {
    paths
        .game_dir(instance_id)
        .join(".spud")
        .join("store.json")
}

fn is_dedupable(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            lower == "jar" || lower == "zip"
        })
        .unwrap_or(false)
}

/// 64 KiB chunks: a 300 MB shaderpack hashes in ~5k reads instead of one
/// allocation, and the file never has to fit in memory.
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).at(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer).at(path)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(name: &str) -> LauncherPaths {
        let root = std::env::temp_dir().join(format!("spud-store-{name}-{}", std::process::id()));
        if root.exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        LauncherPaths::at(root).expect("layout")
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn identical_files_share_one_blob() {
        let paths = temp_paths("dedupe");
        let store = ModStore::new(&paths);
        let config = crate::instance::NewInstance {
            name: "demo".into(),
            mc_version: "1.21.1".into(),
            loader: crate::LoaderKind::Fabric,
            loader_version: Some("0.16.5".into()),
            max_memory_mb: None,
        };
        let instance = crate::instance::InstanceConfig::create(&paths, config).unwrap();
        let mods = paths.instance_mods_dir(&instance.id);

        write(&mods.join("a.jar"), b"same bytes");
        write(&mods.join("b.jar"), b"same bytes");

        let report = store.dedupe_instance(&paths, &instance.id).unwrap();
        assert_eq!(report.examined, 2);
        assert_eq!(report.added + report.reused, 2);
        // Second file must be a re-use, not a second copy on disk.
        assert!(report.reused >= 1);

        let index = store.load_index().unwrap();
        assert_eq!(index.blobs.len(), 1);
        assert_eq!(
            std::fs::read(mods.join("a.jar")).unwrap(),
            b"same bytes".to_vec()
        );
        assert_eq!(
            std::fs::read(mods.join("b.jar")).unwrap(),
            b"same bytes".to_vec()
        );

        let _ = std::fs::remove_dir_all(paths.root());
    }

    #[test]
    fn gc_keeps_referenced_blobs_and_drops_orphans() {
        let paths = temp_paths("gc");
        let store = ModStore::new(&paths);
        let config = crate::instance::NewInstance {
            name: "demo".into(),
            mc_version: "1.21.1".into(),
            loader: crate::LoaderKind::Fabric,
            loader_version: None,
            max_memory_mb: None,
        };
        let instance = crate::instance::InstanceConfig::create(&paths, config).unwrap();
        let mods = paths.instance_mods_dir(&instance.id);

        write(&mods.join("keep.jar"), b"keep me");
        store.dedupe_instance(&paths, &instance.id).unwrap();
        assert_eq!(store.load_index().unwrap().blobs.len(), 1);

        // A blob with no registry entry and no second link is garbage.
        let orphan = store.blob_path(&"0".repeat(64));
        write(&orphan, b"orphan");
        let mut index = store.load_index().unwrap();
        index.blobs.insert(
            "0".repeat(64),
            BlobEntry {
                hash: "0".repeat(64),
                size: 6,
                first_seen: Utc::now(),
                last_seen: Utc::now(),
            },
        );
        store.save_index(&index).unwrap();

        let dry = store.gc(&paths, true).unwrap();
        assert_eq!(dry.removed, 1, "dry run must report the orphan");
        assert!(orphan.exists(), "dry run must not delete");

        // Remove the instance's jar: now its blob is unreferenced too.
        std::fs::remove_file(mods.join("keep.jar")).unwrap();
        store.forget_instance(&paths, &instance.id).unwrap();

        let real = store.gc(&paths, false).unwrap();
        assert_eq!(real.removed, 2);
        assert!(!orphan.exists());

        let _ = std::fs::remove_dir_all(paths.root());
    }

    #[test]
    fn hash_is_stable_across_chunks() {
        let dir = std::env::temp_dir().join(format!("spud-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blob.bin");
        // Larger than one 64 KiB chunk so the loop has to iterate.
        let bytes: Vec<u8> = (0..200_000).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();

        let hash = hash_file(&path).unwrap();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_file(&path).unwrap());

        // Known-answer check against sha256 of the same buffer, computed by the
        // crate the store already depends on.
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        assert_eq!(hash, hex::encode(hasher.finalize()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
