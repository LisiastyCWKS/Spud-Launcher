mod accounts;
pub mod console;
pub mod friends;
mod game;
mod instances;
mod launchers;
mod mods;
mod system;
pub mod transfer;

pub use accounts::*;
pub use console::*;
pub use friends::*;
pub use game::*;
pub use instances::*;
pub use launchers::*;
pub use mods::*;
pub use system::*;
pub use transfer::*;

use tauri::AppHandle;
use tokio::sync::mpsc::{self, UnboundedSender};

use launcher_net::download::DownloadEvent;

/// Forward downloader progress to the webview. The returned sender must be
/// dropped when the install finishes so the forwarder task exits.
pub(crate) fn progress_channel(
    app: &AppHandle,
    event_name: &'static str,
) -> UnboundedSender<DownloadEvent> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            use tauri::Emitter;
            let _ = handle.emit(event_name, &event);
        }
    });
    tx
}
