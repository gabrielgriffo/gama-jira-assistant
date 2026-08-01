use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use super::error::AppResult;
use crate::domain::jira_worklog::{JiraWorklog, JiraWorklogDay, JiraWorklogHistoryResponse};
use crate::domain::ports::CredentialStore;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::database::{SqliteWorklogRepository, StoredWorklog};

pub struct JiraWorklogService {
    worklog_repo: Arc<SqliteWorklogRepository>,
    credential_service: Arc<CredentialService>,
}

impl JiraWorklogService {
    pub fn new(worklog_repo: SqliteWorklogRepository, credential_service: CredentialService) -> Self {
        Self {
            worklog_repo: Arc::new(worklog_repo),
            credential_service: Arc::new(credential_service),
        }
    }

    pub async fn get_worklogs_for_period(
        &self,
        from_date: &str,
        to_date: &str,
    ) -> AppResult<JiraWorklogHistoryResponse> {
        let user = self.credential_service.get_current_user().await?;
        let stored = self
            .worklog_repo
            .get_synced_for_author(&user.account_id, from_date, to_date)
            .await?;
        let last_synced_at = self.worklog_repo.get_sync_timestamp(&user.account_id).await?;

        log::info!(
            "[JIRA_WORKLOGS] {} worklogs no cache entre {} e {}",
            stored.len(),
            from_date,
            to_date
        );

        Ok(build_response(stored, user.display_name, last_synced_at))
    }
}

impl Clone for JiraWorklogService {
    fn clone(&self) -> Self {
        Self {
            worklog_repo: self.worklog_repo.clone(),
            credential_service: self.credential_service.clone(),
        }
    }
}

fn build_response(
    stored: Vec<StoredWorklog>,
    author_name: String,
    last_synced_at: Option<String>,
) -> JiraWorklogHistoryResponse {
    let mut days_map: BTreeMap<String, Vec<JiraWorklog>> = BTreeMap::new();
    let mut unique_issues: HashSet<String> = HashSet::new();

    for worklog in stored {
        unique_issues.insert(worklog.issue_id.clone());
        days_map
            .entry(worklog.started_local_date.clone())
            .or_default()
            .push(JiraWorklog {
                id: worklog.jira_worklog_id.unwrap_or_else(|| worklog.id.to_string()),
                issue_key: worklog.issue_key,
                issue_summary: worklog.issue_summary,
                issue_browse_url: worklog.issue_browse_url,
                origin: worklog.origin,
                author_account_id: worklog.author_account_id,
                author_display_name: Some(author_name.clone()),
                started: worklog.started_at,
                time_spent_seconds: worklog.time_spent_seconds,
                comment: worklog.comment_text,
                created: worklog.jira_created_at.unwrap_or_default(),
                updated: worklog.jira_updated_at.unwrap_or_default(),
            });
    }

    let days: Vec<JiraWorklogDay> = days_map
        .into_iter()
        .rev()
        .map(|(date, mut worklogs)| {
            worklogs.sort_by(|a, b| a.started.cmp(&b.started));
            let total_seconds = worklogs.iter().map(|w| w.time_spent_seconds).sum();
            JiraWorklogDay {
                date,
                worklogs,
                total_seconds,
            }
        })
        .collect();

    JiraWorklogHistoryResponse {
        total_seconds: days.iter().map(|d| d.total_seconds).sum(),
        total_worklogs: days.iter().map(|d| d.worklogs.len()).sum(),
        unique_issues: unique_issues.len(),
        last_synced_at,
        days,
    }
}
