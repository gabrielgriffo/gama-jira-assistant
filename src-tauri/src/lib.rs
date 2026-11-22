#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod application;
mod bootstrap;
mod domain;
mod infrastructure;
mod presentation;

use presentation::commands::jira::{
    get_cached_status_history, get_cached_tasks, get_connection_info, get_connection_status, get_jira_status_catalog,
    get_jira_status_catalog_synced_at, get_jira_time_zone, get_status_history, list_my_tasks, login, logout,
    sync_jira_status_catalog, update_jira_statuses_is_development,
};
use presentation::commands::jira_worklog::{get_jira_worklogs_for_period, sync_jira_worklogs, sync_worklog_period};
use presentation::commands::opener::open_jira_url;
use presentation::commands::pontomais::{
    get_pontomais_timeline, pontomais_connect, pontomais_disconnect, pontomais_status, sync_pontomais_entries,
};
use presentation::commands::preferences::{
    clear_local_data, get_active_syncs, get_preferences, update_min_sync_interval, update_range_preference,
    update_theme,
};
use presentation::commands::window::{
    focus_main_window, prepare_main_window, show_main_window, window_state_flags, MAIN_WINDOW,
};
use presentation::commands::worklog::save_and_sync_worklogs_to_jira;

const LOG_MAX_FILE_SIZE_BYTES: u128 = 5 * 1024 * 1024;
const LOG_MAX_FILES: usize = 5;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app)
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    tauri_plugin_log::log::LevelFilter::Debug
                } else {
                    tauri_plugin_log::log::LevelFilter::Info
                })
                .max_file_size(LOG_MAX_FILE_SIZE_BYTES)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(LOG_MAX_FILES))
                .build(),
        )
        .plugin(prevent_default())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(window_state_flags())
                .skip_initial_state(MAIN_WINDOW)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            log::info!("Gama iniciado - logging plugin ativo");
            prepare_main_window(app.handle());
            bootstrap::setup(app)
        })
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            open_jira_url,
            login,
            logout,
            get_connection_status,
            get_connection_info,
            get_jira_time_zone,
            list_my_tasks,
            get_cached_tasks,
            get_status_history,
            get_cached_status_history,
            sync_jira_status_catalog,
            get_jira_status_catalog,
            update_jira_statuses_is_development,
            get_jira_status_catalog_synced_at,
            save_and_sync_worklogs_to_jira,
            get_jira_worklogs_for_period,
            sync_jira_worklogs,
            sync_worklog_period,
            pontomais_connect,
            pontomais_disconnect,
            pontomais_status,
            sync_pontomais_entries,
            get_pontomais_timeline,
            get_preferences,
            update_theme,
            update_range_preference,
            update_min_sync_interval,
            get_active_syncs,
            clear_local_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn prevent_default<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri_plugin_prevent_default::Builder::new().with_flags(tauri_plugin_prevent_default::Flags::debug());
    #[cfg(windows)]
    let builder = builder.platform(
        tauri_plugin_prevent_default::PlatformOptions::new()
            .general_autofill(false)
            .password_autosave(false),
    );
    builder.build()
}
