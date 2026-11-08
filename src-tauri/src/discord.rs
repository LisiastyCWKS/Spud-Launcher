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
        state: format!("{loader} \u{00b7} {version}"),
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
            // Opcode 0 = handshake
            let handshake = json!({ "v": 1, "client_id": client_id });
            if pipe.send_op(0, &handshake).is_err() {
                continue;
            }
            // Discord responds with opcode 1 (FRAME) containing READY.
            // We MUST drain this reply — if we skip it, the kernel pipe buffer
            // fills up on the next write and SET_ACTIVITY is silently lost.
            // Spawn a short-lived reader thread so we can impose a timeout
            // without blocking the whole presence worker indefinitely.
            let handle_clone = match pipe.try_clone() {
                Some(c) => c,
                None => continue,
            };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(read_frame(handle_clone));
            });
            match rx.recv_timeout(std::time::Duration::from_secs(3)) {
                Ok(Ok(_)) => return Some(pipe),
                _ => continue,
            }
        }
        None
    }

    /// Send an opcode + JSON payload in the Discord wire format:
    /// [op: u32 LE][len: u32 LE][json bytes]
    fn send_op(&mut self, op: u32, payload: &Value) -> std::io::Result<()> {
        let raw = serde_json::to_vec(payload)?;
        let mut frame = Vec::with_capacity(raw.len() + 8);
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        frame.extend_from_slice(&raw);
        self.handle.write_all(&frame)?;
        self.handle.flush()
    }

    /// Convenience: opcode 1 (FRAME) is what SET_ACTIVITY uses.
    fn send(&mut self, payload: &Value) -> std::io::Result<()> {
        self.send_op(1, payload)
    }

    fn try_clone(&self) -> Option<std::fs::File> {
        self.handle.try_clone().ok()
    }
}

fn read_frame(mut handle: std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut header = [0u8; 8];
    handle.read_exact(&mut header)?;
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    // Cap at 64 KB — a READY frame is always tiny.
    if len > 65536 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut body = vec![0u8; len];
    handle.read_exact(&mut body)?;
    Ok(body)
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
                    "assets": {
                        "large_image": "spud",
                        "large_text": "Spud Launcher"
                    }
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
