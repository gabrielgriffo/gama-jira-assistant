#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod application;
mod domain;
mod infrastructure;
mod presentation;

use application::{
    JiraAuthService, JiraService, JiraWorklogService, JiraWorklogSyncService, PontoMaisAuthService, SyncThrottle,
    WorklogService, WorklogTimelineService,
};
use infrastructure::credentials::CredentialService;
use infrastructure::database::migrations;
use infrastructure::database::{
    migrations::DatabaseMigrator, ChangelogRepository, IssueRepository, JiraConnectionRepository,
    PontoMaisEntryRepository, SqliteWorklogRepository, StatusRepository,
};
use infrastructure::events::TauriEventPublisher;
use infrastructure::integration::jira::{JiraClient, JiraProvider};
use infrastructure::integration::pontomais::{PontoMaisClient, PontoMaisProvider};
use infrastructure::pontomais_state::PontoMaisStateStore;
use infrastructure::preferences::{PreferencesManager, DEFAULT_MIN_SYNC_INTERVAL_MINUTES};
use infrastructure::secrets::SecretStore;
use presentation::commands::jira::{
    get_cached_status_history, get_cached_tasks, get_connection_info, get_connection_status, get_jira_status_catalog,
    get_jira_status_catalog_synced_at, get_jira_time_zone, get_status_history, list_my_tasks, login, logout,
    sync_jira_status_catalog, update_jira_statuses_is_development,
};
use presentation::commands::jira_worklog::{get_jira_worklogs_for_period, sync_jira_worklogs, sync_worklog_period};
use presentation::commands::pontomais::{
    get_pontomais_timeline, pontomais_connect, pontomais_disconnect, pontomais_status, sync_pontomais_entries,
};
use presentation::commands::preferences::{
    clear_local_data, get_active_syncs, get_preferences, update_min_sync_interval, update_range_preference,
    update_theme,
};
use presentation::commands::worklog::save_and_sync_worklogs_to_jira;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tauri::Manager;

const LOG_MAX_FILE_SIZE_BYTES: u128 = 5 * 1024 * 1024;
const LOG_MAX_FILES: usize = 5;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_prevent_default::debug())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            log::info!("GAMA iniciado - logging plugin ativo");

            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_window_state::Builder::new().build())?;

            let app_data_dir = app.path().app_data_dir()?;
            let db_path = app_data_dir.join("gama.db");
            let app_config_dir = app.path().app_config_dir()?;

            std::fs::create_dir_all(db_path.parent().expect("db path always has a parent directory"))?;

            let (
                connection_repository,
                issue_repository,
                status_repository,
                changelog_repository,
                pontomais_entry_repository,
                worklog_repository,
            ) = tauri::async_runtime::block_on(async {
                let pool = SqlitePoolOptions::new()
                    .connect_with(migrations::connect_options(&db_path))
                    .await?;

                restrict_to_owner(&db_path)?;

                let migrator = DatabaseMigrator::new(pool.clone());
                migrator.run_migrations().await?;

                let connection_repository = JiraConnectionRepository::new(pool.clone());
                let issue_repository = IssueRepository::new(pool.clone());
                let status_repository = StatusRepository::new(pool.clone());
                let changelog_repository = ChangelogRepository::new(pool.clone());
                let pontomais_entry_repository = PontoMaisEntryRepository::new(pool.clone());
                let worklog_repository = SqliteWorklogRepository::new(pool);

                Ok::<_, Box<dyn std::error::Error>>((
                    connection_repository,
                    issue_repository,
                    status_repository,
                    changelog_repository,
                    pontomais_entry_repository,
                    worklog_repository,
                ))
            })?;

            let event_publisher = TauriEventPublisher::new(app.handle().clone());
            let preferences_manager = PreferencesManager::new(app_config_dir);
            let min_sync_interval = preferences_manager
                .load()
                .map(|p| p.min_sync_interval_minutes)
                .unwrap_or(DEFAULT_MIN_SYNC_INTERVAL_MINUTES);
            let sync_throttle = Arc::new(SyncThrottle::new(min_sync_interval, Arc::new(event_publisher.clone())));
            app.manage(preferences_manager);
            app.manage(Arc::clone(&sync_throttle));

            let credential_service = CredentialService::new(SecretStore::new(), connection_repository.clone());
            let jira_client = JiraClient::new();
            let jira_provider = JiraProvider::new(jira_client.clone());
            let pontomais_client = PontoMaisClient::new();
            let pontomais_provider = PontoMaisProvider::new(pontomais_client.clone());
            let pontomais_auth_service = PontoMaisAuthService::new(
                pontomais_client,
                credential_service.clone(),
                PontoMaisStateStore::new(app_data_dir),
            );

            app.manage(connection_repository);
            app.manage(issue_repository.clone());
            app.manage(status_repository.clone());
            app.manage(changelog_repository.clone());
            app.manage(JiraAuthService::new(jira_provider.clone(), credential_service.clone()));
            app.manage(JiraService::new(
                JiraProvider::new(jira_client.clone()),
                credential_service.clone(),
                issue_repository.clone(),
                status_repository.clone(),
                changelog_repository.clone(),
                event_publisher.clone(),
                Arc::clone(&sync_throttle),
            ));
            app.manage(JiraWorklogService::new(
                worklog_repository.clone(),
                credential_service.clone(),
            ));
            app.manage(JiraWorklogSyncService::new(
                pontomais_entry_repository.clone(),
                pontomais_provider.clone(),
                pontomais_auth_service.clone(),
                changelog_repository.clone(),
                issue_repository.clone(),
                status_repository,
                worklog_repository.clone(),
                jira_provider.clone(),
                credential_service.clone(),
                Arc::clone(&sync_throttle),
            ));
            app.manage(WorklogTimelineService::new(
                pontomais_entry_repository.clone(),
                changelog_repository.clone(),
                issue_repository,
                worklog_repository.clone(),
                credential_service.clone(),
            ));
            app.manage(WorklogService::new(jira_provider.clone(), worklog_repository));
            app.manage(jira_provider);
            app.manage(pontomais_provider);
            app.manage(pontomais_auth_service);
            app.manage(pontomais_entry_repository);
            app.manage(credential_service);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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

#[cfg(unix)]
fn restrict_to_owner(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
