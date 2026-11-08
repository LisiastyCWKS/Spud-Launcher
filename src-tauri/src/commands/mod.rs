mod accounts;
mod ai;
mod appearance;
pub mod console;
mod curseforge;
pub mod friends;
mod game;
mod instances;
mod launchers;
mod mods;
pub mod overlay;
mod plugins;
mod store;
mod sync;
mod system;
mod tuning;
pub mod transfer;
mod update;

pub use accounts::*;
pub use ai::*;
pub use appearance::*;
pub use console::*;
pub use curseforge::*;
pub use friends::*;
pub use game::*;
pub use instances::*;
pub use launchers::*;
pub use mods::*;
pub use overlay::*;
pub use plugins::*;
pub use store::*;
pub use sync::*;
pub use system::*;
pub use transfer::*;
pub use tuning::*;
pub use update::*;

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
