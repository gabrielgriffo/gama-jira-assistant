use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::error::AppResult;
use super::pontomais_auth_service::PontoMaisAuthService;
use super::sync_throttle::{changelog_key, period_key, worklogs_key, SyncThrottle};
use crate::domain::date_range::{normalize_jira_timestamp, parse_time_zone, start_of_day_utc, DateRange};
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail};
use crate::domain::ports::CredentialStore;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::database::{
    ChangelogRepository, IssueRepository, PontoMaisEntryRepository, SqliteWorklogRepository, StatusRepository,
    SyncedWorklog,
};
use crate::infrastructure::integration::jira::{
    issues_assigned_during_jql, issues_with_my_worklogs_jql, mapper, JiraProvider, JiraWorklogDto,
};
use crate::infrastructure::integration::pontomais::PontoMaisProvider;

const CHANGELOG_CONCURRENCY: usize = 5;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodSyncSummary {
    pub ponto_entries: Option<usize>,
    pub issues: usize,
    pub worklogs: usize,
}

pub struct JiraWorklogSyncService {
    pontomais_repo: Arc<PontoMaisEntryRepository>,
    pontomais_provider: Arc<PontoMaisProvider>,
    pontomais_auth: PontoMaisAuthService,
    changelog_repo: Arc<ChangelogRepository>,
    issue_repo: Arc<IssueRepository>,
    status_repo: Arc<StatusRepository>,
    worklog_repo: Arc<SqliteWorklogRepository>,
    jira_provider: Arc<JiraProvider>,
    credential_service: Arc<CredentialService>,
    throttle: Arc<SyncThrottle>,
}

impl JiraWorklogSyncService {
    pub fn new(
        pontomais_repo: PontoMaisEntryRepository,
        pontomais_provider: PontoMaisProvider,
        pontomais_auth: PontoMaisAuthService,
        changelog_repo: ChangelogRepository,
        issue_repo: IssueRepository,
        status_repo: StatusRepository,
        worklog_repo: SqliteWorklogRepository,
        jira_provider: JiraProvider,
        credential_service: CredentialService,
        throttle: Arc<SyncThrottle>,
    ) -> Self {
        Self {
            pontomais_repo: Arc::new(pontomais_repo),
            pontomais_provider: Arc::new(pontomais_provider),
            pontomais_auth,
            changelog_repo: Arc::new(changelog_repo),
            issue_repo: Arc::new(issue_repo),
            status_repo: Arc::new(status_repo),
            worklog_repo: Arc::new(worklog_repo),
            jira_provider: Arc::new(jira_provider),
            credential_service: Arc::new(credential_service),
            throttle,
        }
    }

    pub async fn sync_generate_period_if_stale(
        &self,
        range: &DateRange,
        force: bool,
    ) -> AppResult<Option<PeriodSyncSummary>> {
        let key = period_key(range);
        if !self.throttle.should_sync(&key, force) {
            return Ok(None);
        }
        let _guard = self.throttle.begin(key.clone());
        let summary = self.sync_generate_period(range).await?;
        self.throttle.mark_synced(&key);
        Ok(Some(summary))
    }

    pub async fn sync_worklogs_if_stale(&self, range: &DateRange, force: bool) -> AppResult<Option<usize>> {
        let key = worklogs_key(range);
        if !self.throttle.should_sync(&key, force) {
            return Ok(None);
        }
        let _guard = self.throttle.begin(key);
        Ok(Some(self.sync_worklogs(range).await?))
    }

    pub async fn sync_generate_period(&self, range: &DateRange) -> AppResult<PeriodSyncSummary> {
        let ponto_entries = if self.pontomais_auth.is_connected() {
            Some(self.sync_ponto(range).await?)
        } else {
            None
        };
        let issues = self.sync_period_issues(range).await?;
        let worklogs = self.sync_worklogs(range).await?;

        Ok(PeriodSyncSummary {
            ponto_entries,
            issues,
            worklogs,
        })
    }

