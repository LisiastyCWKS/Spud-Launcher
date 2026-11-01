//! Detect instances that belong to other launchers and pull them in.
//!
//! Every launcher keeps its profiles in a slightly different folder with a
//! slightly different metadata file, so each one gets a small adapter: locate
//! the profile directory, read whatever JSON/CFG the launcher wrote, and map it
//! onto `InstanceConfig`. Nothing here touches the network — an import copies
//! files and records the version, then the normal install path downloads the
//! game itself.
//!
//! Scanning is read-only and tolerant: a launcher that is not installed is a
//! missing folder, a profile with a broken metadata file still shows up with
//! its folder-derived name, and the mods folder is never descended into for
//! files larger than the report needs.

use std::path::{Path, PathBuf};

use launcher_core::instance::{InstanceConfig, NewInstance};
use launcher_core::LoaderKind;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// Folders copied from a foreign profile. Worlds are included: "move my pack"
/// usually means "my pack and my builds".
const CONTENT_DIRS: &[&str] = &[
    "mods",
    "config",
    "defaultconfigs",
    "resourcepacks",
    "shaderpacks",
    "saves",
    "screenshots",
];

#[derive(Debug, Clone, Serialize)]
pub struct DetectedInstance {
    /// Human name of the launcher the profile came from.
    pub launcher: String,
    /// Stable key for the launcher (`prism`, `modrinth`, `lunar`, ...).
    pub launcher_key: String,
    pub name: String,
    /// Game directory that holds `mods/`, `config/`, ...
    pub path: String,
    pub mc_version: Option<String>,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub mods: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedLauncher {
    pub key: String,
    pub name: String,
    pub root: String,
    pub instances: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub launchers: Vec<DetectedLauncher>,
    pub instances: Vec<DetectedInstance>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportOutcome {
    pub instance: InstanceConfig,
    pub copied: usize,
    pub skipped: usize,
}

pub fn scan() -> ScanResult {
    let mut launchers = Vec::new();
    let mut instances = Vec::new();

    for candidate in candidates() {
        let root = candidate.root.clone();
        if !root.is_dir() {
            continue;
        }
        let found = match candidate.kind {
            Kind::Prism => scan_prism_dir(&candidate),
            Kind::Modrinth => scan_modrinth_dir(&candidate),
            Kind::CurseForge => scan_curseforge_dir(&candidate),
            Kind::AtLauncher => scan_atlauncher_dir(&candidate),
            Kind::GdLauncher => scan_gdlauncher_dir(&candidate),
            Kind::Technic => scan_folder_list(&candidate, "technic"),
            Kind::Lunar => scan_lunar_dir(&candidate),
            Kind::Vanilla => scan_vanilla(&candidate),
        };
        if found.is_empty() {
            continue;
        }
        launchers.push(DetectedLauncher {
            key: candidate.key.to_string(),
            name: candidate.name.to_string(),
            root: root.display().to_string(),
            instances: found.len(),
        });
        instances.extend(found);
    }

    instances.sort_by(|a, b| a.launcher.cmp(&b.launcher).then_with(|| a.name.cmp(&b.name)));
    ScanResult {
        launchers,
        instances,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Prism,
    Modrinth,
    CurseForge,
    AtLauncher,
    GdLauncher,
    Technic,
    Lunar,
    Vanilla,
}

struct Candidate {
    key: &'static str,
    name: &'static str,
    root: PathBuf,
    kind: Kind,
}

#[tauri::command]
pub fn launchers_scan() -> ScanResult {
    scan()
}

fn appdata() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

fn localappdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

/// Every launcher location worth checking, in the order the UI lists them.
fn candidates() -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut push = |key: &'static str, name: &'static str, root: Option<PathBuf>, kind: Kind| {
        if let Some(root) = root {
            out.push(Candidate {
                key,
                name,
                root,
                kind,
            });
        }
    };

    let roaming = appdata();
    let local = localappdata();
    let home_dir = home();

    push(
        "prism",
        "Prism Launcher",
        roaming
            .as_ref()
            .map(|base| base.join("PrismLauncher").join("instances")),
        Kind::Prism,
    );
    push(
        "prism",
        "Prism Launcher (portable)",
        home_dir
            .as_ref()
            .map(|base| base.join("PrismLauncher").join("instances")),
        Kind::Prism,
    );
    push(
        "multimc",
        "MultiMC",
        roaming
            .as_ref()
            .map(|base| base.join("MultiMC").join("instances")),
        Kind::Prism,
    );
    push(
        "polymc",
        "PolyMC",
        roaming
            .as_ref()
            .map(|base| base.join("PolyMC").join("instances")),
        Kind::Prism,
    );
    push(
        "modrinth",
        "Modrinth App",
        roaming
            .as_ref()
            .map(|base| base.join("com.modrinth.theseus").join("profiles")),
        Kind::Modrinth,
    );
    push(
        "modrinth",
        "Modrinth App",
        local
            .as_ref()
            .map(|base| base.join("com.modrinth.theseus").join("profiles")),
        Kind::Modrinth,
    );
    push(
        "modrinth",
        "Modrinth App",
        roaming
            .as_ref()
            .map(|base| base.join("ModrinthApp").join("profiles")),
        Kind::Modrinth,
    );
    push(
        "curseforge",
        "CurseForge",
        home_dir
            .as_ref()
            .map(|base| base.join("curseforge").join("minecraft").join("Instances")),
        Kind::CurseForge,
    );
    push(
        "curseforge",
        "CurseForge",
        home_dir.as_ref().map(|base| {
            base.join("Documents")
                .join("curseforge")
                .join("minecraft")
                .join("Instances")
        }),
        Kind::CurseForge,
    );
    push(
        "atlauncher",
        "ATLauncher",
        roaming
            .as_ref()
            .map(|base| base.join("ATLauncher").join("instances")),
        Kind::AtLauncher,
    );
    push(
        "gdlauncher",
        "GDLauncher",
        roaming
            .as_ref()
            .map(|base| base.join("gdlauncher_next").join("instances")),
        Kind::GdLauncher,
    );
    push(
        "technic",
        "Technic",
        roaming
            .as_ref()
            .map(|base| base.join(".technic").join("modpacks")),
        Kind::Technic,
    );
    push(
        "lunar",
        "Lunar Client",
        home_dir
            .as_ref()
            .map(|base| base.join(".lunarclient").join("profiles")),
        Kind::Lunar,
    );
    push(
        "vanilla",
        "Minecraft Launcher",
        roaming.as_ref().map(|base| base.join(".minecraft")),
        Kind::Vanilla,
    );

    out
}

fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&read_text(path)?).ok()
}


