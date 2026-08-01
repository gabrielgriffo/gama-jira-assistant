use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::error::AppResult;
use super::sync_throttle::{changelog_key, issues_key, SyncGuard, SyncThrottle};
use crate::domain::changelog::ChangelogResponse;
use crate::domain::date_range::{parse_time_zone, DateRange};
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail};
use crate::domain::ports::{CredentialStore, EventPublisher};
use crate::infrastructure::database::{ChangelogRepository, IssueRepository, StatusRepository};
use crate::infrastructure::integration::jira::JiraProvider;
use crate::infrastructure::integration::jira::MAX_PAGES;

#[derive(Default)]
struct IssueSyncState {
    running: bool,
    pending: Option<(DateRange, SyncGuard)>,
}

pub struct JiraService<C, E>
where
    C: CredentialStore + 'static,
    E: EventPublisher + 'static,
{
    provider: Arc<JiraProvider>,
    credentials: Arc<C>,
    issues: Arc<IssueRepository>,
    statuses: Arc<StatusRepository>,
    changelog: Arc<ChangelogRepository>,
    events: Arc<E>,
    issue_sync: Arc<Mutex<IssueSyncState>>,
    syncing_changelog: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    throttle: Arc<SyncThrottle>,
}

impl<C, E> JiraService<C, E>
where
    C: CredentialStore + 'static,
    E: EventPublisher + 'static,
{
    pub fn new(
        provider: JiraProvider,
        credentials: C,
        issues: IssueRepository,
        statuses: StatusRepository,
        changelog: ChangelogRepository,
        events: E,
        throttle: Arc<SyncThrottle>,
    ) -> Self {
        Self {
            provider: Arc::new(provider),
            credentials: Arc::new(credentials),
            issues: Arc::new(issues),
            statuses: Arc::new(statuses),
            changelog: Arc::new(changelog),
            events: Arc::new(events),
            issue_sync: Arc::new(Mutex::new(IssueSyncState::default())),
            syncing_changelog: Arc::new(Mutex::new(HashMap::new())),
            throttle,
        }
    }

    pub async fn get_cached_issues(&self, range: &DateRange) -> AppResult<Vec<Issue>> {
        let user = self.credentials.get_current_user().await?;
        let (from_utc, to_utc) = range.utc_bounds(parse_time_zone(&user.time_zone));
        Ok(self
            .issues
            .get_for_user_updated_between(&user.account_id, &from_utc, &to_utc)
            .await?)
    }

    fn try_start_sync(&self, range: DateRange) -> Option<SyncGuard> {
        let guard = self.throttle.begin(issues_key(&range));
        let mut state = self.issue_sync.lock().unwrap_or_else(|e| e.into_inner());
        if state.running {
            state.pending = Some((range, guard));
            return None;
        }
        state.running = true;
        Some(guard)
    }

    fn next_pending_sync(&self) -> Option<(DateRange, SyncGuard)> {
        let mut state = self.issue_sync.lock().unwrap_or_else(|e| e.into_inner());
        let next = state.pending.take();
        if next.is_none() {
            state.running = false;
        }
        next
    }

    fn try_start_changelog_sync(&self, issue_key: &str) -> bool {
        let mut map = self.syncing_changelog.lock().unwrap_or_else(|e| e.into_inner());
        let syncing = map
            .entry(issue_key.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)));
        syncing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

async fn perform_sync_and_get_issues<C: CredentialStore, E: EventPublisher>(
    provider: &JiraProvider,
    credentials: &C,
    issues: &IssueRepository,
    statuses: &StatusRepository,
    events: &E,
    range: &DateRange,
) -> Result<(Vec<Issue>, Vec<Issue>), DomainError> {
    let creds = credentials.get_jira_credentials().await?;
    let user = credentials.get_current_user().await?;
    let tz = parse_time_zone(&user.time_zone);
    let cached = issues.get_all_for_user(&user.account_id).await?;
    log::info!("[SYNC] Cache possui {} issues antes de salvar", cached.len());
    let development_status_ids = statuses.get_development_ids().await?;

    let mut fresh_issues: Vec<Issue> = Vec::new();
    let mut page_token: Option<String> = None;
    let mut same_token_retry_count = 0;

    let mut pages = 0;
    loop {
        pages += 1;
        if pages > MAX_PAGES {
            log::warn!(
                "[JIRA] limite de {} páginas atingido, interrompendo paginação",
                MAX_PAGES
            );
            break;
        }
        let (page, next_token) = provider
            .search_my_issues_page(&creds, range, &development_status_ids, page_token.clone())
            .await?;

        for (issue, status) in &page {
            issues.save_issue_with_status(issue, status, IssueDetail::Full).await?;
        }

        if !page.is_empty() {
            events.tasks_updated();
        }

        fresh_issues.extend(page.into_iter().map(|(issue, _)| issue));

        if next_token.is_none() {
            break;
        }

        if next_token == page_token {
            if same_token_retry_count == 0 {
                log::info!("[SEARCH] API retornou o mesmo page_token, aguardando 3s antes de tentar novamente");
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                same_token_retry_count += 1;
                continue;
            }
            log::warn!("[SEARCH] API retornou o mesmo page_token após retry, parando paginação");
            break;
        }

        same_token_retry_count = 0;
        page_token = next_token;
    }

    log::info!(
        "[SYNC] Jira retornou {} issues frescas entre {} e {}",
        fresh_issues.len(),
        range.from,
        range.to
    );

    if range.reaches_today(tz) {
        let fresh_ids: Vec<String> = fresh_issues.iter().map(|i| i.id.clone()).collect();
        let (from_utc, to_utc) = range.utc_bounds(tz);
        issues
            .reconcile_assigned_issues_updated_between(&user.account_id, &fresh_ids, &from_utc, &to_utc)
            .await?;
    }

    Ok((fresh_issues, cached))
}

