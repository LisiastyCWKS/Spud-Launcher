use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{read_json, write_json_atomic, CoreError, Result};
use crate::paths::LauncherPaths;

/// `version_manifest_v2.json` — the list shown in the "new instance" dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    /// Mojang calls it `type`; the UI reads `kind`. Serialising the raw name
    /// left the snapshot label rendering as `(undefined)`.
    #[serde(rename(serialize = "kind", deserialize = "type"))]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub time: String,
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

/// One Mojang/loader version JSON. Loader profiles use `inheritsFrom` to point
/// at the vanilla JSON, so the launcher resolves a chain before launching.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionJson {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(rename = "mainClass", default)]
    pub main_class: String,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: VersionDownloads,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub logging: Option<Logging>,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    #[serde(rename = "minimumLauncherVersion", default)]
    pub minimum_launcher_version: Option<u32>,
    #[serde(rename = "releaseTime", default)]
    pub release_time: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionDownloads {
    #[serde(default)]
    pub client: Option<Artifact>,
    #[serde(default)]
    pub server: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
    #[serde(rename = "component", default)]
    pub component: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(rename = "totalSize", default)]
    pub total_size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
    /// Pre-1.7 versions can require a materialized `virtual/legacy` tree.
    #[serde(rename = "virtual", default)]
    pub virtual_: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

