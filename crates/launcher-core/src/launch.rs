use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, IoContext, Result};
use crate::instance::InstanceConfig;
use crate::java::JavaRuntime;
use crate::paths::LauncherPaths;
use crate::version::{
    library_destination, load_merged, AssetIndex, LibraryFile, MergedVersion, OsInfo,
};

/// The authenticated identity handed to the game. Tokens are never logged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    /// Dasshed UUID from the Minecraft profile.
    pub uuid: String,
    pub access_token: String,
    #[serde(default)]
    pub xuid: Option<String>,
    #[serde(default)]
    pub user_type: String,
}

impl Account {
    pub fn uuid_undashed(&self) -> String {
        self.uuid.replace('-', "")
    }
}

/// Everything needed to spawn the JVM. Building it is pure: no process is
/// started here, which keeps it testable.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub log_file: PathBuf,
    pub java_major: u32,
}

pub struct LaunchRequest<'a> {
    pub paths: &'a LauncherPaths,
    pub instance: &'a InstanceConfig,
    pub account: &'a Account,
    pub java: &'a JavaRuntime,
    pub version: &'a MergedVersion,
    pub version_id: &'a str,
}

/// Build the full command line for one launch.
pub fn build_plan(request: LaunchRequest<'_>) -> Result<LaunchPlan> {
    let LaunchRequest {
        paths,
        instance,
        account,
        java,
        version,
        version_id,
    } = request;

    if account.access_token.trim().is_empty() {
        return Err(CoreError::InvalidVersion("missing access token".into()));
    }
    if java.major < version.java_major {
        return Err(CoreError::JavaTooOld {
            required: version.java_major,
            actual: java.major,
        });
    }

    let os = OsInfo::current();
    let game_dir = paths.game_dir(&instance.id);
    std::fs::create_dir_all(&game_dir).at(&game_dir)?;

    let mut features = HashMap::new();
    features.insert("is_demo_user".to_string(), false);
    features.insert("has_custom_resolution".to_string(), true);
    features.insert("has_quick_plays_support".to_string(), false);
    features.insert("is_quick_play_singleplayer".to_string(), false);
    features.insert("is_quick_play_multiplayer".to_string(), false);
    features.insert("is_quick_play_realms".to_string(), false);

    // --- libraries -------------------------------------------------------
    let libraries = version.libraries_for(&os, &features);
    let mut missing = Vec::new();
    let mut classpath: Vec<PathBuf> = Vec::with_capacity(libraries.len() + 1);

    let client_jar = paths.version_jar(&instance.game.version);
    if !client_jar.exists() {
        missing.push(client_jar.clone());
    }
    classpath.push(client_jar);

    for lib in libraries.iter().filter(|l| !l.is_native) {
        let path = library_destination(&paths.libraries_dir(), &lib.relative_path)?;
        if !path.exists() {
            missing.push(path.clone());
        }
        classpath.push(path);
    }
    if !missing.is_empty() {
        return Err(CoreError::InvalidVersion(format!(
            "{} file(s) missing — press Repair in the launcher to fetch them: {}",
            missing.len(),
            missing
                .iter()
                .take(3)
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    // --- natives ---------------------------------------------------------
    let natives_dir = paths.instance_natives_dir(&instance.id);
    extract_natives(paths, &libraries, &natives_dir)?;

    // --- asset root ------------------------------------------------------
    let asset_index = version
        .asset_index
        .clone()
        .ok_or_else(|| CoreError::InvalidVersion("version has no asset index".into()))?;
    let asset_index_path = paths
        .asset_indexes_dir()
        .join(format!("{}.json", asset_index.id));
    let index: Option<AssetIndex> = std::fs::read(&asset_index_path)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok());
    let virtual_assets = index.as_ref().map(|i| i.is_virtual()).unwrap_or(false);
    let assets_root = if virtual_assets {
        paths.asset_virtual_dir(&asset_index.id)
    } else {
        paths.assets_dir()
    };

    // --- logging ---------------------------------------------------------
    let log_file = paths.instance_logs_dir(&instance.id).join(format!(
        "game-{}.log",
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }

    let mut jvm_args = version.jvm_arguments(&os, &features);
    if jvm_args.is_empty() {
        // Legacy version JSONs ship no `arguments.jvm`; supply the minimum.
        jvm_args.push("-Djava.library.path=${natives_directory}".to_string());
        jvm_args.push("-cp".to_string());
        jvm_args.push("${classpath}".to_string());
    }

    let log_config = version.logging.as_ref().map(|logging| {
        let file_name = logging
            .file
            .id
            .clone()
            .or_else(|| logging.file.path.clone())
            .or_else(|| logging.file.url.rsplit('/').next().map(str::to_owned))
            .unwrap_or_else(|| format!("{}-client-1.2.xml", version.id));
        let path = paths.log_configs_dir().join(file_name);
        (path, logging.argument.clone())
    });

    if let Some((path, _)) = &log_config {
        if !path.exists() {
            return Err(CoreError::InvalidVersion(format!(
                "missing logging config: {}",
                path.display()
            )));
        }
    }

    if let Some((path, argument)) = &log_config {
        let flag = argument.replace("${path}", &path.display().to_string());
        if !jvm_args.iter().any(|a| a.starts_with("-Dlog4j.configurationFile")) {
            jvm_args.push(flag);
        }
    }

    let cp = join_classpath(&classpath);
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", account.username.clone());
    vars.insert("version_name", version_id.to_string());
    vars.insert("profile_name", account.username.clone());
    vars.insert("game_directory", game_dir.display().to_string());
    vars.insert("assets_root", assets_root.display().to_string());
    vars.insert("game_assets", assets_root.display().to_string());
    vars.insert("assets_index_name", asset_index.id.clone());
    vars.insert("auth_uuid", account.uuid_undashed());
    vars.insert("auth_access_token", account.access_token.clone());
    vars.insert("auth_session", format!("token:{}", account.access_token));
    vars.insert("clientid", "spud-launcher".to_string());
    vars.insert(
        "auth_xuid",
        account.xuid.clone().unwrap_or_else(|| "0".to_string()),
    );
    vars.insert(
        "user_type",
        if account.user_type.is_empty() {
            "msa".to_string()
        } else {
            account.user_type.clone()
        },
    );
    vars.insert("user_properties", "{}".to_string());
    vars.insert("version_type", version.kind.clone());
    vars.insert("natives_directory", natives_dir.display().to_string());
    vars.insert("launcher_name", "spud".to_string());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").to_string());
    vars.insert("classpath", cp.clone());
    vars.insert(
        "classpath_separator",
        if cfg!(windows) { ";" } else { ":" }.to_string(),
    );
    vars.insert(
        "library_directory",
        paths.libraries_dir().display().to_string(),
    );
    vars.insert("resolution_width", instance.window.width.to_string());
    vars.insert("resolution_height", instance.window.height.to_string());
    if let Some((path, _)) = &log_config {
        vars.insert("path", path.display().to_string());
    }

    // `-Xms`/`-Xmx` are launcher-owned; ignore duplicates from the instance.
    let mut args: Vec<String> = Vec::new();
    args.push(format!("-Xms{}m", instance.java.min_memory_mb));
    args.push(format!("-Xmx{}m", instance.java.max_memory_mb));
    args.push("-Dminecraft.launcher.brand=spud".to_string());
    args.push(format!(
        "-Dminecraft.launcher.version={}",
        env!("CARGO_PKG_VERSION")
    ));

    for arg in jvm_args
        .iter()
        .chain(instance.jvm_args.iter())
        .chain(instance.java.extra_args.iter())
    {
        args.push(substitute(arg, &vars));
    }

    args.push(version.main_class.clone());

    for arg in version.game_arguments(&os, &features) {
        args.push(substitute(&arg, &vars));
    }

    if !instance.window.fullscreen && instance.window.width > 0 && instance.window.height > 0 {
        push_flag(&mut args, "--width", &instance.window.width.to_string());
        push_flag(&mut args, "--height", &instance.window.height.to_string());
    }
    if instance.window.fullscreen {
        args.push("--fullscreen".to_string());
    }

    for extra in &instance.game_args {
        args.push(substitute(extra, &vars));
    }

    let mut env = vec![
        ("JAVA_TOOL_OPTIONS".to_string(), String::new()),
        ("_JAVA_OPTIONS".to_string(), String::new()),
    ];
    env.extend(instance.env.iter().map(|(k, v)| (k.clone(), v.clone())));

    Ok(LaunchPlan {
        program: java.path.clone(),
        args,
        cwd: game_dir,
        env,
        log_file,
        java_major: java.major,
    })
}

/// Load and merge the installed version for this instance.
pub fn merged_for_instance(
    paths: &LauncherPaths,
    instance: &InstanceConfig,
) -> Result<MergedVersion> {
    let version_id = instance
        .game
        .version_id
        .as_deref()
        .ok_or_else(|| CoreError::InvalidVersion("instance is not installed yet".into()))?;
    load_merged(paths, version_id)
}

fn join_classpath(entries: &[PathBuf]) -> String {
    let separator = if cfg!(windows) { ";" } else { ":" };
    entries
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(separator)
}

fn substitute(input: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = input.to_string();
    for (key, value) in vars {
        let needle = format!("${{{key}}}");
        if out.contains(&needle) {
            out = out.replace(&needle, value);
        }
    }
    out
}

fn push_flag(args: &mut Vec<String>, flag: &str, value: &str) {
    if !args.iter().any(|a| a == flag) {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

/// Clear and repopulate the instance natives directory. JVM native libraries
/// load from here via `-Djava.library.path`.
pub fn extract_natives(
    paths: &LauncherPaths,
    libraries: &[LibraryFile],
    destination: &Path,
) -> Result<()> {
    if destination.exists() {
        std::fs::remove_dir_all(destination).at(destination)?;
    }
    std::fs::create_dir_all(destination).at(destination)?;

    for file in libraries.iter().filter(|l| l.is_native) {
        let jar_path = library_destination(&paths.libraries_dir(), &file.relative_path)?;
        if !jar_path.exists() {
            return Err(CoreError::InvalidVersion(format!(
                "missing natives jar: {}",
                jar_path.display()
            )));
        }
        extract_jar(&jar_path, destination)?;
    }
    Ok(())
}

fn extract_jar(jar_path: &Path, destination: &Path) -> Result<()> {
    let file = std::fs::File::open(jar_path).at(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::Archive(format!("{}: {e}", jar_path.display())))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CoreError::Archive(format!("{}: {e}", jar_path.display())))?;
        let Some(name) = entry.enclosed_name() else {
            continue; // zip-slip attempt
        };
        let name_str = name.to_string_lossy();
        if entry.is_dir() || name_str.starts_with("META-INF/") {
            continue;
        }

        let out_path = destination.join(&name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        let mut out = std::fs::File::create(&out_path).at(&out_path)?;
        std::io::copy(&mut entry, &mut out).at(&out_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classpath_uses_platform_separator() {
        let entries = vec![PathBuf::from("a.jar"), PathBuf::from("b.jar")];
        let expected = if cfg!(windows) { "a.jar;b.jar" } else { "a.jar:b.jar" };
        assert_eq!(join_classpath(&entries), expected);
    }

    #[test]
    fn placeholders_are_replaced_once() {
        let mut vars = HashMap::new();
        vars.insert("auth_player_name", "spud".to_string());
        vars.insert("version_name", "1.21.1".to_string());
        assert_eq!(
            substitute("--username=${auth_player_name} --version=${version_name}", &vars),
            "--username=spud --version=1.21.1"
        );
        assert_eq!(substitute("--width=${resolution_width}", &vars), "--width=${resolution_width}");
    }
}
