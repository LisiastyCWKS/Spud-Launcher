//! Console and log plumbing for the diagnostics view.
//!
//! The game already writes everything to `<instance>/logs/game.log` (stdout and
//! stderr share one handle, see `instance_launch`). Tailing that file instead of
//! tapping the process pipes keeps the launch path unchanged: a crashed game
//! still leaves a complete log behind, and a running one streams as it writes.
//!
//! One tailer per instance is enough — starting a second for the same id is a
//! no-op, and the tailer exits on its own when the file stops growing.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use launcher_core::instance::validate_id;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// How often the tailer polls. 300 ms is fast enough to feel live and cheap
/// enough that six running instances cost nothing measurable.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(300);
/// Idle time after which a tailer stops watching a log that has gone quiet.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);
/// Lines returned by a cold read; the UI keeps far fewer in view.
const TAIL_LINES: usize = 400;

#[derive(Debug, Clone, Serialize)]
pub struct LogChunk {
    pub instance_id: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogSnapshot {
    pub instance_id: String,
    pub path: String,
    pub exists: bool,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogFileInfo {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub modified: Option<String>,
}

#[derive(Default)]
pub struct LogTailers {
    active: Mutex<HashMap<String, ()>>,
}

impl LogTailers {
    /// Claim the watcher slot for an instance. `false` means one is already up.
    fn claim(&self, instance_id: &str) -> bool {
        let Ok(mut active) = self.active.lock() else {
            return false;
        };
        if active.contains_key(instance_id) {
            return false;
        }
        active.insert(instance_id.to_string(), ());
        true
    }

    fn release(&self, instance_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(instance_id);
        }
    }
}

fn log_path(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    validate_id(&id).map_err(|e| e.to_string())?;
    Ok(state.paths.instance_dir(id).join("logs").join("game.log"))
}

#[tauri::command]
pub fn instance_logs(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<LogSnapshot, String> {
    let path = log_path(&state, &id)?;
    let lines = read_tail(&path, TAIL_LINES).unwrap_or_default();
    Ok(LogSnapshot {
        instance_id: id,
        path: path.display().to_string(),
        exists: path.exists(),
        lines,
    })
}

/// Everything under `logs/` plus `crash-reports/`, newest first — the two
/// places a mod failure actually leaves evidence.
#[tauri::command]
pub fn instance_log_files(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<LogFileInfo>, String> {
    validate_id(&id).map_err(|e| e.to_string())?;
    let game_dir = state.paths.game_dir(&id);
    let mut out = Vec::new();

    for (folder, extension) in [("logs", "log"), ("crash-reports", "txt")] {
        let dir = game_dir.join(folder);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let matches_extension = path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case(extension))
                .unwrap_or(false);
            if !matches_extension {
                continue;
            }
            let metadata = entry.metadata().ok();
            let modified = metadata
                .as_ref()
                .and_then(|meta| meta.modified().ok())
                .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339());
            out.push(LogFileInfo {
                name: format!("{folder}/{}", entry.file_name().to_string_lossy()),
                path: path.display().to_string(),
                bytes: metadata.map(|meta| meta.len()).unwrap_or(0),
                modified,
            });
        }
    }

    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

/// Full text of one file inside `logs/` or `crash-reports/`, capped so a huge
/// log cannot lock up the webview. `name` is the friendly `logs/xyz.log` form.
#[tauri::command]
pub fn instance_log_read(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
) -> Result<String, String> {
    validate_id(&id).map_err(|e| e.to_string())?;
    let game_dir = state.paths.game_dir(&id);
    let relative = name.trim_start_matches(".minecraft/");
    let path = launcher_core::instance::safe_join(&game_dir, relative).map_err(|e| e.to_string())?;
    if !path.starts_with(game_dir.join("logs")) && !path.starts_with(game_dir.join("crash-reports"))
    {
        return Err("only logs and crash reports can be read".to_string());
    }
    read_capped(&path, 4 * 1024 * 1024)
}

/// Start streaming `game.log` to the webview as `game://log`.
#[tauri::command]
pub fn instance_log_stream(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    start(&app, &state, &id)
}

/// Same watcher, callable from the launch path so the console is live before
/// the user ever opens the diagnostics tab.
pub(crate) fn start(app: &AppHandle, state: &AppState, id: &str) -> Result<bool, String> {
    let path = log_path(state, id)?;
    if !state.logs.claim(id) {
        return Ok(false);
    }
    let handle = app.clone();
    let instance_id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut offset = 0u64;
        let mut last_growth = std::time::Instant::now();

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            let Ok(metadata) = std::fs::metadata(&path) else {
                if last_growth.elapsed() > IDLE_TIMEOUT {
                    break;
                }
                continue;
            };
            let length = metadata.len();
            if length < offset {
                offset = 0; // truncated or recreated between launches
            }
            if length == offset {
                if last_growth.elapsed() > IDLE_TIMEOUT {
                    break;
                }
                continue;
            }

            match read_from(&path, offset, 256 * 1024) {
                Ok((chunk, read)) => {
                    offset += read;
                    last_growth = std::time::Instant::now();
                    if chunk.is_empty() {
                        continue;
                    }
                    let lines: Vec<String> = chunk.lines().map(|line| line.to_string()).collect();
                    let _ = handle.emit(
                        "game://log",
                        LogChunk {
                            instance_id: instance_id.clone(),
                            lines,
                        },
                    );
                }
                Err(_) => {
                    // The writer owns the file; a failed read is retried on the
                    // next tick rather than ending the stream.
                    if last_growth.elapsed() > IDLE_TIMEOUT {
                        break;
                    }
                }
            }
        }

