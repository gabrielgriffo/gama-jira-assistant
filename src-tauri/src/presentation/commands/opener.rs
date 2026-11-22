use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::application::error::AppResult;
use crate::domain::credentials::{belongs_to_connection, validate_jira_page_url};
use crate::domain::error::DomainError;
use crate::infrastructure::database::JiraConnectionRepository;

const REPOSITORY_URL: &str = "https://github.com/gabrielgriffo/gama-jira-assistant";
const RELEASES_URL: &str = "https://github.com/gabrielgriffo/gama-jira-assistant/releases";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppPage {
    Repository,
    Releases,
}

impl AppPage {
    fn url(self) -> &'static str {
        match self {
            AppPage::Repository => REPOSITORY_URL,
            AppPage::Releases => RELEASES_URL,
        }
    }
}

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
    open_in_browser(&app, url.as_str())
}

#[tauri::command]
pub fn open_app_page(app: AppHandle, page: AppPage) -> AppResult<()> {
    open_in_browser(&app, page.url())
}

fn open_in_browser(app: &AppHandle, url: &str) -> AppResult<()> {
    app.opener().open_url(url, None::<&str>).map_err(|e| {
        log::warn!("[OPENER] falha ao abrir o navegador: {e}");
        DomainError::ExternalApiError("não foi possível abrir o navegador".to_string()).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_pages_point_to_the_project_on_github() {
        for page in [AppPage::Repository, AppPage::Releases] {
            assert!(page.url().starts_with(REPOSITORY_URL));
        }
        assert!(AppPage::Releases.url().ends_with("/releases"));
    }

    #[test]
    fn only_known_pages_are_accepted() {
        assert_eq!(
            serde_json::from_str::<AppPage>("\"releases\"").unwrap(),
            AppPage::Releases
        );
        assert!(serde_json::from_str::<AppPage>("\"https://evil.com\"").is_err());
    }
}
