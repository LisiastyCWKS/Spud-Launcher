use std::path::{Path, PathBuf};

use crate::{NetError, Result};

/// Content-addressed cache used for files shared by many instances
/// (Modrinth jars, loader installers, exported packs).
///
/// Layout: `<root>/objects/<sha1>/<file name>`.
///
/// The cache never mutates a stored object. Materialising into an instance
/// uses a hard link where the filesystem supports it, falling back to a copy.
#[derive(Debug, Clone)]
pub struct ObjectCache {
    root: PathBuf,
}

impl ObjectCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn object_path(&self, sha1: &str, file_name: &str) -> PathBuf {
        self.root.join("objects").join(sha1).join(safe_file_name(file_name))
    }

    pub fn contains(&self, sha1: &str, file_name: &str) -> bool {
        self.object_path(sha1, file_name).is_file()
    }

    /// Move a freshly verified download into the cache. Re-downloading the same
    /// hash is a no-op.
    pub fn store(&self, source: &Path, sha1: &str, file_name: &str) -> Result<PathBuf> {
        let dest = self.object_path(sha1, file_name);
        if dest.exists() {
            return Ok(dest);
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|source| NetError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::rename(source, &dest).map_err(|source| NetError::Io {
            path: dest.clone(),
            source,
        })?;
        Ok(dest)
    }

    /// Hard-link (or copy) a cached object to its final in-instance location.
    /// Existing destinations are left untouched.
    pub fn materialise(&self, sha1: &str, file_name: &str, dest: &Path) -> Result<bool> {
        let object = self.object_path(sha1, file_name);
        if !object.is_file() {
            return Ok(false);
        }
        if dest.exists() {
            return Ok(true);
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|source| NetError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        if std::fs::hard_link(&object, dest).is_err() {
            std::fs::copy(&object, dest).map_err(|source| NetError::Io {
                path: dest.to_path_buf(),
                source,
            })?;
        }
        Ok(true)
    }
}

/// Strip any directory components from a file name supplied by an API.
pub fn safe_file_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != "." && *n != "..")
        .unwrap_or("download.bin")
        .to_string()
}