    pub async fn sync_ponto(&self, range: &DateRange) -> AppResult<usize> {
        let credentials = self.pontomais_auth.credentials()?;
        let from = range.from.to_string();
        let to = range.to.to_string();

        let work_days = match self.pontomais_provider.get_work_days(&credentials, &from, &to).await {
            Err(DomainError::SessionExpired) => {
                let renewed = self.pontomais_auth.reconnect(&credentials).await?;
                self.pontomais_provider.get_work_days(&renewed, &from, &to).await?
            }
            other => other?,
        };

        let mut total_entries = 0;
        for work_day in work_days {
            self.pontomais_repo
                .save_for_date(&work_day.date, &work_day.time_cards)
                .await?;
            total_entries += work_day.time_cards.len();
        }

        Ok(total_entries)
    }

    pub async fn sync_period_issues(&self, range: &DateRange) -> AppResult<usize> {
        let credentials = self.credential_service.get_jira_credentials().await?;
        let development_status_ids = self.status_repo.get_development_ids().await?;
        let issues = self
            .jira_provider
            .search_issues(
                &credentials,
                &issues_assigned_during_jql(range, &development_status_ids),
                IssueDetail::Basic,
            )
            .await?;

        log::info!(
            "[WORKLOG_SYNC] {} issues estiveram com o usuário entre {} e {}",
            issues.len(),
            range.from,
            range.to
        );

        for (issue, status) in &issues {
            self.issue_repo
                .save_issue_with_status(issue, status, IssueDetail::Basic)
                .await?;
        }

        let fresh: Vec<Issue> = issues.into_iter().map(|(issue, _)| issue).collect();
        self.sync_changelogs(&fresh).await;

        Ok(fresh.len())
    }

    async fn sync_changelogs(&self, issues: &[Issue]) {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(CHANGELOG_CONCURRENCY));
        let mut handles = Vec::with_capacity(issues.len());

