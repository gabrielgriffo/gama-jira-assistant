use tauri::State;

use crate::application::error::AppResult;
use crate::application::jira_worklog_sync_service::PeriodSyncSummary;
use crate::application::{JiraWorklogService, JiraWorklogSyncService};
use crate::domain::date_range::DateRange;
use crate::domain::jira_worklog::JiraWorklogHistoryResponse;

#[tauri::command]
pub async fn get_jira_worklogs_for_period(
    from_date: String,
    to_date: String,
    service: State<'_, JiraWorklogService>,
) -> AppResult<JiraWorklogHistoryResponse> {
    let range = DateRange::parse(&from_date, &to_date)?;
    service
        .get_worklogs_for_period(&range.from.to_string(), &range.to.to_string())
        .await
}

#[tauri::command]
pub async fn sync_jira_worklogs(
    from_date: String,
    to_date: String,
    force: bool,
    service: State<'_, JiraWorklogSyncService>,
) -> AppResult<Option<usize>> {
    let range = DateRange::parse(&from_date, &to_date)?;
    service.sync_worklogs_if_stale(&range, force).await
}

#[tauri::command]
pub async fn sync_worklog_period(
    from_date: String,
    to_date: String,
    force: bool,
    service: State<'_, JiraWorklogSyncService>,
) -> AppResult<Option<PeriodSyncSummary>> {
    let range = DateRange::parse(&from_date, &to_date)?;
    service.sync_generate_period_if_stale(&range, force).await
}
