use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::instance::{JavaMode, JavaSettings};
use crate::paths::LauncherPaths;

/// A JVM discovered on the machine, fully described so the UI can show it
/// without running `java -version` again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub path: PathBuf,
    /// Feature release: 8, 17, 21, 25...
    pub major: u32,
    /// Full version string as reported by the JVM (`17.0.11+9`).
    pub version: String,
    pub vendor: String,
    /// `x86_64`, `aarch64`, ...
    pub arch: String,
}

impl JavaRuntime {
    pub fn label(&self) -> String {
        let vendor = if self.vendor.is_empty() {
            "Java"
        } else {
            &self.vendor
        };
        format!("Java {} ({vendor}) — {}", self.major, self.version)
    }
}

/// Parse `java -XshowSettings:properties -version` output. The JVM prints
/// properties to stderr; newer builds also print a version banner there.
pub fn parse_properties(stderr: &str) -> Option<(u32, String, String, String)> {
    let mut version = None;
    let mut vendor = String::new();
    let mut arch = String::new();

    for line in stderr.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "java.version" => version = Some(value.to_string()),
            "java.vendor" => vendor = value.to_string(),
            "os.arch" => arch = value.to_string(),
            _ => {}
        }
    }

    let version = version?;
    let major = parse_major(&version)?;
    Some((major, version, vendor, arch))
}

/// `1.8.0_402` -> 8, `17.0.11` -> 17, `21-ea` -> 21.
pub fn parse_major(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '-', '+', '_']);
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Inspect one JVM. Returns `None` when the path is not an executable JVM.
pub async fn inspect(path: &Path) -> Option<JavaRuntime> {
    if !path.is_file() {
        return None;
    }
    let output = Command::new(path)
        .args(["-XshowSettings:properties", "-version"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let (major, version, vendor, arch) = parse_properties(&stderr)?;
    Some(JavaRuntime {
        path: path.to_path_buf(),
        major,
        version,
        vendor,
        arch,
    })
}

/// Discover JVMs from PATH, JAVA_HOME, launcher-managed runtimes and the usual
/// vendor install locations. Sorted newest first; duplicates removed by
/// canonical path.
pub async fn discover(paths: &LauncherPaths) -> Vec<JavaRuntime> {
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();

    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.insert(PathBuf::from(java_home).join(java_in_bin()));
    }
    if let Some(on_path) = java_on_path() {
        candidates.insert(on_path);
    }
    collect_binaries(&paths.root().join("java"), 3, &mut candidates);

    for dir in search_roots() {
        collect_binaries(&dir, 4, &mut candidates);
    }

    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        // `java` from PATH is relative; canonicalize once, then run that.
        let resolved = std::fs::canonicalize(&candidate).unwrap_or(candidate);
        if !seen.insert(resolved.clone()) {
            continue;
        }
        if let Some(runtime) = inspect(&resolved).await {
            found.push(runtime);
        }
    }

    found.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| b.version.cmp(&a.version)));
    found
}

/// Pick a runtime for the required Java feature release.
///
/// Preference order: exact major match, then the smallest newer runtime (Java
/// is backwards compatible), never an older one. A manual path always wins if
/// it satisfies the requirement.
pub fn select<'a>(
    runtimes: &'a [JavaRuntime],
    required: u32,
    settings: &JavaSettings,
) -> Option<&'a JavaRuntime> {
    if settings.mode == JavaMode::Manual {
        if let Some(path) = settings.path.as_ref() {
            if let Some(found) = runtimes.iter().find(|r| same_file(&r.path, path)) {
                return (found.major >= required).then_some(found);
            }
        }
    }

    let mut newer: Option<&JavaRuntime> = None;
    for runtime in runtimes.iter().filter(|r| r.major >= required) {
        if runtime.major == required {
            return Some(runtime);
        }
        newer = match newer {
            Some(current) if current.major <= runtime.major => Some(current),
            _ => Some(runtime),
        };
    }
    newer
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn java_exe() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

fn java_in_bin() -> &'static str {
    if cfg!(windows) {
        "bin/java.exe"
    } else {
        "bin/java"
    }
}