        for issue in issues {
            let semaphore = Arc::clone(&semaphore);
            let provider = Arc::clone(&self.jira_provider);
            let credentials = Arc::clone(&self.credential_service);
            let changelog = Arc::clone(&self.changelog_repo);
            let throttle = Arc::clone(&self.throttle);
            let issue_id = issue.id.clone();
            let issue_key = issue.key.clone();
            let guard = self.throttle.begin(changelog_key(&issue_key));

            handles.push(tauri::async_runtime::spawn(async move {
                let _guard = guard;
                let _permit = semaphore.acquire().await;
                match sync_changelog(&provider, &*credentials, &changelog, &issue_id, &issue_key).await {
                    Ok(()) => throttle.mark_synced(&changelog_key(&issue_key)),
                    Err(err) => {
                        log::error!("[WORKLOG_SYNC] Erro ao sincronizar changelog de {}: {}", issue_key, err)
                    }
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    pub async fn sync_worklogs(&self, range: &DateRange) -> AppResult<usize> {
        let credentials = self.credential_service.get_jira_credentials().await?;
        let user = self.credential_service.get_current_user().await?;
        let tz = parse_time_zone(&user.time_zone);

        let issues = self
            .jira_provider
            .search_issues(&credentials, &issues_with_my_worklogs_jql(range), IssueDetail::Basic)
            .await?;

        for (issue, status) in &issues {
            self.issue_repo
                .save_issue_with_status(issue, status, IssueDetail::Basic)
                .await?;
        }

        let started_after = start_of_day_utc(range.from, tz).timestamp_millis();
        let started_before = start_of_day_utc(range.end_exclusive(), tz).timestamp_millis();
        let from = range.from.to_string();
        let to = range.to.to_string();

        let mut records: Vec<SyncedWorklog> = Vec::new();
        for (issue, _) in &issues {
            let worklogs = self
                .jira_provider
                .get_worklogs_started_between(&credentials, &issue.id, Some((started_after, started_before)))
                .await?;

            for worklog in worklogs.iter().filter(|w| w.time_spent_seconds <= 0) {
                log::warn!(
                    "[WORKLOG_SYNC] Worklog {} da issue {} veio com timeSpentSeconds = {} (autor: {}, started: {})",
                    worklog.id,
                    issue.key,
                    worklog.time_spent_seconds,
                    worklog
                        .author
                        .as_ref()
                        .map(|a| a.account_id.as_str())
                        .unwrap_or("desconhecido"),
                    worklog.started
                );
            }

            records.extend(
                worklogs
                    .iter()
                    .filter(|w| {
                        w.author
                            .as_ref()
                            .map(|a| a.account_id == user.account_id)
                            .unwrap_or(false)
                    })
                    .filter_map(|w| to_synced_worklog(w, &issue.id, &user.account_id, tz, "jira"))
                    .filter(|w| w.started_local_date >= from && w.started_local_date <= to),
            );
        }

        let keep: Vec<String> = records
            .iter()
            .map(|w| w.jira_worklog_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let saved = self.worklog_repo.upsert_synced(&records).await?;
        let removed = self
            .worklog_repo
            .delete_synced_missing(&user.account_id, &from, &to, &keep)
            .await?;

        log::info!(
            "[WORKLOG_SYNC] {} worklogs gravados e {} removidos entre {} e {}",
            saved,
            removed,
            from,
            to
        );

        self.throttle.mark_synced(&worklogs_key(range));
        Ok(saved)
    }
}

async fn sync_changelog(
    provider: &JiraProvider,
    credentials: &CredentialService,
    changelog: &ChangelogRepository,
    issue_id: &str,
    issue_key: &str,
) -> Result<(), DomainError> {
    let creds = credentials.get_jira_credentials().await?;
    let mut histories = provider
        .get_changelog_incremental(&creds, issue_key, changelog, issue_id)
        .await?;
    if histories.is_empty() {
        return Ok(());
    }
    histories.reverse();
    let changes = mapper::map_changelogs(histories);
    changelog.save_changes(issue_id, &changes).await
}

pub fn to_synced_worklog(
    dto: &JiraWorklogDto,
    issue_id: &str,
    author_account_id: &str,
    tz: chrono_tz::Tz,
    origin: &'static str,
) -> Option<SyncedWorklog> {
    let started = DateTime::parse_from_str(&dto.started, "%Y-%m-%dT%H:%M:%S%.f%z")
        .or_else(|_| DateTime::parse_from_rfc3339(&dto.started))
        .ok()?
        .with_timezone(&Utc);

    Some(SyncedWorklog {
        jira_worklog_id: dto.id.clone(),
        issue_id: dto.issue_id.clone().unwrap_or_else(|| issue_id.to_string()),
        author_account_id: author_account_id.to_string(),
        started_at: normalize_jira_timestamp(&dto.started),
        started_local_date: started.with_timezone(&tz).date_naive().to_string(),
        time_spent_seconds: dto.time_spent_seconds,
        comment_text: dto.comment_text(),
        origin,
        jira_created_at: dto.created.as_deref().map(normalize_jira_timestamp),
        jira_updated_at: dto.updated.as_deref().map(normalize_jira_timestamp),
    })
}

impl Clone for JiraWorklogSyncService {
    fn clone(&self) -> Self {
        Self {
            pontomais_repo: self.pontomais_repo.clone(),
            pontomais_provider: self.pontomais_provider.clone(),
            pontomais_auth: self.pontomais_auth.clone(),
            changelog_repo: self.changelog_repo.clone(),
            issue_repo: self.issue_repo.clone(),
            status_repo: self.status_repo.clone(),
            worklog_repo: self.worklog_repo.clone(),
            jira_provider: self.jira_provider.clone(),
            credential_service: self.credential_service.clone(),
            throttle: self.throttle.clone(),
        }
    }
}
