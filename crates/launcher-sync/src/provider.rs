//! Where backups live.
//!
//! Six methods, all blocking and all keyed by a `/`-separated relative path. A
//! folder is the reference implementation: a local directory, a mounted drive,
//! a network share or a Syncthing/Dropbox folder that the user already syncs.
//! That is enough for real use and, unlike an HTTP backend, it is testable
//! without credentials.
//!
//! Keys are validated, never repaired: anything that could climb out of the
//! provider root (`..`, an absolute path, a Windows drive, a NUL) is refused
//! with [`SyncError::UnsafeKey`]. A provider that silently rewrote `../x` into
//! `x` would turn a manifest typo into a write outside the backup folder.

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Result, SyncError};

/// A remote that can store opaque files under relative keys.
///
/// Implementations are expected to be idempotent where it is cheap: `remove` of
/// a missing key is not an error, `get_file` of a missing key is.
pub trait SyncProvider {
    /// Short stable identifier, also what `settings.json` stores.
    fn id(&self) -> &'static str;
    /// Human-readable location for the UI. Never includes a credential.
    fn describe(&self) -> String;
    fn put_file(&self, key: &str, source: &Path) -> Result<()>;
    fn get_file(&self, key: &str, target: &Path) -> Result<()>;
    /// Whole file into memory; used for manifests only.
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<()>;
    /// Every key below `prefix`, `/`-separated and relative to the provider root.
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    fn remove(&self, key: &str) -> Result<()>;
}

/// Backups stored in a directory the user owns.
pub struct FolderProvider {
    root: PathBuf,
}

impl FolderProvider {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self { root: root.into() })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, key: &str) -> Result<PathBuf> {
        let key = safe_key(key)?;
        let path = self.root.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        // `safe_key` already rejected `..`, but the belt-and-braces check keeps
        // the invariant local: nothing outside the root can ever be returned.
        if !path.starts_with(&self.root) {
            return Err(SyncError::UnsafeKey(key));
        }
        Ok(path)
    }
}

impl SyncProvider for FolderProvider {
    fn id(&self) -> &'static str {
        "folder"
    }

    fn describe(&self) -> String {
        self.root.display().to_string()
    }

    fn put_file(&self, key: &str, source: &Path) -> Result<()> {
        let target = self.resolve(key)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SyncError::io(parent, e))?;
        }
        std::fs::copy(source, &target).map_err(|e| SyncError::io(&target, e))?;
        Ok(())
    }

    fn get_file(&self, key: &str, target: &Path) -> Result<()> {
        let source = self.resolve(key)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SyncError::io(parent, e))?;
        }
        std::fs::copy(&source, target).map_err(|e| SyncError::io(&source, e))?;
        Ok(())
    }

    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.resolve(key)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SyncError::io(&path, error)),
        }
    }

    fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.resolve(key)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SyncError::io(parent, e))?;
        }
        // Write-then-rename: a manifest read by another machine is either the
        // old file or the new one, never half of each.
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, bytes).map_err(|e| SyncError::io(&temp, e))?;
        std::fs::rename(&temp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            SyncError::io(&path, e)
        })?;
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let normalized = prefix.replace('\\', "/");
        let base = self.root.join(normalized.trim_end_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
        if !base.is_dir() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        let mut stack = vec![base.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    stack.push(entry.path());
                } else if kind.is_file() {
                    let path = entry.path();
                    let Ok(relative) = path.strip_prefix(&self.root) else {
                        continue;
                    };
                    out.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        Ok(out)
    }

    fn remove(&self, key: &str) -> Result<()> {
        let path = self.resolve(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SyncError::io(&path, error)),
        }
    }
}

/// Reject anything that could escape the provider root.
pub fn safe_key(key: &str) -> Result<String> {
    let normalized = key.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') {
        return Err(SyncError::UnsafeKey(key.to_string()));
    }
    if normalized
        .chars()
        .any(|c| c == '\0' || c.is_control() || c == ':')
    {
        return Err(SyncError::UnsafeKey(key.to_string()));
    }
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(SyncError::UnsafeKey(key.to_string()));
        }
    }
    Ok(normalized)
}

/// SHA-256 and byte length of one file, read in 64 KiB chunks.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let file = std::fs::File::open(path).map_err(|e| SyncError::io(path, e))?;
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(|e| SyncError::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((hex::encode(hasher.finalize()), total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_round_trip_under_nested_keys() {
        let dir = tempfile::tempdir().expect("temp");
        let provider = FolderProvider::new(dir.path().join("remote")).expect("provider");
        let source = dir.path().join("local.bin");
        std::fs::write(&source, b"payload").expect("write");

        provider
            .put_file("instances/a/snapshots/one.zip", &source)
            .expect("put");
        let keys = provider.list("instances/").expect("list");
        assert_eq!(keys, vec!["instances/a/snapshots/one.zip"]);

        let target = dir.path().join("back.bin");
        provider
            .get_file("instances/a/snapshots/one.zip", &target)
            .expect("get");
        assert_eq!(std::fs::read(&target).expect("read"), b"payload");

        provider
            .remove("instances/a/snapshots/one.zip")
            .expect("remove");
        assert!(provider.list("instances/").expect("list").is_empty());
        // Removing a key that is already gone stays quiet.
        provider
            .remove("instances/a/snapshots/one.zip")
            .expect("idempotent");
    }

    #[test]
    fn keys_never_escape_the_root() {
        for key in [
            "../outside",
            "a/../../outside",
            "/absolute",
            "C:/Windows",
            "a//b",
            "",
            "a/./b",
            "nul\0byte",
        ] {
            let error = safe_key(key).expect_err(key);
            assert!(matches!(error, SyncError::UnsafeKey(_)), "key: {key}");
        }
        assert_eq!(safe_key("instances/id/manifest.json").expect("ok"), "instances/id/manifest.json");
    }

    #[test]
    fn writes_are_atomic_and_overwrite() {
        let dir = tempfile::tempdir().expect("temp");
        let provider = FolderProvider::new(dir.path()).expect("provider");
        provider.write("a/manifest.json", b"first").expect("write");
        provider.write("a/manifest.json", b"second").expect("overwrite");
        let bytes = provider.read("a/manifest.json").expect("read").expect("present");
        assert_eq!(bytes, b"second");
        assert!(provider.read("a/missing.json").expect("read").is_none());
    }

    #[test]
    fn sha256_matches_the_known_digest() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").expect("write");
        let (hash, bytes) = sha256_file(&path).expect("hash");
        assert_eq!(bytes, 3);
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