impl<C, E> Clone for JiraService<C, E>
where
    C: CredentialStore + 'static,
    E: EventPublisher + 'static,
{
    fn clone(&self) -> Self {
        Self {
            provider: Arc::clone(&self.provider),
            credentials: Arc::clone(&self.credentials),
            issues: Arc::clone(&self.issues),
            statuses: Arc::clone(&self.statuses),
            changelog: Arc::clone(&self.changelog),
            events: Arc::clone(&self.events),
            issue_sync: Arc::clone(&self.issue_sync),
            syncing_changelog: Arc::clone(&self.syncing_changelog),
            throttle: Arc::clone(&self.throttle),
        }
    }
}

impl<C, E> JiraService<C, E>
where
    C: CredentialStore + 'static,
    E: EventPublisher + 'static,
{
    pub async fn list_my_issues_with_background_changelog_sync(
        &self,
        range: DateRange,
        force: bool,
    ) -> AppResult<Vec<Issue>> {
        let cached = self.get_cached_issues(&range).await?;

        if !self.throttle.should_sync(&issues_key(&range), force) {
            return Ok(cached);
        }

        if let Some(guard) = self.try_start_sync(range) {
            let service_clone = self.clone();

            tauri::async_runtime::spawn(async move {
                let mut current = range;
                let mut current_guard = guard;

                loop {
                    let result = perform_sync_and_get_issues(
                        &*service_clone.provider,
                        &*service_clone.credentials,
                        &*service_clone.issues,
                        &*service_clone.statuses,
                        &*service_clone.events,
                        &current,
                    )
                    .await;

                    match result {
                        Ok((fresh_issues, cached_issues)) => {
                            service_clone.throttle.mark_synced(&issues_key(&current));
                            service_clone.events.tasks_synced();
                            drop(current_guard);
                            Arc::new(service_clone.clone())
                                .trigger_changelog_sync_in_background(fresh_issues, cached_issues)
                                .await;
                        }
                        Err(err) => {
                            log::error!("[SYNC] Erro ao sincronizar issues: {}", err);
                            service_clone.events.tasks_sync_failed(&err.to_string());
                            drop(current_guard);
                        }
                    }

                    match service_clone.next_pending_sync() {
                        Some((next, next_guard)) => {
                            current = next;
                            current_guard = next_guard;
                        }
                        None => break,
                    }
                }
            });
        }

        Ok(cached)
    }

    pub async fn sync_status_catalog(&self) -> AppResult<()> {
        let creds = self.credentials.get_jira_credentials().await?;
        let status_dtos = self.provider.get_all_statuses(&creds).await?;
        let statuses = crate::infrastructure::integration::jira::mapper::map_statuses(&status_dtos);
        self.statuses.sync_catalog(&statuses).await?;
        Ok(())
    }

    pub async fn get_status_history(&self, issue_key: &str) -> AppResult<ChangelogResponse> {
        let issue_id = self.issues.get_id_by_key(issue_key).await?;
        let cached = self.changelog.get_changelog(&issue_id).await?;

        if !self.throttle.should_sync(&changelog_key(issue_key), false) {
            return Ok(cached);
        }

        if self.try_start_changelog_sync(issue_key) {
            let provider = Arc::clone(&self.provider);
            let credentials = Arc::clone(&self.credentials);
            let issues = Arc::clone(&self.issues);
            let changelog = Arc::clone(&self.changelog);
            let events = Arc::clone(&self.events);
            let syncing_map = Arc::clone(&self.syncing_changelog);
            let throttle = Arc::clone(&self.throttle);
            let issue_key_owned = issue_key.to_string();
            let guard = self.throttle.begin(changelog_key(issue_key));

            tauri::async_runtime::spawn(async move {
                let _guard = guard;
                sync_changelog_and_report(
                    &*provider,
                    &*credentials,
                    &*issues,
                    &*changelog,
                    &*events,
                    &throttle,
                    &issue_key_owned,
                )
                .await;

                if let Some(syncing) = syncing_map
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&issue_key_owned)
                {
                    syncing.store(false, Ordering::SeqCst);
                }
            });
        }

        Ok(cached)
    }

    pub async fn get_cached_status_history(&self, issue_key: &str) -> AppResult<ChangelogResponse> {
        let issue_id = self.issues.get_id_by_key(issue_key).await?;
        Ok(self.changelog.get_changelog(&issue_id).await?)
    }

    async fn detect_changed_issues(&self, fresh_issues: &[Issue], cached_issues: &[Issue]) -> Vec<String> {
        let mut changed = Vec::new();

        log::info!(
            "[SYNC] Detectando mudanças: {} issues frescas vs {} cacheadas",
            fresh_issues.len(),
            cached_issues.len()
        );

        for issue in fresh_issues {
            let cached = cached_issues.iter().find(|i| i.key == issue.key);

            let has_changed = match cached {
                None => {
                    log::info!("[SYNC] {} é nova (não estava no cache)", issue.key);
                    true
                }
                Some(cached_issue) => {
                    let status_changed = issue.status_id != cached_issue.status_id;

                    let has_changelog = match self.issues.get_id_by_key(&issue.key).await {
                        Ok(issue_id) => match self.changelog.get_changelog(&issue_id).await {
                            Ok(response) => !response.entries.is_empty(),
                            _ => false,
                        },
                        _ => false,
                    };

                    let should_sync = status_changed || !has_changelog;

                    if should_sync {
                        if status_changed {
                            log::info!(
                                "[SYNC] {} mudou status: {} -> {}",
                                issue.key,
                                cached_issue.status_name,
                                issue.status_name
                            );
                        }
                        if !has_changelog {
                            log::info!("[SYNC] {} não possui histórico de changelog no cache", issue.key);
                        }
                    }
                    should_sync
                }
            };

            if has_changed {
                changed.push(issue.key.clone());
            }
        }

        log::info!("[SYNC] Total de issues com mudanças: {}", changed.len());
        changed
    }

    async fn sync_changelog_incremental(&self, issue_key: &str) -> Result<(), DomainError> {
        let credentials = self.credentials.get_jira_credentials().await?;
        let issue_id = self.issues.get_id_by_key(issue_key).await?;

        let new_histories = self
            .provider
            .get_changelog_incremental(&credentials, issue_key, &*self.changelog, &issue_id)
            .await?;

        if new_histories.is_empty() {
            return Ok(());
        }

        let mut all_histories = new_histories;
        all_histories.reverse();

        let changes = crate::infrastructure::integration::jira::mapper::map_changelogs(all_histories);

        self.changelog.save_changes(&issue_id, &changes).await?;
        Ok(())
    }

    async fn trigger_changelog_sync_in_background(
        self: Arc<Self>,
        fresh_issues: Vec<Issue>,
        cached_issues: Vec<Issue>,
    ) {
        let changed_keys = self.detect_changed_issues(&fresh_issues, &cached_issues).await;

        if changed_keys.is_empty() {
            return;
        }

        let semaphore = Arc::new(tokio::sync::Semaphore::new(5));

        for issue_key in changed_keys {
            let sem = Arc::clone(&semaphore);
            let service = Arc::clone(&self);
            let guard = self.throttle.begin(changelog_key(&issue_key));

            tauri::async_runtime::spawn(async move {
                let _guard = guard;
                let Ok(_permit) = sem.acquire().await else {
                    return;
                };

                if let Err(e) = service.sync_changelog_incremental(&issue_key).await {
                    log::error!("[CHANGELOG] Erro ao sincronizar changelog de {}: {}", issue_key, e);
                    service.events.status_history_sync_failed(&issue_key, &e.to_string());
                } else {
                    service.throttle.mark_synced(&changelog_key(&issue_key));
                    service.events.status_history_synced(&issue_key);
                }
            });
        }
    }
}

async fn sync_changelog_and_report<C, E>(
    provider: &JiraProvider,
    credentials: &C,
    issues: &IssueRepository,
    changelog: &ChangelogRepository,
    events: &E,
    throttle: &SyncThrottle,
    issue_key: &str,
) where
    C: CredentialStore,
    E: EventPublisher,
{
    match perform_changelog_sync(provider, credentials, issues, changelog, issue_key).await {
        Ok(()) => {
            throttle.mark_synced(&changelog_key(issue_key));
            events.status_history_synced(issue_key)
        }
        Err(err) => events.status_history_sync_failed(issue_key, &err.to_string()),
    }
}

async fn perform_changelog_sync<C: CredentialStore>(
    provider: &JiraProvider,
    credentials: &C,
    issues: &IssueRepository,
    changelog: &ChangelogRepository,
    issue_key: &str,
) -> Result<(), DomainError> {
    let creds = credentials.get_jira_credentials().await?;
    let issue_id = issues.get_id_by_key(issue_key).await?;
    let fresh = provider.get_changelog(&creds, issue_key).await?;
    let changes = crate::infrastructure::integration::jira::mapper::map_changelogs(fresh);
    changelog.save_changes(&issue_id, &changes).await?;
    Ok(())
}