impl DetectedInstance {
    /// Fill in what the launcher did not write down.
    ///
    /// Modrinth App, Lunar and a bare `.minecraft` often keep the version only
    /// in the log line Minecraft prints on every start —
    /// `Loading Minecraft 1.20.1 with Fabric Loader 0.15.11` — so the profile
    /// folder itself has no metadata to read.
    fn fill_from_log(mut self) -> Self {
        if self.mc_version.is_some() && self.loader != LoaderKind::Vanilla {
            return self;
        }
        let game = Path::new(&self.path);
        if let Some((version, loader, loader_version)) = detect_from_log(game) {
            if self.mc_version.is_none() {
                self.mc_version = Some(version);
            }
            if self.loader == LoaderKind::Vanilla {
                self.loader = loader;
                self.loader_version = loader_version;
            }
        }
        if self.mc_version.is_none() {
            self.mc_version = version_from_folder(game);
        }
        self
    }
}

fn read_head(path: &Path, cap: u64) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buffer = vec![0u8; cap as usize];
    let read = file.read(&mut buffer).ok()?;
    buffer.truncate(read);
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

fn detect_from_log(game: &Path) -> Option<(String, LoaderKind, Option<String>)> {
    for name in ["logs/latest.log", "logs/game.log"] {
        let Some(text) = read_head(&game.join(name), 16 * 1024) else {
            continue;
        };
        for line in text.lines().take(60) {
            let Some(index) = line.find("Loading Minecraft ") else {
                continue;
            };
            let rest = &line[index + "Loading Minecraft ".len()..];
            let mut parts = rest.splitn(2, " with ");
            let version = parts.next().unwrap_or("").trim().to_string();
            if version.is_empty() {
                continue;
            }
            let (loader, loader_version) = match parts.next() {
                Some(tail) => {
                    let tail = tail.trim().trim_end_matches('.');
                    let lower = tail.to_ascii_lowercase();
                    let kind = if lower.starts_with("fabric") {
                        LoaderKind::Fabric
                    } else if lower.starts_with("quilt") {
                        LoaderKind::Quilt
                    } else if lower.starts_with("neoforge") {
                        LoaderKind::NeoForge
                    } else if lower.starts_with("forge") {
                        LoaderKind::Forge
                    } else {
                        LoaderKind::Vanilla
                    };
                    let number = tail
                        .split_whitespace()
                        .filter(|word| word.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
                        .last()
                        .map(str::to_string);
                    (kind, number)
                }
                None => (LoaderKind::Vanilla, None),
            };
            return Some((version, loader, loader_version));
        }
    }
    None
}

/// `<game>/versions/<id>` is the vanilla launcher's own record of what ran.
fn version_from_folder(game: &Path) -> Option<String> {
    let entries = std::fs::read_dir(game.join("versions")).ok()?;
    let mut best: Option<String> = None;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            best = Some(match best {
                Some(current) if current > name => current,
                _ => name,
            });
        }
    }
    best
}

