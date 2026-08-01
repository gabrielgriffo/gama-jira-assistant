use tauri::State;

use crate::application::error::AppResult;
use crate::application::WorklogService;
use crate::domain::ports::CredentialStore;
use crate::domain::worklog::WorklogEntry;
use crate::infrastructure::credentials::CredentialService;

pub type AppWorklogService = WorklogService;

#[tauri::command]
pub async fn save_and_sync_worklogs_to_jira(
    worklogs: Vec<WorklogEntry>,
    worklog_service: State<'_, AppWorklogService>,
    credential_service: State<'_, CredentialService>,
) -> AppResult<usize> {
    let credentials = credential_service.get_jira_credentials().await?;
    let user = credential_service.get_current_user().await?;
    let synced = worklog_service
        .submit_worklogs_to_jira(&credentials, &user, &worklogs)
        .await?;
    Ok(synced)
}