fn java_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(java_exe()))
        .find(|candidate| candidate.is_file())
}

fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut push = |p: PathBuf| {
        if p.exists() {
            roots.push(p);
        }
    };

    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
        push(home.join(".jdks"));
        push(home.join(".local/share/PrismLauncher/java"));
        push(home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/java"));
        push(home.join("Library/Java/JavaVirtualMachines"));
    }

    #[cfg(windows)]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let base = PathBuf::from(program_files);
            for vendor in [
                "Java",
                "Eclipse Adoptium",
                "Eclipse Foundation",
                "Microsoft",
                "Zulu",
                "BellSoft",
                "Amazon Corretto",
                "Semeru",
            ] {
                push(base.join(vendor));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            push(PathBuf::from(local).join("Programs/Eclipse Adoptium"));
        }
        if let Some(roaming) = std::env::var_os("APPDATA") {
            push(PathBuf::from(&roaming).join("PrismLauncher/java"));
            push(PathBuf::from(roaming).join(".minecraft/runtime"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        push(PathBuf::from("/usr/lib/jvm"));
        push(PathBuf::from("/usr/java"));
    }

    #[cfg(target_os = "macos")]
    {
        push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    }

    roots
}

/// Search for `bin/java[.exe]` up to `depth` levels below `root`.
fn collect_binaries(root: &Path, depth: usize, out: &mut BTreeSet<PathBuf>) {
    if depth == 0 || !root.is_dir() {
        return;
    }
    let direct = root.join(java_in_bin());
    if direct.is_file() {
        out.insert(direct);
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_binaries(&path, depth - 1, out);
        }
    }
}

/// Mojang version JSONs declare the required Java; older ones do not.
/// This is the fallback table, correct for release versions.
pub fn guess_major(mc_version: &str) -> u32 {
    let mut parts = mc_version.split('.').filter_map(|p| p.parse::<u32>().ok());
    let first = parts.next().unwrap_or(1);
    let second = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);

    if first != 1 {
        return 21;
    }
    match (second, patch) {
        (20, p) if p >= 5 => 21,
        (21..=u32::MAX, _) => 21,
        (18..=20, _) => 17,
        (17, _) => 16,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_and_modern_versions() {
        assert_eq!(parse_major("1.8.0_402"), Some(8));
        assert_eq!(parse_major("17.0.11+9"), Some(17));
        assert_eq!(parse_major("21-ea"), Some(21));
    }

    #[test]
    fn parses_jvm_properties() {
        let sample = r#"
Property settings:
    java.home = C:\Program Files\Java\jdk-17
    java.vendor = Eclipse Adoptium
    java.version = 17.0.11
    os.arch = amd64
"#;
        let (major, version, vendor, arch) = parse_properties(sample).unwrap();
        assert_eq!(major, 17);
        assert_eq!(version, "17.0.11");
        assert_eq!(vendor, "Eclipse Adoptium");
        assert_eq!(arch, "amd64");
    }

    #[test]
    fn guesses_java_requirements() {
        assert_eq!(guess_major("1.20.1"), 17);
        assert_eq!(guess_major("1.20.6"), 21);
        assert_eq!(guess_major("1.16.5"), 8);
        assert_eq!(guess_major("1.21.4"), 21);
    }

    #[test]
    fn selects_smallest_satisfying_runtime() {
        let j8 = JavaRuntime {
            path: "j8".into(),
            major: 8,
            version: "1.8".into(),
            vendor: String::new(),
            arch: "x86_64".into(),
        };
        let j17 = JavaRuntime {
            path: "j17".into(),
            major: 17,
            version: "17".into(),
            vendor: String::new(),
            arch: "x86_64".into(),
        };
        let j21 = JavaRuntime {
            path: "j21".into(),
            major: 21,
            version: "21".into(),
            vendor: String::new(),
            arch: "x86_64".into(),
        };
        let settings = JavaSettings::default();
        let runtimes = vec![j21, j17, j8];
        assert_eq!(select(&runtimes, 17, &settings).unwrap().major, 17);
        assert_eq!(select(&runtimes, 16, &settings).unwrap().major, 17);
    }
}
