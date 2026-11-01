use std::collections::HashMap;
use std::process::Child;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;

/// How long a graceful stop may take before the process tree is killed.
///
/// Minecraft needs a few seconds to flush the world on shutdown; `WM_CLOSE`
/// from the menu screen measured ~5 s, so the budget leaves room for a save.
const GRACEFUL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

struct GameProcess {
    child: Child,
    pid: u32,
    started_at: DateTime<Utc>,
    log_file: String,
}

/// Live game processes spawned by this launcher session.
///
/// Keeping the `Child` handle is what separates a game that is still loading
/// from one that crashed: `try_wait` answers without spawning `tasklist` on
/// every poll. The registry is in memory by design — a game started before the
/// launcher was closed keeps running, but is no longer tracked afterwards.
#[derive(Default)]
pub struct RunningGames {
    inner: Mutex<HashMap<String, GameProcess>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunningGame {
    pub instance_id: String,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub log_file: String,
}

impl RunningGames {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, GameProcess>>, String> {
        self.inner
            .lock()
            .map_err(|_| "game process registry is poisoned".to_string())
    }

    /// Take ownership of a freshly spawned game.
    ///
    /// A second copy of a live instance is refused *and killed*: the caller
    /// checked before spawning, so reaching this branch means a race, and an
    /// untracked Java process would be worse than a failed launch.
    pub fn register(
        &self,
        instance_id: &str,
        mut child: Child,
        log_file: String,
    ) -> Result<RunningGame, String> {
        let pid = child.id();
        let started_at = Utc::now();
        let mut map = self.lock()?;

        if let Some(existing) = map.get_mut(instance_id) {
            if matches!(existing.child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{instance_id} is already running (pid {}) — stop it first",
                    existing.pid
                ));
            }
            map.remove(instance_id);
        }

        map.insert(
            instance_id.to_string(),
            GameProcess {
                child,
                pid,
                started_at,
                log_file: log_file.clone(),
            },
        );

        Ok(RunningGame {
            instance_id: instance_id.to_string(),
            pid,
            started_at,
            log_file,
        })
    }

    pub fn find(&self, instance_id: &str) -> Option<RunningGame> {
        let mut map = self.inner.lock().ok()?;
        reap(&mut map);
        map.get(instance_id).map(|game| RunningGame {
            instance_id: instance_id.to_string(),
            pid: game.pid,
            started_at: game.started_at,
            log_file: game.log_file.clone(),
        })
    }

    pub fn list(&self) -> Vec<RunningGame> {
        let Ok(mut map) = self.inner.lock() else {
            return Vec::new();
        };
        reap(&mut map);
        let mut out: Vec<RunningGame> = map
            .iter()
            .map(|(instance_id, game)| RunningGame {
                instance_id: instance_id.clone(),
                pid: game.pid,
                started_at: game.started_at,
                log_file: game.log_file.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        out
    }

    /// Stop an instance and return the pid that was terminated.
    ///
    /// The first attempt is deliberately graceful: a `WM_CLOSE` is posted to
    /// every visible window owned by the game, so Minecraft saves the world and
    /// exits on its own. Only when that times out is the tree force-killed.
    pub fn stop(&self, instance_id: &str) -> Result<Option<u32>, String> {
        let game = self.lock()?.remove(instance_id);
        let Some(mut game) = game else {
            return Ok(None);
        };
        let pid = game.pid;

        #[cfg(windows)]
        {
            if matches!(game.child.try_wait(), Ok(None)) {
                // `taskkill /T` without `/F` refuses as soon as the JVM owns a
                // child process, which the game always does. Posting the close
                // request to the window sidesteps that.
                if request_close(pid) == 0 {
                    taskkill(pid, false);
                }
                let deadline = Instant::now() + GRACEFUL_TIMEOUT;
                loop {
                    match game.child.try_wait() {
                        Ok(Some(_)) => return Ok(Some(pid)),
                        Ok(None) if Instant::now() < deadline => {
                            std::thread::sleep(POLL_INTERVAL);
                        }
                        _ => break,
                    }
                }
                taskkill(pid, true);
            }
        }

        let _ = game.child.kill();
        let _ = game.child.wait();
        Ok(Some(pid))
    }
}

fn reap(map: &mut HashMap<String, GameProcess>) {
    map.retain(|_, game| !matches!(game.child.try_wait(), Ok(Some(_))));
}

#[cfg(windows)]
mod win {
    //! Minimal `user32` bindings — only what a polite close needs.

    pub const WM_CLOSE: u32 = 0x0010;

    #[link(name = "user32")]
    extern "system" {
        pub fn EnumWindows(callback: extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
        pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        pub fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
        pub fn IsWindowVisible(hwnd: isize) -> i32;
    }
}

/// Post `WM_CLOSE` to every visible top-level window owned by `pid`.
///
/// Returns how many windows accepted the message. Zero means the process has no
/// window — a headless JVM or an already exiting game — and the caller falls
/// back to `taskkill`.
#[cfg(windows)]
fn request_close(pid: u32) -> usize {
    struct Request {
        pid: u32,
        posted: usize,
    }

    extern "system" fn callback(hwnd: isize, lparam: isize) -> i32 {
        // SAFETY: `lparam` is the `&mut Request` from `request_close`, alive for
        // the whole `EnumWindows` call.
        let request = unsafe { &mut *(lparam as *mut Request) };
        let mut owner = 0u32;
        // SAFETY: `hwnd` comes from the system enumerator.
        unsafe {
            win::GetWindowThreadProcessId(hwnd, &mut owner);
            if owner == request.pid && win::IsWindowVisible(hwnd) != 0 {
                if win::PostMessageW(hwnd, win::WM_CLOSE, 0, 0) != 0 {
                    request.posted += 1;
                }
            }
        }
        1 // keep enumerating
    }

    let mut request = Request { pid, posted: 0 };
    // SAFETY: `request` outlives the enumeration and is only touched from the
    // callback running on this thread.
    unsafe {
        win::EnumWindows(callback, &mut request as *mut Request as isize);
    }
    request.posted
}

#[cfg(windows)]
fn taskkill(pid: u32, force: bool) {
    use std::os::windows::process::CommandExt;

    let pid = pid.to_string();
    let mut command = std::process::Command::new("taskkill");
    command.args(["/PID", pid.as_str(), "/T"]);
    if force {
        command.arg("/F");
    }
    // The launcher owns no console window; taskkill must not flash one.
    command
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = command.status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_exit() -> Child {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/C", "exit", "0"])
                .spawn()
                .expect("spawn cmd")
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("sh")
                .args(["-c", "exit 0"])
                .spawn()
                .expect("spawn sh")
        }
    }

    #[test]
    fn a_finished_game_disappears_from_the_registry() {
        let registry = RunningGames::new();
        registry
            .register("finished", quick_exit(), String::new())
            .expect("register");
        assert_eq!(registry.list().len(), 1);

        std::thread::sleep(Duration::from_millis(400));
        assert!(registry.find("finished").is_none());
        assert!(registry.list().is_empty());
    }

    #[test]
    fn stopping_an_untracked_instance_is_a_no_op() {
        let registry = RunningGames::new();
        assert_eq!(registry.stop("missing").expect("stop"), None);
    }
}
