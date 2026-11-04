use crate::state::AppState;
use tauri::Manager;

mod commands;
mod discord;
mod java_install;
mod running;
mod state;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "spud_launcher=info,launcher_net=info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let state = match AppState::bootstrap() {
        Ok(state) => state,
        Err(error) => {
            eprintln!("spud: cannot start: {error}");
            std::process::exit(1);
        }
    };

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::instances_list,
            commands::instance_get,
            commands::instance_create,
            commands::instance_update,
            commands::instance_delete,
            commands::instance_folder,
            commands::settings_get,
            commands::settings_save,
            commands::java_detect,
            commands::java_inspect,
            commands::java_install,
            commands::minecraft_versions,
            commands::loader_versions,
            commands::data_root,
            commands::accounts_list,
            commands::auth_status,
            commands::account_begin_login,
            commands::account_finish_login,
            commands::account_add_offline,
            commands::account_remove,
            commands::account_set_active,
            commands::instance_install,
            commands::instance_launch,
            commands::instances_running,
            commands::instance_stop,
            commands::instance_required_java,
            commands::window_state,
            commands::window_fullscreen,
            commands::window_minimize,
            commands::modrinth_search,
            commands::modrinth_project,
            commands::modrinth_versions,
            commands::mods_installed,
            commands::mods_install,
            commands::optimization_install,
            commands::mods_remove,
            commands::mods_check_updates,
            commands::modpack_install,
            commands::instance_logs,
            commands::instance_log_files,
            commands::instance_log_read,
            commands::instance_log_stream,
            commands::instance_export,
            commands::instance_import,
            commands::launchers_scan,
            commands::launcher_import,
            commands::friends_get,
            commands::friends_save,
            commands::friends_add,
            commands::friends_remove,
            commands::friends_list,
            commands::friends_archives,
            commands::friends_publish,
            commands::friends_share_instance,
            commands::friends_import,
            commands::hardware_profile,
            commands::tuning_preview,
            commands::tuning_apply,
            commands::store_stats,
            commands::store_dedupe,
            commands::store_gc,
            commands::store_forget,
            commands::plugins_list,
            commands::plugins_set_enabled,
            commands::plugins_open_folder,
            commands::ai_status,
            commands::ai_set_key,
            commands::ai_clear_key,
            commands::ai_analyze_crash,
            commands::sync_status,
            commands::sync_remotes,
            commands::sync_push,
            commands::sync_pull,
            commands::sync_forget_snapshot,
            commands::sync_open_folder,
        ]);

    builder
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Spud Launcher");
            }
            crate::commands::friends::spawn_publisher(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Spud Launcher");
}
