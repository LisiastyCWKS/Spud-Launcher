//! Discord Rich Presence over the local IPC pipe.
//!
//! No plugin and no dev dependency: Discord exposes a named pipe
//! (`\\.\pipe\discord-ipc-N`), and the protocol is length-prefixed JSON â€”
//! four bytes little-endian, then one UTF-8 frame.
//!
//! A single worker thread owns the connection and waits on a channel. Every
//! `set` overwrites the *desired* activity; the worker only pushes when the
//! socket is up. Discord restarting, not being installed, or rejecting the app
//! id are all non-events: the launcher keeps working, the worker retries in the
//! background.

use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Retry cadence while no Discord client is listening.
const RETRY: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub details: String,
    pub state: String,
}

/// Handle owned by `AppState`. Dropping it stops the worker.
pub struct DiscordPresence {
    tx: Sender<Option<Activity>>,
}

impl DiscordPresence {
    /// Spawn the worker. `client_id` empty means presence is disabled â€” the
    /// worker is not started at all.
    pub fn start(client_id: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        if let Some(client_id) = client_id.filter(|id| !id.is_empty()) {
            std::thread::Builder::new()
                .name("discord-presence".into())
                .spawn(move || worker(&client_id, rx))
                .ok();
        }
        Self { tx }
    }

    /// Publish what the launcher is doing. Repeats are dropped by the worker.
    pub fn set(&self, activity: Option<Activity>) {
        let _ = self.tx.send(activity);
    }
}

fn worker(client_id: &str, rx: Receiver<Option<Activity>>) {
    let started = Instant::now();
    let mut desired: Option<Activity> = Some(idle());
    let mut pipe: Option<Pipe> = None;
    let mut next_attempt = Instant::now();

    loop {
        // Drain the queue without blocking once the socket is up; with no
        // socket, block so an idle launcher costs nothing.
        let received = if pipe.is_some() {
            rx.recv_timeout(Duration::from_millis(250)).ok()
        } else if Instant::now() >= next_attempt {
            match rx.recv_timeout(Duration::from_millis(1)) {
                Ok(value) => Some(value),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv_timeout(next_attempt.saturating_duration_since(Instant::now())) {
                Ok(value) => Some(value),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };

        if let Some(value) = received {
            desired = value;
            if let Some(active) = pipe.as_mut() {
                if push(active, &desired, started).is_err() {
                    // A dropped socket is the only reliable "Discord is gone"
                    // signal; the next loop reconnects.
                    pipe = None;
                    next_attempt = Instant::now() + RETRY;
                }
            }
        }

        if pipe.is_none() {
            if Instant::now() < next_attempt {
                continue;
            }
            next_attempt = Instant::now() + RETRY;
            match Pipe::connect(client_id) {
                Some(mut fresh) => {
                    if push(&mut fresh, &desired, started).is_ok() {
                        pipe = Some(fresh);
                    }
                }
                None => continue,
            }
        }
    }
}

pub fn idle() -> Activity {
    Activity {
        details: "In the launcher".to_string(),
        state: "Picking an instance".to_string(),
    }
}

pub fn playing(instance: &str, loader: &str, version: &str) -> Activity {
    Activity {
        details: format!("Playing {instance}"),
        state: format!("{loader} Â· {version}"),
    }
}

struct Pipe {
    handle: std::fs::File,
}

impl Pipe {
    fn connect(client_id: &str) -> Option<Self> {
        for index in 0..10 {
            let path = format!(r"\\.\pipe\discord-ipc-{index}");
            let Ok(handle) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
            else {
                continue;
            };
            let mut pipe = Self { handle };
            let handshake = json!({ "v": 1, "client_id": client_id });
            if pipe.send(&handshake).is_err() {
                continue;
            }
            // Discord answers the handshake with a READY frame. The pipe is a
            // plain `File` on this side and cannot be given a read timeout, so
            // the reply stays in the kernel buffer instead of being drained:
            // the payloads here are tiny and a write failure is the signal the
            // connection died.
            return Some(pipe);
        }
        None
    }

    fn send(&mut self, payload: &Value) -> std::io::Result<()> {
        let raw = serde_json::to_vec(payload)?;
        let mut frame = Vec::with_capacity(raw.len() + 4);
        frame.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        frame.extend_from_slice(&raw);
        self.handle.write_all(&frame)?;
        self.handle.flush()
    }
}

fn push(pipe: &mut Pipe, activity: &Option<Activity>, started: Instant) -> Result<(), ()> {
    let payload = match activity {
        Some(activity) => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": {
                    "details": truncate(&activity.details),
                    "state": truncate(&activity.state),
                    "timestamps": { "start": started_epoch_ms(started) },
                    "assets": { "large_text": "Spud Launcher" }
                }
            },
            "nonce": nonce()
        }),
        None => json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": null },
            "nonce": nonce()
        }),
    };

    pipe.send(&payload).map_err(|_| ())?;
    Ok(())
}

/// Discord rejects details/state above 128 characters with a silent no-op.
fn truncate(value: &str) -> String {
    value.chars().take(120).collect()
}

fn started_epoch_ms(started: Instant) -> i64 {
    let elapsed = started.elapsed().as_millis() as i64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0);
    (now - elapsed).max(0)
}

fn nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("spud-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_are_truncated_to_the_discord_limit() {
        let long = "x".repeat(300);
        assert_eq!(truncate(&long).chars().count(), 120);
    }

    #[test]
    fn nonces_are_unique() {
        assert_ne!(nonce(), nonce());
    }
}
