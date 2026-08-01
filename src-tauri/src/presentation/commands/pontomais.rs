use tauri::State;
use zeroize::Zeroizing;

use crate::application::error::AppResult;
use crate::application::pontomais_auth_service::PontoMaisStatus;
use crate::application::worklog_timeline_service::{WorkDayTimeline, WorklogTimelineService};
use crate::application::{JiraWorklogSyncService, PontoMaisAuthService};
use crate::domain::date_range::DateRange;
use crate::domain::pontomais::credentials::PontoMaisLogin;

#[tauri::command]
pub async fn pontomais_connect(
    login: String,
    password: Zeroizing<String>,
    auto_reconnect: bool,
    auth_service: State<'_, PontoMaisAuthService>,
) -> AppResult<()> {
    auth_service
        .connect(PontoMaisLogin::new(login, password), auto_reconnect)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn pontomais_disconnect(auth_service: State<'_, PontoMaisAuthService>) -> AppResult<()> {
    auth_service.disconnect().await?;
    Ok(())
}

#[tauri::command]
pub fn pontomais_status(auth_service: State<'_, PontoMaisAuthService>) -> PontoMaisStatus {
    auth_service.status()
}

#[tauri::command]
pub async fn sync_pontomais_entries(
    from_date: String,
    to_date: String,
    sync_service: State<'_, JiraWorklogSyncService>,
) -> AppResult<usize> {
    let range = DateRange::parse(&from_date, &to_date)?;
    sync_service.sync_ponto(&range).await
}

#[tauri::command]
pub async fn get_pontomais_timeline(
    from_date: String,
    to_date: String,
    timeline_service: State<'_, WorklogTimelineService>,
) -> AppResult<Vec<WorkDayTimeline>> {
    let range = DateRange::parse(&from_date, &to_date)?;
    timeline_service.build(&range).await
}
