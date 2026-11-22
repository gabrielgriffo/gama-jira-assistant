use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use sqlx::sqlite::SqlitePoolOptions;
use tauri::{App, Manager};

use crate::application::{
    JiraAuthService, JiraService, JiraWorklogService, JiraWorklogSyncService, PontoMaisAuthService, SyncThrottle,
    WorklogService, WorklogTimelineService,
};
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::database::migrations::{self, DatabaseMigrator};
use crate::infrastructure::database::Repositories;
use crate::infrastructure::events::TauriEventPublisher;
use crate::infrastructure::integration::jira::{JiraClient, JiraProvider};
use crate::infrastructure::integration::pontomais::{PontoMaisClient, PontoMaisProvider};
use crate::infrastructure::pontomais_state::PontoMaisStateStore;
use crate::infrastructure::preferences::{PreferencesManager, DEFAULT_MIN_SYNC_INTERVAL_MINUTES};
use crate::infrastructure::secrets::SecretStore;

const DATABASE_FILE: &str = "gama.db";

pub fn setup(app: &mut App) -> Result<(), Box<dyn Error>> {
    let app_data_dir = app.path().app_data_dir()?;
    let app_config_dir = app.path().app_config_dir()?;

    std::fs::create_dir_all(&app_data_dir)?;
    restrict_dir_to_owner(&app_data_dir)?;

    let repos = tauri::async_runtime::block_on(open_database(&app_data_dir.join(DATABASE_FILE)))?;

    let event_publisher = TauriEventPublisher::new(app.handle().clone());
    let preferences_manager = PreferencesManager::new(app_config_dir);
    let min_sync_interval = preferences_manager
        .load()
        .map(|p| p.min_sync_interval_minutes)
        .unwrap_or(DEFAULT_MIN_SYNC_INTERVAL_MINUTES);
    let sync_throttle = Arc::new(SyncThrottle::new(min_sync_interval, Arc::new(event_publisher.clone())));

    let credential_service = CredentialService::new(SecretStore::new(), repos.connection.clone());
    let jira_client = JiraClient::new();
    let jira_provider = JiraProvider::new(jira_client.clone());
    let pontomais_client = PontoMaisClient::new();
    let pontomais_provider = PontoMaisProvider::new(pontomais_client.clone());
    let pontomais_auth_service = PontoMaisAuthService::new(
        pontomais_client,
        credential_service.clone(),
        PontoMaisStateStore::new(app_data_dir),
    );

    app.manage(preferences_manager);
    app.manage(Arc::clone(&sync_throttle));
    app.manage(JiraAuthService::new(jira_provider.clone(), credential_service.clone()));
    app.manage(JiraService::new(
        JiraProvider::new(jira_client),
        credential_service.clone(),
        repos.issue.clone(),
        repos.status.clone(),
        repos.changelog.clone(),
        event_publisher,
        Arc::clone(&sync_throttle),
    ));
    app.manage(JiraWorklogService::new(
        repos.worklog.clone(),
        credential_service.clone(),
    ));
    app.manage(JiraWorklogSyncService::new(
        &repos,
        pontomais_provider.clone(),
        pontomais_auth_service.clone(),
        jira_provider.clone(),
        credential_service.clone(),
        sync_throttle,
    ));
    app.manage(WorklogTimelineService::new(
        repos.pontomais_entry.clone(),
        repos.changelog.clone(),
        repos.issue.clone(),
        repos.worklog.clone(),
        credential_service.clone(),
    ));
    app.manage(WorklogService::new(jira_provider.clone(), repos.worklog.clone()));
    app.manage(jira_provider);
    app.manage(pontomais_provider);
    app.manage(pontomais_auth_service);
    app.manage(credential_service);
    app.manage(repos.connection);
    app.manage(repos.issue);
    app.manage(repos.status);
    app.manage(repos.changelog);
    app.manage(repos.pontomais_entry);

    Ok(())
}

async fn open_database(path: &Path) -> Result<Repositories, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .connect_with(migrations::connect_options(path))
        .await?;
    restrict_to_owner(path)?;
    DatabaseMigrator::new(pool.clone()).run_migrations().await?;
    Ok(Repositories::new(pool))
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_dir_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_dir_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