        handle.state::<AppState>().logs.release(&instance_id);
    });

    Ok(true)
}

fn read_from(path: &std::path::Path, offset: u64, cap: u64) -> std::io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0u8; cap as usize];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok((String::from_utf8_lossy(&buffer).into_owned(), read as u64))
}

pub(crate) fn read_capped(path: &std::path::Path, cap: u64) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("cannot open the log: {e}"))?;
    if metadata.len() <= cap {
        return std::fs::read_to_string(path).map_err(|e| format!("cannot read the log: {e}"));
    }
    let start = metadata.len() - cap;
    let (text, _) =
        read_from(path, start, cap).map_err(|e| format!("cannot read the log: {e}"))?;
    Ok(format!("… trimmed to the last {cap} bytes …\n{text}"))
}

/// Last `limit` lines without loading the whole file.
fn read_tail(path: &std::path::Path, limit: usize) -> Option<Vec<String>> {
    let metadata = std::fs::metadata(path).ok()?;
    let window = 512 * 1024u64;
    let start = metadata.len().saturating_sub(window);
    let (text, _) = read_from(path, start, window).ok()?;
    let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tail_keeps_the_last_lines_and_survives_a_short_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("game.log");
        let mut file = std::fs::File::create(&path).expect("create");
        for index in 0..50 {
            writeln!(file, "line {index}").expect("write");
        }
        drop(file);

        let tail = read_tail(&path, 10).expect("tail");
        assert_eq!(tail.len(), 10);
        assert_eq!(tail.last().map(String::as_str), Some("line 49"));
    }

    #[test]
    fn reads_only_new_bytes_after_an_offset() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("game.log");
        std::fs::write(&path, "alpha\n").expect("write");
        let (chunk, read) = read_from(&path, 0, 1024).expect("read");
        assert_eq!(chunk, "alpha\n");
        assert_eq!(read, 6);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| writeln!(file, "beta"))
            .expect("append");
        let (chunk, read) = read_from(&path, 6, 1024).expect("read");
        assert_eq!(chunk, "beta\n");
        assert_eq!(read, 5);
    }
}