fn dir_size(root: &Path, limit: u64) -> u64 {
    let mut total = 0u64;
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
                total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            }
            if total > limit {
                return total;
            }
        }
    }
    total
}

fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter(|e| e.path().is_file()).count())
        .unwrap_or(0)
}

fn find_game_dir(profile: &Path) -> PathBuf {
    let nested = profile.join(".minecraft");
    if nested.is_dir() {
        nested
    } else {
        profile.to_path_buf()
    }
}

#[derive(Debug, Default, Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(Debug, Default, Deserialize)]
struct MmcComponent {
    #[serde(default, rename = "uid")]
    uid: String,
    #[serde(default, rename = "version")]
    version: String,
}

fn scan_prism_dir(candidate: &Candidate) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let path = find_game_dir(&profile);
        if !path.is_dir() {
            continue;
        }

        let mut name = entry.file_name().to_string_lossy().to_string();
        if let Some(cfg) = read_text(&profile.join("instance.cfg")) {
            for line in cfg.lines() {
                if let Some(value) = line.strip_prefix("name=") {
                    let value = value.trim();
                    if !value.is_empty() {
                        name = value.to_string();
                    }
                }
            }
        }

        let mut mc_version = None;
        let mut loader = LoaderKind::Vanilla;
        let mut loader_version = None;
        if let Some(pack) = read_json::<MmcPack>(&profile.join("mmc-pack.json")) {
            for component in pack.components {
                match component.uid.as_str() {
                    "net.minecraft" => mc_version = Some(component.version),
                    "net.fabricmc.fabric-loader" => {
                        loader = LoaderKind::Fabric;
                        loader_version = Some(component.version);
                    }
                    "org.quiltmc.quilt-loader" => {
                        loader = LoaderKind::Quilt;
                        loader_version = Some(component.version);
                    }
                    "net.minecraftforge" => {
                        loader = LoaderKind::Forge;
                        loader_version = Some(component.version);
                    }
                    "net.neoforged" => {
                        loader = LoaderKind::NeoForge;
                        loader_version = Some(component.version);
                    }
                    _ => {}
                }
            }
        }

        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name,
            path: path.display().to_string(),
            mc_version,
            loader,
            loader_version,
            mods: count_files(&path.join("mods")),
            bytes: dir_size(&path, 8 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

#[derive(Debug, Default, Deserialize)]
struct ModrinthProfile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    game_version: String,
    #[serde(default)]
    loader: String,
    #[serde(default)]
    loader_version: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

fn scan_modrinth_dir(candidate: &Candidate) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let meta = read_json::<ModrinthProfile>(&profile.join("profile.json")).unwrap_or_default();
        let path = meta
            .path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| find_game_dir(&profile));
        if !path.is_dir() {
            continue;
        }
        let loader = match meta.loader.to_ascii_lowercase().as_str() {
            "fabric" => LoaderKind::Fabric,
            "quilt" => LoaderKind::Quilt,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            _ => LoaderKind::Vanilla,
        };
        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name: if meta.name.is_empty() {
                entry.file_name().to_string_lossy().to_string()
            } else {
                meta.name
            },
            path: path.display().to_string(),
            mc_version: Some(meta.game_version).filter(|value| !value.is_empty()),
            loader,
            loader_version: meta.loader_version,
            mods: count_files(&path.join("mods")),
            bytes: dir_size(&path, 8 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

#[derive(Debug, Default, Deserialize)]
struct CurseInstance {
    #[serde(default)]
    name: String,
    #[serde(default)]
    game_version: String,
    #[serde(default, rename = "baseModLoader")]
    base_mod_loader: Option<CurseLoader>,
}

#[derive(Debug, Default, Deserialize)]
struct CurseLoader {
    #[serde(default)]
    name: String,
    #[serde(default)]
    forge_version: Option<String>,
    #[serde(default)]
    maven_version_string: Option<String>,
}

fn scan_curseforge_dir(candidate: &Candidate) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let meta =
            read_json::<CurseInstance>(&profile.join("minecraftinstance.json")).unwrap_or_default();
        let path = find_game_dir(&profile);

        let lower = meta
            .base_mod_loader
            .as_ref()
            .map(|loader| loader.name.to_ascii_lowercase())
            .unwrap_or_default();
        let (loader, loader_version) = if lower.contains("neoforge") {
            (LoaderKind::NeoForge, meta.base_mod_loader.as_ref().and_then(version_of))
        } else if lower.contains("forge") {
            (LoaderKind::Forge, meta.base_mod_loader.as_ref().and_then(version_of))
        } else if lower.contains("fabric") {
            (LoaderKind::Fabric, meta.base_mod_loader.as_ref().and_then(version_of))
        } else if lower.contains("quilt") {
            (LoaderKind::Quilt, meta.base_mod_loader.as_ref().and_then(version_of))
        } else {
            (LoaderKind::Vanilla, None)
        };

        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name: if meta.name.is_empty() {
                entry.file_name().to_string_lossy().to_string()
            } else {
                meta.name
            },
            path: path.display().to_string(),
            mc_version: Some(meta.game_version).filter(|value| !value.is_empty()),
            loader,
            loader_version,
            mods: count_files(&path.join("mods")),
            bytes: dir_size(&path, 8 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

/// CurseForge writes the full maven coordinate (`forge-47.2.0`); only the
/// trailing version is useful here.
fn version_of(loader: &CurseLoader) -> Option<String> {
    loader
        .forge_version
        .clone()
        .or_else(|| loader.maven_version_string.clone())
        .map(|value| {
            value
                .rsplit(['-', ':'])
                .next()
                .unwrap_or(&value)
                .to_string()
        })
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Default, Deserialize)]
struct AtInstance {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "minecraftVersion")]
    minecraft_version: String,
    #[serde(default)]
    loader: Option<AtLoader>,
}

