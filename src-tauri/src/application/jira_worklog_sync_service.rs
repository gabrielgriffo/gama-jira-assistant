use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use serde::Serialize;

use super::changelog_sync::save_issues_with_changelogs;
use super::error::{AppError, AppResult};
use super::pontomais_auth_service::PontoMaisAuthService;
use super::sync_throttle::{changelog_key, period_key, worklogs_key, SyncThrottle};
use crate::domain::date_range::{normalize_jira_timestamp, parse_time_zone, start_of_day_utc, DateRange};
use crate::domain::error::DomainError;
use crate::domain::issue::IssueDetail;
use crate::domain::ports::CredentialStore;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::database::{
    ChangelogRepository, IssueRepository, PontoMaisEntryRepository, SqliteWorklogRepository, StatusRepository,
    SyncedCleanup, SyncedWorklog,
};
use crate::infrastructure::integration::jira::{
    issues_assigned_during_jql, issues_with_my_worklogs_jql, JiraProvider, JiraWorklogDto,
};
use crate::infrastructure::integration::pontomais::PontoMaisProvider;

const WORKLOG_FETCH_CONCURRENCY: usize = 5;

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
        let Some(_guard) = self.throttle.try_begin(key.clone(), force) else {
            return Ok(None);
        };
        let summary = match self.sync_generate_period(range).await {
            Err(AppError::Domain(DomainError::SyncCancelled)) => return Ok(None),
            other => other?,
        };
        self.throttle.mark_synced(&key);
        Ok(Some(summary))
    }

    pub async fn sync_worklogs_if_stale(&self, range: &DateRange, force: bool) -> AppResult<Option<usize>> {
        let key = worklogs_key(range);
        let Some(_guard) = self.throttle.try_begin(key, force) else {
            return Ok(None);
        };
        match self.sync_worklogs(range).await {
            Err(AppError::Domain(DomainError::SyncCancelled)) => Ok(None),
            other => Ok(Some(other?)),
        }
    }

    pub async fn sync_generate_period(&self, range: &DateRange) -> AppResult<PeriodSyncSummary> {
        let ponto = if self.pontomais_auth.is_connected() {
            Some(self.sync_ponto(range).await)
        } else {
            None
        };
        let issues = self.sync_period_issues(range).await?;
        let worklogs = match self.throttle.try_begin(worklogs_key(range), true) {
            Some(_guard) => self.sync_worklogs(range).await?,
            None => {
                self.throttle.wait_until_idle(&worklogs_key(range)).await;
                self.cached_worklog_count(range).await?
            }
        };

        let ponto_entries = match ponto {
            Some(Ok(entries)) => Some(entries),
            Some(Err(err)) => {
                log::error!("[WORKLOG_SYNC] Erro ao importar o ponto: {}", err);
                return Err(err);
            }
            None => None,
        };

        Ok(PeriodSyncSummary {
            ponto_entries,
            issues,
            worklogs,
        })
    }

    async fn cached_worklog_count(&self, range: &DateRange) -> AppResult<usize> {
        let user = self.credential_service.get_current_user().await?;
        let cached = self
            .worklog_repo
            .get_synced_for_author(&user.account_id, &range.from.to_string(), &range.to.to_string())
            .await?;
        Ok(cached.len())
    }

    pub async fn sync_ponto(&self, range: &DateRange) -> AppResult<usize> {
        let generation = self.throttle.generation();
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

        let cards: Vec<_> = work_days.into_iter().flat_map(|day| day.time_cards).collect();
        self.throttle.ensure_generation(generation)?;
        self.pontomais_repo.replace_range(&from, &to, &cards).await?;

        Ok(cards.len())
    }

    pub async fn sync_period_issues(&self, range: &DateRange) -> AppResult<usize> {
        let generation = self.throttle.generation();
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

        let _guards: Vec<_> = issues
            .iter()
            .map(|(issue, _)| self.throttle.begin(changelog_key(&issue.key)))
            .collect();
        self.throttle.ensure_generation(generation)?;
        save_issues_with_changelogs(
            &self.jira_provider,
            &credentials,
            &self.issue_repo,
            &self.changelog_repo,
            &issues,
            || self.throttle.ensure_generation(generation),
        )
        .await?;
        for (issue, _) in &issues {
            self.throttle.mark_synced(&changelog_key(&issue.key));
        }

        Ok(issues.len())
    }

    pub async fn sync_worklogs(&self, range: &DateRange) -> AppResult<usize> {
        let sync_started_at = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let generation = self.throttle.generation();
        let credentials = self.credential_service.get_jira_credentials().await?;
        let user = self.credential_service.get_current_user().await?;
        let tz = parse_time_zone(&user.time_zone);

        let issues = self
            .jira_provider
            .search_issues(&credentials, &issues_with_my_worklogs_jql(range), IssueDetail::Basic)
            .await?;

        self.throttle.ensure_generation(generation)?;
        self.issue_repo
            .save_issues_with_status(&issues, IssueDetail::Basic)
            .await?;

        let started_between = (
            start_of_day_utc(range.from, tz).timestamp_millis(),
            start_of_day_utc(range.end_exclusive(), tz).timestamp_millis(),
        );
        let from = range.from.to_string();
        let to = range.to.to_string();

        let found_ids: HashSet<&str> = issues.iter().map(|(issue, _)| issue.id.as_str()).collect();
        let cached_only: Vec<String> = self
            .worklog_repo
            .issue_ids_with_worklogs(&user.account_id, &from, &to)
            .await?
            .into_iter()
            .filter(|id| !found_ids.contains(id.as_str()))
            .collect();

        let targets: Vec<(String, String)> = issues
            .iter()
            .map(|(issue, _)| (issue.id.clone(), issue.key.clone()))
            .chain(cached_only.iter().map(|id| (id.clone(), id.clone())))
            .collect();
        let fetched: Vec<(String, String, Result<Vec<JiraWorklogDto>, DomainError>)> = stream::iter(targets)
            .map(|(issue_id, label)| {
                let provider = Arc::clone(&self.jira_provider);
                let credentials = credentials.clone();
                async move {
                    let result = provider
                        .get_worklogs_started_between(&credentials, &issue_id, Some(started_between))
                        .await;
                    (issue_id, label, result)
                }
            })
            .buffered(WORKLOG_FETCH_CONCURRENCY)
            .collect()
            .await;

        let cached_only: HashSet<&str> = cached_only.iter().map(String::as_str).collect();
        let mut records: Vec<SyncedWorklog> = Vec::new();
        let mut skipped_issue_ids: Vec<String> = Vec::new();
        for (issue_id, label, result) in fetched {
            match result {
                Ok(worklogs) => records.extend(my_worklogs_in_range(
                    &worklogs,
                    &issue_id,
                    &label,
                    &user.account_id,
                    tz,
                    &from,
                    &to,
                )),
                Err(err) if !cached_only.contains(issue_id.as_str()) => return Err(err.into()),
                Err(DomainError::NotFound(_)) => {
                    log::info!(
                        "[WORKLOG_SYNC] A issue {} não existe mais no Jira; seus worklogs saem do cache",
                        issue_id
                    );
                }
                Err(err) => {
                    log::warn!(
                        "[WORKLOG_SYNC] Não foi possível conferir os worklogs da issue {} fora da busca: {}",
                        issue_id,
                        err
                    );
                    skipped_issue_ids.push(issue_id);
                }
            }
        }

        let keep: Vec<String> = records
            .iter()
            .map(|w| w.jira_worklog_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        self.throttle.ensure_generation(generation)?;
        let saved = self.worklog_repo.upsert_synced(&records).await?;
        let removed = self
            .worklog_repo
            .delete_synced_missing(&SyncedCleanup {
                author_account_id: &user.account_id,
                from_date: &from,
                to_date: &to,
                keep_jira_ids: &keep,
                skip_issue_ids: &skipped_issue_ids,
                updated_before: &sync_started_at,
            })
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

fn my_worklogs_in_range(
    worklogs: &[JiraWorklogDto],
    issue_id: &str,
    issue_label: &str,
    account_id: &str,
    tz: chrono_tz::Tz,
    from: &str,
    to: &str,
) -> Vec<SyncedWorklog> {
    for worklog in worklogs.iter().filter(|w| w.time_spent_seconds <= 0) {
        log::warn!(
            "[WORKLOG_SYNC] Worklog {} da issue {} veio com timeSpentSeconds = {} (autor: {}, started: {})",
            worklog.id,
            issue_label,
            worklog.time_spent_seconds,
            worklog
                .author
                .as_ref()
                .map(|a| a.account_id.as_str())
                .unwrap_or("desconhecido"),
            worklog.started
        );
    }

    worklogs
        .iter()
        .filter(|w| w.author.as_ref().is_some_and(|a| a.account_id == account_id))
        .filter_map(|w| to_synced_worklog(w, issue_id, account_id, tz, "jira"))
        .filter(|w| w.started_local_date.as_str() >= from && w.started_local_date.as_str() <= to)
        .collect()
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
