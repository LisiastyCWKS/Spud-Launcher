//! Background images for the launcher UI.
//!
//! The user drops pictures into `<root>/backgrounds/`; the launcher lists them,
//! validates the name and hands the webview a `data:` URL. Reading bytes here
//! (rather than exposing Tauri's asset protocol) keeps the allowed directory
//! exactly one folder: the launcher has no business serving arbitrary paths, and
//! a `file://`-style scope would have to be as wide as the data root.
//!
//! Everything in this module is pure filesystem work, so it is unit-tested
//! without a window.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Extensions the webview renders reliably. SVG is deliberately absent: it is a
/// document that can carry script and external references, and a background is
/// never worth that surface.
pub const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "avif", "bmp"];

/// Refuse anything bigger than this instead of pushing a 90 MB wall of base64
/// through the IPC bridge. 24 MiB covers a 4K JPEG with room to spare.
pub const MAX_BACKGROUND_BYTES: u64 = 24 * 1024 * 1024;

/// How many files the picker will list. A folder with more is a folder someone
/// dumped a wallpaper pack into; the first 200 sorted by name are enough.
pub const MAX_BACKGROUNDS: usize = 200;

/// One usable image in the backgrounds folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackgroundFile {
    /// File name only. Never a path — `read_*` re-validates it.
    pub name: String,
    /// MIME type derived from the extension, used to build the data URL.
    pub mime: String,
    pub bytes: u64,
}

/// Lowercase extension of `name`, or `None` when it is missing or not an image.
pub fn image_extension(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    let lowered = extension.to_ascii_lowercase();
    IMAGE_EXTENSIONS
        .iter()
        .copied()
        .find(|allowed| *allowed == lowered)
}

/// `png` → `image/png`. Every extension in [`IMAGE_EXTENSIONS`] maps to a
/// standard type; the fallback never runs for a validated name.
pub fn mime_for(name: &str) -> &'static str {
    match image_extension(name) {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Is this a bare file name the launcher may open?
///
/// Rejects separators, drive letters, `.`/`..`, control characters, leading
/// dots (hidden files, and `.` / `..` again) and anything that is not a
/// supported image. This is the only gate between a settings string and
/// `File::open`, so it is strict on purpose.
pub fn is_valid_background_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 200 {
        return false;
    }
    if name.starts_with('.') || name.contains("..") {
        return false;
    }
    if name.chars().any(|ch| {
        ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    }) {
        return false;
    }
    image_extension(name).is_some()
}

fn entry(dir: &Path, name: String, bytes: u64) -> Option<BackgroundFile> {
    let path = dir.join(&name);
    if !is_valid_background_name(&name) || !path.is_file() || bytes == 0 {
        return None;
    }
    Some(BackgroundFile {
        mime: mime_for(&name).to_string(),
        name,
        bytes,
    })
}

/// Every image in `dir`, sorted by name. A missing folder is an empty list —
/// "no wallpapers yet" is not an error the UI should have to catch.
pub fn list_backgrounds(dir: &Path) -> Vec<BackgroundFile> {
    let reader = match std::fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(_) => return Vec::new(),
    };

    let mut found: Vec<BackgroundFile> = reader
        .filter_map(|item| item.ok())
        .filter_map(|item| {
            let metadata = item.metadata().ok()?;
            if !metadata.is_file() || metadata.len() > MAX_BACKGROUND_BYTES {
                return None;
            }
            let name = item.file_name().to_str()?.to_string();
            entry(dir, name, metadata.len())
        })
        .collect();

    found.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    found.truncate(MAX_BACKGROUNDS);
    found
}

pub fn background_path(dir: &Path, name: &str) -> Option<PathBuf> {
    if !is_valid_background_name(name) {
        return None;
    }
    let path = dir.join(name);
    path.is_file().then_some(path)
}

/// Bytes of one background, or `None` when the name is refused or the file is
/// gone. Size is re-checked here because the picker list can be stale.
pub fn read_background(dir: &Path, name: &str) -> Option<Vec<u8>> {
    let path = background_path(dir, name)?;
    let metadata = std::fs::metadata(&path).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_BACKGROUND_BYTES {
        return None;
    }
    std::fs::read(&path).ok()
}

/// `data:` URL ready for a CSS `background-image`.
pub fn background_data_url(dir: &Path, name: &str) -> Option<String> {
    use base64::Engine as _;
    let bytes = read_background(dir, name)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{};base64,{}", mime_for(name), encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn bare_image_names_are_accepted() {
        assert!(is_valid_background_name("mountains.jpg"));
        assert!(is_valid_background_name("Spud Wall 01.PNG"));
        assert!(is_valid_background_name("a.webp"));
    }

    #[test]
    fn paths_and_non_images_are_refused() {
        for name in [
            "",
            "..",
            "../secret.png",
            "sub/dir.png",
            "sub\\dir.png",
            "C:evil.png",
            ".hidden.png",
            "notes.txt",
            "vector.svg",
            "trailing.png.",
            "tab\there.png",
        ] {
            assert!(
                !is_valid_background_name(name),
                "{name:?} should be refused"
            );
        }
    }

    #[test]
    fn mime_is_derived_from_the_extension() {
        assert_eq!(mime_for("a.png"), "image/png");
        assert_eq!(mime_for("a.JPG"), "image/jpeg");
        assert_eq!(mime_for("a.jpeg"), "image/jpeg");
        assert_eq!(mime_for("a.avif"), "image/avif");
        assert_eq!(mime_for("a.txt"), "application/octet-stream");
    }

    #[test]
    fn listing_skips_foreign_files_and_sorts() {
        let dir = tempdir();
        std::fs::write(dir.path().join("b.png"), b"bbb").unwrap();
        std::fs::write(dir.path().join("a.jpg"), b"aa").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"nope").unwrap();
        std::fs::create_dir(dir.path().join("folder.png")).unwrap();

        let found = list_backgrounds(dir.path());
        let names: Vec<&str> = found.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, ["a.jpg", "b.png"]);
        assert_eq!(found[0].mime, "image/jpeg");
        assert_eq!(found[1].bytes, 3);
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let dir = tempdir();
        assert!(list_backgrounds(&dir.path().join("nope")).is_empty());
    }

    #[test]
    fn reading_refuses_traversal_even_when_the_target_exists() {
        let dir = tempdir();
        std::fs::write(dir.path().join("real.png"), b"x").unwrap();
        assert!(read_background(dir.path(), "real.png").is_some());
        assert!(read_background(dir.path(), "../real.png").is_none());
        assert!(read_background(dir.path(), "missing.png").is_none());
    }

    #[test]
    fn the_data_url_carries_the_mime_and_the_bytes() {
        let dir = tempdir();
        std::fs::write(dir.path().join("dot.png"), b"ab").unwrap();
        let url = background_data_url(dir.path(), "dot.png").unwrap();
        assert_eq!(url, "data:image/png;base64,YWI=");
        assert!(background_data_url(dir.path(), "nope.png").is_none());
    }
}
