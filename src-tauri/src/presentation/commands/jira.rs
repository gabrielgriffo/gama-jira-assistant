use std::sync::Arc;

use tauri::State;
use zeroize::Zeroizing;

use crate::application::error::AppResult;
use crate::application::{JiraAuthService, JiraService, SyncThrottle};
use crate::domain::changelog::ChangelogResponse;
use crate::domain::current_user::CurrentUser;
use crate::domain::date_range::DateRange;
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, Status};
use crate::domain::issue_key::validate_issue_id_or_key;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::database::{JiraConnectionRepository, StatusRepository};
use crate::infrastructure::events::TauriEventPublisher;
use crate::infrastructure::integration::jira::JiraProvider;

const MAX_STATUS_UPDATES: usize = 1000;

pub type AppAuthService = JiraAuthService<JiraProvider, CredentialService>;

pub type AppJiraService = JiraService<CredentialService, TauriEventPublisher>;

#[tauri::command]
pub async fn login(
    auth: State<'_, AppAuthService>,
    throttle: State<'_, Arc<SyncThrottle>>,
    base_url: String,
    email: String,
    api_token: String,
) -> AppResult<CurrentUser> {
    let api_token = Zeroizing::new(api_token);
    let user = auth.login(&base_url, &email, &api_token).await?;
    throttle.clear();
    Ok(user)
}

#[tauri::command]
pub async fn logout(auth: State<'_, AppAuthService>, throttle: State<'_, Arc<SyncThrottle>>) -> AppResult<()> {
    auth.logout().await?;
    throttle.clear();
    Ok(())
}

#[tauri::command]
pub async fn get_connection_status(auth: State<'_, AppAuthService>) -> AppResult<bool> {
    auth.connection_status().await
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub instance: String,
    pub user: CurrentUser,
}

#[tauri::command]
pub async fn get_connection_info(
    connection_repo: State<'_, JiraConnectionRepository>,
) -> AppResult<Option<ConnectionInfo>> {
    Ok(connection_repo.get_active().await?.map(|record| ConnectionInfo {
        instance: record.base_url,
        user: CurrentUser {
            account_id: record.account_id,
            display_name: record.display_name,
            email: record.email,
            time_zone: record.time_zone,
        },
    }))
}

#[tauri::command]
pub async fn get_jira_time_zone(connection_repo: State<'_, JiraConnectionRepository>) -> AppResult<Option<String>> {
    Ok(connection_repo.get_time_zone().await?)
}

#[tauri::command]
pub async fn list_my_tasks(
    jira: State<'_, AppJiraService>,
    from: String,
    to: String,
    force: bool,
) -> AppResult<Vec<Issue>> {
    let range = DateRange::parse(&from, &to)?;
    jira.list_my_issues_with_background_changelog_sync(range, force).await
}

#[tauri::command]
pub async fn get_cached_tasks(jira: State<'_, AppJiraService>, from: String, to: String) -> AppResult<Vec<Issue>> {
    let range = DateRange::parse(&from, &to)?;
    jira.get_cached_issues(&range).await
}

#[tauri::command]
pub async fn get_status_history(jira: State<'_, AppJiraService>, issue_key: String) -> AppResult<ChangelogResponse> {
    jira.get_status_history(validate_issue_id_or_key(&issue_key)?).await
}

#[tauri::command]
pub async fn get_cached_status_history(
    jira: State<'_, AppJiraService>,
    issue_key: String,
) -> AppResult<ChangelogResponse> {
    jira.get_cached_status_history(validate_issue_id_or_key(&issue_key)?)
        .await
}

#[tauri::command]
pub async fn sync_jira_status_catalog(jira: State<'_, AppJiraService>) -> AppResult<()> {
    jira.sync_status_catalog().await
}

#[tauri::command]
pub async fn get_jira_status_catalog(status_repo: State<'_, StatusRepository>) -> AppResult<Vec<Status>> {
    Ok(status_repo.get_all().await?)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDevelopmentUpdate {
    pub status_id: String,
    pub is_development: bool,
}

#[tauri::command]
pub async fn update_jira_statuses_is_development(
    status_repo: State<'_, StatusRepository>,
    updates: Vec<StatusDevelopmentUpdate>,
) -> AppResult<()> {
    if updates.len() > MAX_STATUS_UPDATES {
        return Err(DomainError::InvalidInput("quantidade de status excede o limite".to_string()).into());
    }
    if let Some(invalid) = updates
        .iter()
        .find(|u| u.status_id.is_empty() || u.status_id.len() > 32 || !u.status_id.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(DomainError::InvalidInput(format!("id de status inválido: {}", invalid.status_id.len())).into());
    }
    let updates: Vec<(String, bool)> = updates.into_iter().map(|u| (u.status_id, u.is_development)).collect();
    Ok(status_repo.update_multiple_is_development(&updates).await?)
}

#[tauri::command]
pub async fn get_jira_status_catalog_synced_at(status_repo: State<'_, StatusRepository>) -> AppResult<Option<String>> {
    Ok(status_repo.get_sync_timestamp().await?)
}