impl AssetIndex {
    pub fn is_virtual(&self) -> bool {
        self.virtual_
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Artifact {
    /// Present on logging configs; absent on client/library artifacts.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: HashMap<String, Artifact>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: LibraryDownloads,
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub extract: Option<Extract>,
    /// Maven repository override for libraries without a `downloads` block.
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: Artifact,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    /// Mojang writes `disallow`; `deny` is accepted for JSONs produced by
    /// earlier Spud builds. Both mean the same thing.
    #[serde(alias = "disallow", alias = "deny")]
    Deny,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rule {
    pub action: Option<RuleAction>,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OsInfo {
    /// Mojang naming: `windows`, `linux`, `osx`.
    pub name: String,
    /// Mojang naming: `x86`, `x86_64`, `arm64`.
    pub arch: String,
    pub version: String,
}

impl OsInfo {
    pub fn current() -> Self {
        let name = match std::env::consts::OS {
            "macos" => "osx",
            other => other,
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "aarch64" => "arm64",
            other => other,
        };
        Self {
            name: name.to_string(),
            arch: arch.to_string(),
            version: String::new(),
        }
    }
}

impl OsRule {
    fn matches(&self, os: &OsInfo) -> bool {
        if let Some(name) = &self.name {
            if name != &os.name {
                return false;
            }
        }
        if let Some(arch) = &self.arch {
            if arch != &os.arch {
                return false;
            }
        }
        if let Some(version) = &self.version {
            // Mojang only ships simple anchored patterns like `^10\.`.
            let probe = version
                .trim_start_matches('^')
                .trim_end_matches('\\')
                .trim_end_matches('.');
            if !probe.is_empty() && !os.version.starts_with(probe) {
                return false;
            }
        }
        true
    }
}

/// Last matching rule wins, matching the official launcher's semantics.
pub fn rules_allow(rules: &[Rule], os: &OsInfo, features: &HashMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_ok = rule.os.as_ref().map(|r| r.matches(os)).unwrap_or(true);
        let features_ok = rule
            .features
            .as_ref()
            .map(|f| f.iter().all(|(k, v)| features.get(k).copied().unwrap_or(false) == *v))
            .unwrap_or(true);
        if os_ok && features_ok {
            allowed = rule.action.unwrap_or(RuleAction::Allow) == RuleAction::Allow;
        }
    }
    allowed
}

/// Evaluate `arguments.game` / `arguments.jvm` entries. An entry is either a
/// plain string or an object with `rules` and `value` (string or array).
pub fn resolve_arguments(
    entries: &[serde_json::Value],
    os: &OsInfo,
    features: &HashMap<String, bool>,
) -> Vec<String> {
    let mut out = Vec::new();
    for entry in entries {
        match entry {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Object(map) => {
                let rules: Vec<Rule> = map
                    .get("rules")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                if !rules_allow(&rules, os, features) {
                    continue;
                }
                match map.get("value") {
                    Some(serde_json::Value::String(s)) => out.push(s.clone()),
                    Some(serde_json::Value::Array(values)) => {
                        out.extend(values.iter().filter_map(|v| v.as_str().map(str::to_owned)));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoord {
    pub fn path(&self) -> String {
        let mut file = format!("{}-{}", self.artifact, self.version);
        if let Some(classifier) = &self.classifier {
            file.push('-');
            file.push_str(classifier);
        }
        file.push('.');
        file.push_str(&self.extension);
        format!(
            "{}/{}/{}/{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            file
        )
    }
}

/// `group:artifact:version[:classifier][@extension]`.
pub fn parse_maven(name: &str) -> Result<MavenCoord> {
    let (coords, extension) = match name.split_once('@') {
        Some((coords, ext)) if !ext.is_empty() => (coords, ext.to_string()),
        _ => (name, "jar".to_string()),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return Err(CoreError::Maven(name.to_string()));
    }
    let classifier = parts.get(3).filter(|s| !s.is_empty()).map(|s| s.to_string());
    Ok(MavenCoord {
        group: parts[0].to_string(),
        artifact: parts[1].to_string(),
        version: parts[2].to_string(),
        classifier,
        extension,
    })
}

/// A library file that must exist on disk before launch.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryFile {
    pub name: String,
    /// Relative to `<root>/libraries`.
    pub relative_path: PathBuf,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub is_native: bool,
}

fn artifact_for(lib: &Library, artifact: &Artifact) -> Result<LibraryFile> {
    let relative = match &artifact.path {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(parse_maven(&lib.name)?.path()),
    };
    Ok(LibraryFile {
        name: lib.name.clone(),
        relative_path: relative,
        url: artifact.url.clone(),
        sha1: artifact.sha1.clone(),
        size: artifact.size,
        is_native: false,
    })
}

/// Resolve a library against the current OS. Returns `None` when rules exclude
/// it (e.g. a Linux-only library on Windows).
pub fn resolve_library(
    lib: &Library,
    os: &OsInfo,
    features: &HashMap<String, bool>,
) -> Option<LibraryFile> {
    if !rules_allow(&lib.rules, os, features) {
        return None;
    }

    // Native jars live under classifiers, not the main artifact.
    if let Some(classifier_template) = lib.natives.get(&os.name) {
        let classifier = classifier_template
            .replace("${arch}", if os.arch == "x86" { "32" } else { "64" });
        if let Some(artifact) = lib.downloads.classifiers.get(&classifier) {
            let mut file = artifact_for(lib, artifact).ok()?;
            file.is_native = true;
            return Some(file);
        }
        // Legacy fallback: maven coordinate carries the classifier explicitly.
        if let Ok(mut coord) = parse_maven(&lib.name) {
            coord.classifier = Some(classifier);
            let path = coord.path();
            return Some(LibraryFile {
                name: lib.name.clone(),
                relative_path: PathBuf::from(&path),
                url: format!("https://libraries.minecraft.net/{path}"),
                sha1: None,
                size: None,
                is_native: true,
            });
        }
    }

    if let Some(artifact) = &lib.downloads.artifact {
        return artifact_for(lib, artifact).ok();
    }

    // Old JSONs have no downloads block at all.
    let coord = parse_maven(&lib.name).ok()?;
    let path = coord.path();
    let base = lib
        .url
        .clone()
        .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
    Some(LibraryFile {
        name: lib.name.clone(),
        relative_path: PathBuf::from(&path),
        url: format!("{}/{}", base.trim_end_matches('/'), path),
        sha1: None,
        size: None,
        is_native: false,
    })
}

pub fn resolve_libraries(
    libraries: &[Library],
    os: &OsInfo,
    features: &HashMap<String, bool>,
) -> Vec<LibraryFile> {
    libraries
        .iter()
        .filter_map(|lib| resolve_library(lib, os, features))
        .collect()
}

/// A fully resolved version: the vanilla JSON merged with every loader layer.
#[derive(Debug, Clone)]
pub struct MergedVersion {
    pub id: String,
    pub main_class: String,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: String,
    pub libraries: Vec<Library>,
    pub jvm_args: Vec<serde_json::Value>,
    pub game_args: Vec<serde_json::Value>,
    pub legacy_arguments: Option<String>,
    pub java_major: u32,
    pub logging: Option<LoggingClient>,
    pub client: Option<Artifact>,
    pub kind: String,
}

impl MergedVersion {
    pub fn libraries_for(&self, os: &OsInfo, features: &HashMap<String, bool>) -> Vec<LibraryFile> {
        resolve_libraries(&self.libraries, os, features)
    }

    pub fn game_arguments(&self, os: &OsInfo, features: &HashMap<String, bool>) -> Vec<String> {
        let mut result: Vec<String> = self
            .legacy_arguments
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        result.extend(resolve_arguments(&self.game_args, os, features));
        result
    }

    pub fn jvm_arguments(&self, os: &OsInfo, features: &HashMap<String, bool>) -> Vec<String> {
        resolve_arguments(&self.jvm_args, os, features)
    }
}

fn merge(parent: VersionJson, child: VersionJson) -> MergedVersion {
    let mut libraries = child.libraries.clone();
    libraries.extend(parent.libraries.clone());
    dedupe_libraries(&mut libraries);

    let java_major = child
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .or_else(|| parent.java_version.as_ref().map(|j| j.major_version))
        .unwrap_or_else(|| crate::java::guess_major(child.id.split('-').last().unwrap_or("1.21")));

    let mut jvm_args = parent
        .arguments
        .as_ref()
        .map(|a| a.jvm.clone())
        .unwrap_or_default();
    let mut game_args = parent
        .arguments
        .as_ref()
        .map(|a| a.game.clone())
        .unwrap_or_default();
    if let Some(arguments) = child.arguments.as_ref() {
        jvm_args.extend(arguments.jvm.clone());
        game_args.extend(arguments.game.clone());
    }

    MergedVersion {
        id: if child.id.is_empty() { parent.id } else { child.id },
        main_class: if child.main_class.is_empty() {
            parent.main_class
        } else {
            child.main_class
        },
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets).unwrap_or_else(|| "legacy".into()),
        libraries,
        jvm_args,
        game_args,
        legacy_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        java_major,
        logging: child.logging.or(parent.logging).and_then(|l| l.client),
        client: parent.downloads.client,
        kind: child
            .kind
            .or(parent.kind)
            .unwrap_or_else(|| "release".into()),
    }
}

fn dedupe_libraries(libraries: &mut Vec<Library>) {
    let mut seen = std::collections::HashSet::new();
    libraries.retain(|lib| seen.insert(lib.name.clone()));
}

/// Read a version JSON from the local meta directory.
pub fn load_version_json(paths: &LauncherPaths, version_id: &str) -> Result<VersionJson> {
    let path = paths.version_json(version_id);
    if !path.exists() {
        return Err(CoreError::InvalidVersion(format!(
            "version `{version_id}` is not installed"
        )));
    }
    read_json(&path)
}

/// Resolve `inheritsFrom` recursively (vanilla → loader). Depth is capped so a
/// corrupted or hostile JSON cannot loop forever.
pub fn load_merged(paths: &LauncherPaths, version_id: &str) -> Result<MergedVersion> {
    fn walk(paths: &LauncherPaths, id: &str, depth: usize) -> Result<VersionJson> {
        if depth > 8 {
            return Err(CoreError::InvalidVersion(format!(
                "inheritsFrom chain too deep at `{id}`"
            )));
        }
        let json = load_version_json(paths, id)?;
        match json.inherits_from.clone() {
            Some(parent_id) => {
                let parent = walk(paths, &parent_id, depth + 1)?;
                let merged = merge(parent, json);
                // Flatten to a VersionJson so nested loaders compose.
                Ok(VersionJson {
                    id: merged.id,
                    kind: Some(merged.kind),
                    main_class: merged.main_class,
                    asset_index: merged.asset_index,
                    assets: Some(merged.assets),
                    downloads: VersionDownloads {
                        client: merged.client,
                        server: None,
                    },
                    java_version: Some(JavaVersion {
                        major_version: merged.java_major,
                        component: String::new(),
                    }),
                    libraries: merged.libraries,
                    arguments: Some(Arguments {
                        game: merged.game_args,
                        jvm: merged.jvm_args,
                    }),
                    minecraft_arguments: merged.legacy_arguments,
                    logging: merged.logging.map(|client| Logging {
                        client: Some(client),
                    }),
                    inherits_from: None,
                    minimum_launcher_version: None,
                    release_time: None,
                })
            }
            None => Ok(json),
        }
    }

    let json = walk(paths, version_id, 0)?;
    // The walk returns a flattened JSON; convert it once more through `merge`
    // with an empty parent to get the launch-facing struct.
    let merged = merge(VersionJson::default(), json);
    if merged.main_class.is_empty() {
        return Err(CoreError::InvalidVersion(format!(
            "`{version_id}` has no mainClass"
        )));
    }
    Ok(merged)
}

/// Persist a downloaded/resolved version JSON.
pub fn save_version_json(paths: &LauncherPaths, json: &VersionJson) -> Result<()> {
    write_json_atomic(&paths.version_json(&json.id), json)
}

/// Convenience for tests and for installers that need the path.
pub fn version_path(paths: &LauncherPaths, id: &str) -> PathBuf {
    paths.version_json(id)
}

/// Ensure a path stays inside the libraries root (defends against a hostile
/// version JSON with `path: "../../evil"`).
pub fn library_destination(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            _ => {
                return Err(CoreError::InvalidVersion(format!(
                    "unsafe library path: {}",
                    relative.display()
                )))
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(name: &str) -> OsInfo {
        OsInfo {
            name: name.into(),
            arch: "x86_64".into(),
            version: String::new(),
        }
    }

    #[test]
    fn maven_paths_are_correct() {
        let coord = parse_maven("net.fabricmc:fabric-loader:0.16.5").unwrap();
        assert_eq!(
            coord.path(),
            "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar"
        );

        let with_classifier = parse_maven("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap();
        assert_eq!(
            with_classifier.path(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
    }

    #[test]
    fn version_summary_reads_type_but_emits_kind() {
        let raw = r#"{"id":"26.4-snapshot-6","type":"snapshot","url":"https://example/26.4.json"}"#;
        let summary: VersionSummary = serde_json::from_str(raw).expect("parse manifest entry");
        assert_eq!(summary.kind, "snapshot");
        assert_eq!(summary.url, "https://example/26.4.json");

        let out = serde_json::to_value(&summary).expect("serialise");
        assert_eq!(out["kind"], "snapshot");
        assert!(out.get("type").is_none(), "UI reads `kind` only");
    }

    #[test]
    fn rules_respect_the_last_match() {
        let rules = vec![
            Rule {
                action: Some(RuleAction::Allow),
                ..Default::default()
            },
            Rule {
                action: Some(RuleAction::Deny),
                os: Some(OsRule {
                    name: Some("osx".into()),
                    ..Default::default()
                }),
                features: None,
            },
        ];
        assert!(rules_allow(&rules, &os("windows"), &HashMap::new()));
        assert!(!rules_allow(&rules, &os("osx"), &HashMap::new()));
    }

    #[test]
    fn rules_accept_mojang_disallow_wording() {
        // Mojang's own version JSONs (e.g. the 1.0 alpha profile) write
        // "disallow"; earlier Spud builds only understood "deny".
        let json = r#"{"action":"disallow","os":{"name":"osx"}}"#;
        let rule: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.action, Some(RuleAction::Deny));

        let legacy: Rule = serde_json::from_str(r#"{"action":"deny"}"#).unwrap();
        assert_eq!(legacy.action, Some(RuleAction::Deny));

        let allow: Rule = serde_json::from_str(r#"{"action":"allow"}"#).unwrap();
        assert_eq!(allow.action, Some(RuleAction::Allow));

        assert!(!rules_allow(&[rule], &os("osx"), &HashMap::new()));
        assert!(rules_allow(&[allow], &os("windows"), &HashMap::new()));
    }

    #[test]
    fn conditional_arguments_are_filtered() {
        let entries = vec![
            serde_json::json!("--demo"),
            serde_json::json!({
                "rules": [{"action": "allow", "features": {"is_demo_user": true}}],
                "value": "--demo"
            }),
        ];
        let mut features = HashMap::new();
        features.insert("is_demo_user".to_string(), false);
        let resolved = resolve_arguments(&entries, &os("windows"), &features);
        assert_eq!(resolved, vec!["--demo"]);
    }

    #[test]
    fn java_requirement_comes_from_version_json() {
        let parent = VersionJson {
            id: "1.20.1".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            java_version: Some(JavaVersion {
                major_version: 17,
                component: "java-runtime-gamma".into(),
            }),
            ..Default::default()
        };
        let child = VersionJson {
            id: "fabric-loader-0.16.5-1.20.1".into(),
            main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".into(),
            inherits_from: Some("1.20.1".into()),
            ..Default::default()
        };
        let merged = merge(parent, child);
        assert_eq!(merged.java_major, 17);
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
    }
}
