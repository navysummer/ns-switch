pub mod adapters;
pub mod commands;
pub mod config;
pub mod conv;
pub mod db;
pub mod models;
pub mod presets;
pub mod tray;
#[derive(Debug)]
pub struct AppError(pub String);

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AppError {}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError(e.to_string())
    }
}

impl From<String> for AppError {
    fn from(e: String) -> Self {
        AppError(e)
    }
}

impl From<&str> for AppError {
    fn from(e: &str) -> Self {
        AppError(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn state_err<T>(msg: &str) -> AppResult<T> {
    Err(AppError(msg.to_string()))
}

pub fn err(msg: &str) -> AppError {
    AppError(msg.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = db::default_data_dir();
    let state = match db::AppState::open(&data_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to init data dir: {e}");
            std::process::exit(1);
        }
    };
    db::ensure_autobackup(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state)
        .manage(std::sync::Arc::new(commands::sessions::ProxyManager::default()))
        .setup(|app| {
            let _ = env_logger::builder()
                .filter_level(log::LevelFilter::Info)
                .try_init();
            if let Err(e) = crate::tray::setup(app.handle()) {
                eprintln!("tray setup failed: {e}");
            }
            // auto-import ns-switch:// links passed on the command line
            let urls: Vec<String> = std::env::args()
                .skip(1)
                .filter(|a| a.starts_with("ns-switch://") || a.starts_with("ccswitch://"))
                .collect();
            if !urls.is_empty() {
                use tauri::{Emitter, Manager};
                let text = urls.join("\n");
                let state = app.state::<crate::db::AppState>();
                if let Ok(result) = crate::commands::deeplink::import_deep_links_impl(&state, text) {
                    let _ = app.emit("deeplink-imported", result.message);
                }
            }
            // auto-takeover: start proxies for tools marked enabled in Settings
            {
                use tauri::Manager as _;
                for (tid, _) in crate::db::ALL_TOOLS {
                    let tid = tid.to_string();
                    let state = app.state::<crate::db::AppState>();
                    let has_providers = crate::commands::providers::load_providers(&state, &tid)
                        .map(|v| !v.is_empty())
                        .unwrap_or(false);
                    if !has_providers {
                        continue;
                    }
                    let enabled =
                        crate::commands::sessions::proxy_takeover_enabled(&state, &tid);
                    if !enabled {
                        continue;
                    }
                    let started = tauri::async_runtime::block_on(
                        crate::commands::sessions::proxy_start(
                            app.state::<crate::db::AppState>(),
                            app.state::<std::sync::Arc<crate::commands::sessions::ProxyManager>>(),
                            tid,
                        ),
                    );
                    if let Err(e) = started {
                        eprintln!("auto-start proxy takeover failed: {e}");
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::providers::get_tools,
            commands::providers::set_tool_visible,
            commands::providers::get_providers,
            commands::providers::get_active_provider,
            commands::providers::create_provider,
            commands::providers::update_provider,
            commands::providers::delete_provider,
            commands::providers::duplicate_provider,
            commands::providers::switch_provider,
            commands::providers::get_presets,
            commands::providers::export_providers,
            commands::providers::import_providers,
            commands::providers::import_from_cli,
            commands::providers::import_all_from_cli,
            commands::providers::onboarding_status,
            commands::providers::complete_onboarding,
            commands::providers::reorder_providers,
            commands::providers::reset_to_official,
            commands::providers::get_config_snapshot,
            commands::mcp::mcp_list,
            commands::mcp::mcp_save,
            commands::mcp::mcp_delete,
            commands::mcp::mcp_sync,
            commands::skills::skills_list,
            commands::skills::skill_install,
            commands::skills::skill_uninstall,
            commands::skills::skill_sources_list,
            commands::skills::skill_source_add,
            commands::skills::skill_source_remove,
            commands::prompts::prompts_list,
            commands::prompts::prompt_save,
            commands::prompts::prompt_delete,
            commands::prompts::prompt_activate,
            commands::prompts::prompt_deactivate,
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::settings::backup_now,
            commands::settings::list_backups,
            commands::settings::restore_backup,
            commands::settings::check_updates,
            commands::sessions::list_sessions,
            commands::sessions::open_path,
            commands::sessions::restore_session,
            commands::sessions::proxy_statuses,
            commands::sessions::proxy_health,
            commands::sessions::proxy_start,
            commands::sessions::proxy_stop,
            commands::usage::usage_overview,
            commands::usage::usage_by_day,
            commands::usage::usage_logs,
            commands::usage::usage_events,
            commands::usage::usage_by_model,
            commands::usage::usage_pricing_get,
            commands::usage::usage_pricing_set,
            commands::usage::usage_scan_sessions,
            commands::usage::usage_reset,
            commands::deeplink::import_deep_links,
            commands::deeplink::register_scheme,
            commands::cloud::cloud_get,
            commands::cloud::cloud_set,
            commands::cloud::cloud_info,
            commands::cloud::cloud_test,
            commands::cloud::cloud_push,
            commands::cloud::cloud_pull,
            commands::cloud::cloud_set_config_dir,
            commands::workspace::workspace_files,
            commands::workspace::workspace_read,
            commands::workspace::workspace_save,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}