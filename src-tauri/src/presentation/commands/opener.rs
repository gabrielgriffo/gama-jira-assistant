use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::application::error::AppResult;
use crate::domain::credentials::{belongs_to_connection, validate_jira_page_url};
use crate::domain::error::DomainError;
use crate::infrastructure::database::JiraConnectionRepository;

#[tauri::command]
pub async fn open_jira_url(
    app: AppHandle,
    connection_repo: State<'_, JiraConnectionRepository>,
    url: String,
) -> AppResult<()> {
    let url = validate_jira_page_url(&url)?;
    let connected = connection_repo.get_active().await?;
    if !connected.is_some_and(|connection| belongs_to_connection(&url, &connection.base_url)) {
        return Err(DomainError::InvalidInput("o endereço não pertence ao Jira conectado".to_string()).into());
    }
    app.opener().open_url(url.as_str(), None::<&str>).map_err(|e| {
        log::warn!("[OPENER] falha ao abrir o navegador: {e}");
        DomainError::ExternalApiError("não foi possível abrir o navegador".to_string()).into()
    })
}