#[derive(Debug, Default, Deserialize)]
struct AtLoader {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    version: String,
}

fn scan_atlauncher_dir(candidate: &Candidate) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let game = find_game_dir(&profile);
        let meta = read_json::<AtInstance>(&profile.join("instance.json")).unwrap_or_default();
        let kind = meta
            .loader
            .as_ref()
            .map(|loader| loader.kind.to_ascii_lowercase())
            .unwrap_or_default();
        let loader = match kind.as_str() {
            value if value.contains("fabric") => LoaderKind::Fabric,
            value if value.contains("quilt") => LoaderKind::Quilt,
            value if value.contains("neoforge") => LoaderKind::NeoForge,
            value if value.contains("forge") => LoaderKind::Forge,
            _ => LoaderKind::Vanilla,
        };

        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name: if meta.name.is_empty() {
                entry.file_name().to_string_lossy().to_string()
            } else {
                meta.name
            },
            path: game.display().to_string(),
            mc_version: Some(meta.minecraft_version).filter(|value| !value.is_empty()),
            loader,
            loader_version: meta.loader.and_then(|loader| {
                Some(loader.version).filter(|value| !value.is_empty())
            }),
            mods: count_files(&game.join("mods")),
            bytes: dir_size(&game, 8 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

#[derive(Debug, Default, Deserialize)]
struct GdConfig {
    #[serde(default)]
    name: String,
    #[serde(default)]
    loader: GdLoader,
    #[serde(default)]
    game_version: String,
}

#[derive(Debug, Default, Deserialize)]
struct GdLoader {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    version: String,
}

fn scan_gdlauncher_dir(candidate: &Candidate) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let meta = read_json::<GdConfig>(&profile.join("config.json")).unwrap_or_default();
        let kind = meta.loader.kind.to_ascii_lowercase();
        let loader = match kind.as_str() {
            value if value.contains("fabric") => LoaderKind::Fabric,
            value if value.contains("quilt") => LoaderKind::Quilt,
            value if value.contains("neoforge") => LoaderKind::NeoForge,
            value if value.contains("forge") => LoaderKind::Forge,
            _ => LoaderKind::Vanilla,
        };
        let game = find_game_dir(&profile);
        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name: if meta.name.is_empty() {
                entry.file_name().to_string_lossy().to_string()
            } else {
                meta.name
            },
            path: game.display().to_string(),
            mc_version: Some(meta.game_version).filter(|value| !value.is_empty()),
            loader,
            loader_version: Some(meta.loader.version).filter(|value| !value.is_empty()),
            mods: count_files(&game.join("mods")),
            bytes: dir_size(&game, 8 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

fn scan_folder_list(candidate: &Candidate, _kind: &str) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let game = find_game_dir(&profile);
        if !game.is_dir() {
            continue;
        }
        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name: entry.file_name().to_string_lossy().to_string(),
            path: game.display().to_string(),
            mc_version: None,
            loader: LoaderKind::Vanilla,
            loader_version: None,
            mods: count_files(&game.join("mods")),
            bytes: dir_size(&game, 8 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

/// Lunar keeps one directory per supported game version under
/// `.lunarclient/profiles` (older builds used `offline/multiver`).
fn scan_lunar_dir(candidate: &Candidate) -> Vec<DetectedInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&candidate.root) else {
        return out;
    };
    for entry in entries.flatten() {
        let profile = entry.path();
        if !profile.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        out.push(DetectedInstance {
            launcher: candidate.name.to_string(),
            launcher_key: candidate.key.to_string(),
            name: format!("Lunar {name}"),
            path: profile.display().to_string(),
            mc_version: Some(name.clone()).filter(|value| {
                value
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            }),
            loader: LoaderKind::Vanilla,
            loader_version: None,
            mods: count_files(&profile.join("mods")),
            bytes: dir_size(&profile, 4 * 1024 * 1024 * 1024),
        }
        .fill_from_log());
    }
    out
}

/// A stock `.minecraft` folder is one instance, not a collection.
fn scan_vanilla(candidate: &Candidate) -> Vec<DetectedInstance> {
    vec![DetectedInstance {
        launcher: candidate.name.to_string(),
        launcher_key: candidate.key.to_string(),
        name: "Default .minecraft".to_string(),
        path: candidate.root.display().to_string(),
        mc_version: None,
        loader: LoaderKind::Vanilla,
        loader_version: None,
        mods: count_files(&candidate.root.join("mods")),
        bytes: dir_size(&candidate.root.join("mods"), 4 * 1024 * 1024 * 1024),
    }
    .fill_from_log()]
}

#[tauri::command]
pub async fn launcher_import(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
    mc_version: Option<String>,
    loader: Option<String>,
    loader_version: Option<String>,
) -> Result<ImportOutcome, String> {
    let source = PathBuf::from(path.trim());
    if !source.is_dir() {
        return Err(format!("{} is not a folder", source.display()));
    }

    let display_name = name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            source
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "Imported profile".to_string());

    let loader = match loader.as_deref().unwrap_or("vanilla") {
        "fabric" => LoaderKind::Fabric,
        "quilt" => LoaderKind::Quilt,
        "forge" => LoaderKind::Forge,
        "neoforge" => LoaderKind::NeoForge,
        _ => LoaderKind::Vanilla,
    };

    let version = mc_version
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "this profile has no Minecraft version recorded — pick one before importing".to_string()
        })?;

    let config = InstanceConfig::create(
        &state.paths,
        NewInstance {
            name: display_name,
            mc_version: version,
            loader,
            loader_version: loader_version.filter(|value| !value.is_empty()),
            max_memory_mb: None,
        },
    )
    .map_err(|e| e.to_string())?;

    let destination = state.paths.game_dir(&config.id);
    let mut copied = 0usize;
    let mut skipped = 0usize;

    for folder in CONTENT_DIRS {
        let from = source.join(folder);
        if !from.is_dir() {
            continue;
        }
        for file in walk(&from)? {
            let Ok(relative) = file.strip_prefix(&source) else {
                skipped += 1;
                continue;
            };
            let to = destination.join(relative);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("cannot create: {e}"))?;
            }
            match std::fs::copy(&file, &to) {
                Ok(_) => copied += 1,
                Err(_) => skipped += 1,
            }
        }
    }

    // `options.txt` and `servers.dat` are the two files that make a profile feel
    // like home; copy them when present.
    for file in ["options.txt", "servers.dat", "servers.dat_old"] {
        let from = source.join(file);
        if from.is_file() {
            let to = destination.join(file);
            if std::fs::copy(&from, &to).is_ok() {
                copied += 1;
            } else {
                skipped += 1;
            }
        }
    }

    let mut config = config;
    config.notes = format!("Imported from an external profile at {}", source.display());
    config
        .save_touched(&state.paths)
        .map_err(|e| e.to_string())?;

    Ok(ImportOutcome {
        instance: config,
        copied,
        skipped,
    })
}

/// Every regular file below `root`; symlinks are ignored so an import cannot
/// pull in a path outside the profile.
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
    fn forge_version_is_extracted_from_a_maven_coordinate() {
        let loader = CurseLoader {
            name: "forge".into(),
            forge_version: None,
            maven_version_string: Some("net.minecraftforge:forge:1.20.1-47.2.0".into()),
        };
        assert_eq!(version_of(&loader).as_deref(), Some("47.2.0"));
    }

    #[test]
    fn walk_skips_nothing_regular() {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(dir.path().join("mods")).expect("mkdir");
        std::fs::write(dir.path().join("mods/a.jar"), b"x").expect("write");
        assert_eq!(walk(dir.path()).expect("walk").len(), 1);
    }
}
