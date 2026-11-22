use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::changelog_sync::sync_changelogs_in_bulk;
use super::error::AppResult;
use super::sync_throttle::{changelog_key, issues_key, SyncGuard, SyncThrottle};
use crate::domain::changelog::ChangelogResponse;
use crate::domain::date_range::{format_utc, parse_time_zone, start_of_day_utc, DateRange};
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail};
use crate::domain::ports::{CredentialStore, EventPublisher};
use crate::infrastructure::database::{ChangelogRepository, IssueRepository, StatusRepository};
use crate::infrastructure::integration::jira::JiraProvider;
use crate::infrastructure::integration::jira::{incomplete_pagination, MAX_PAGES};

#[derive(Default)]
struct IssueSyncState {
    running: bool,
    pending: Option<(DateRange, SyncGuard)>,
}

const REFRESH_CHUNK: usize = 100;

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

#[allow(clippy::too_many_arguments)]
async fn perform_sync_and_get_issues<C: CredentialStore, E: EventPublisher>(
    provider: &JiraProvider,
    credentials: &C,
    issues: &IssueRepository,
    statuses: &StatusRepository,
    events: &E,
    throttle: &SyncThrottle,
    generation: u64,
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
            log::warn!("[JIRA] limite de {} páginas atingido na busca de issues", MAX_PAGES);
            return Err(incomplete_pagination());
        }
        let (page, next_token) = provider
            .search_my_issues_page(&creds, range, &development_status_ids, page_token.clone())
            .await?;

        throttle.ensure_generation(generation)?;
        issues.save_issues_with_status(&page, IssueDetail::Full).await?;

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
            log::warn!("[SEARCH] API retornou o mesmo page_token após retry");
            return Err(incomplete_pagination());
        }

        same_token_retry_count = 0;
        page_token = next_token;
    }

    let mut seen = HashSet::new();
    fresh_issues.reverse();
    fresh_issues.retain(|issue| seen.insert(issue.id.clone()));
    fresh_issues.reverse();

    log::info!(
        "[SYNC] Jira retornou {} issues frescas entre {} e {}",
        fresh_issues.len(),
        range.from,
        range.to
    );

    if range.reaches_today(tz) {
        refresh_assigned_issues_outside_search(
            provider,
            &creds,
            issues,
            throttle,
            generation,
            &user.account_id,
            &fresh_issues,
            &worth_rechecking(&cached, &format_utc(start_of_day_utc(range.from, tz))),
        )
        .await?;
    }

    Ok((fresh_issues, cached))
}

#[allow(clippy::too_many_arguments)]
async fn refresh_assigned_issues_outside_search(
    provider: &JiraProvider,
    creds: &crate::domain::credentials::JiraCredentials,
    issues: &IssueRepository,
    throttle: &SyncThrottle,
    generation: u64,
    account_id: &str,
    fresh_issues: &[Issue],
    cached: &[Issue],
) -> Result<(), DomainError> {
    let missing = assigned_ids_outside_search(fresh_issues, cached);
    if missing.is_empty() {
        return Ok(());
    }

    let mut confirmed: HashSet<String> = HashSet::new();
    let mut unverified: HashSet<String> = HashSet::new();
    for chunk in missing.chunks(REFRESH_CHUNK) {
        let jql = format!("id in ({})", chunk.join(","));
        match provider.search_issues(creds, &jql, IssueDetail::Basic).await {
            Ok(found) => {
                throttle.ensure_generation(generation)?;
                issues.save_issues_with_status(&found, IssueDetail::Basic).await?;
                confirmed.extend(found.into_iter().map(|(issue, _)| issue.id));
            }
            Err(err @ (DomainError::InvalidInput(_) | DomainError::NotFound(_))) => {
                log::warn!(
                    "[SYNC] O Jira recusou a conferência em lote ({}); conferindo as issues uma a uma",
                    err
                );
                for id in chunk {
                    match provider
                        .search_issues(creds, &format!("id = {id}"), IssueDetail::Basic)
                        .await
                    {
                        Ok(found) => {
                            throttle.ensure_generation(generation)?;
                            issues.save_issues_with_status(&found, IssueDetail::Basic).await?;
                            confirmed.extend(found.into_iter().map(|(issue, _)| issue.id));
                        }
                        Err(DomainError::InvalidInput(_) | DomainError::NotFound(_)) => {}
                        Err(_) => {
                            unverified.insert(id.clone());
                        }
                    }
                }
            }
            Err(err) => {
                log::warn!(
                    "[SYNC] Não foi possível conferir o responsável de issues fora da busca: {}",
                    err
                );
                unverified.extend(chunk.iter().cloned());
            }
        }
    }

    let gone: Vec<String> = missing
        .into_iter()
        .filter(|id| !confirmed.contains(id) && !unverified.contains(id))
        .collect();
    throttle.ensure_generation(generation)?;
    issues.clear_assignee(account_id, &gone).await
}

fn worth_rechecking(cached: &[Issue], period_start_utc: &str) -> Vec<Issue> {
    cached
        .iter()
        .filter(|issue| issue.status_is_development || issue.jira_updated_at.as_str() >= period_start_utc)
        .cloned()
        .collect()
}

fn assigned_ids_outside_search(fresh_issues: &[Issue], cached: &[Issue]) -> Vec<String> {
    let fresh: HashSet<&str> = fresh_issues.iter().map(|issue| issue.id.as_str()).collect();
    cached
        .iter()
        .map(|issue| issue.id.as_str())
        .filter(|id| !fresh.contains(id) && !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .collect()
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
                    let generation = service_clone.throttle.generation();
                    let result = perform_sync_and_get_issues(
                        &service_clone.provider,
                        &*service_clone.credentials,
                        &service_clone.issues,
                        &service_clone.statuses,
                        &*service_clone.events,
                        &service_clone.throttle,
                        generation,
                        &current,
                    )
                    .await;

                    match result {
                        Ok((fresh_issues, cached_issues)) => {
                            service_clone.throttle.mark_synced(&issues_key(&current), generation);
                            service_clone.events.tasks_synced();
                            drop(current_guard);
                            Arc::new(service_clone.clone())
                                .trigger_changelog_sync_in_background(generation, fresh_issues, cached_issues)
                                .await;
                        }
                        Err(DomainError::SyncCancelled) => {
                            log::info!("[SYNC] Sincronização de issues descartada após troca de conta ou limpeza");
                            drop(current_guard);
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

    pub async fn get_status_history(&self, issue_key: &str, force: bool) -> AppResult<ChangelogResponse> {
        let issue_id = self.issues.get_id_by_key(issue_key).await?;
        let cached = self.changelog.get_changelog(&issue_id).await?;

        if !self.throttle.should_sync(&changelog_key(issue_key), force) {
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
                    &provider,
                    &*credentials,
                    &issues,
                    &changelog,
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

    async fn detect_changed_issues(&self, fresh_issues: &[Issue], cached_issues: &[Issue]) -> Vec<(String, String)> {
        log::info!(
            "[SYNC] Detectando mudanças: {} issues frescas vs {} cacheadas",
            fresh_issues.len(),
            cached_issues.len()
        );

        let ids: Vec<String> = fresh_issues.iter().map(|issue| issue.id.clone()).collect();
        let with_history = self.changelog.issue_ids_with_changes(&ids).await.unwrap_or_else(|e| {
            log::error!("[SYNC] Erro ao verificar históricos no cache: {}", e);
            HashSet::new()
        });

        let changed = changed_issues(fresh_issues, cached_issues, &with_history);
        log::info!("[SYNC] Total de issues com mudanças: {}", changed.len());
        changed
    }

    async fn trigger_changelog_sync_in_background(
        self: Arc<Self>,
        generation: u64,
        fresh_issues: Vec<Issue>,
        cached_issues: Vec<Issue>,
    ) {
        let changed = self.detect_changed_issues(&fresh_issues, &cached_issues).await;

        if changed.is_empty() || self.throttle.ensure_generation(generation).is_err() {
            return;
        }

        let credentials = match self.credentials.get_jira_credentials().await {
            Ok(credentials) => credentials,
            Err(e) => {
                log::error!("[CHANGELOG] Erro ao ler credenciais do Jira: {}", e);
                for (issue_key, _) in &changed {
                    self.events.status_history_sync_failed(issue_key, &e.to_string());
                }
                return;
            }
        };

        let guards: Vec<_> = changed
            .iter()
            .map(|(issue_key, _)| self.throttle.begin(changelog_key(issue_key)))
            .collect();
        let service = Arc::clone(&self);

        tauri::async_runtime::spawn(async move {
            let _guards = guards;
            let ids: Vec<String> = changed.iter().map(|(_, issue_id)| issue_id.clone()).collect();
            let result = sync_changelogs_in_bulk(&service.provider, &credentials, &service.changelog, &ids, || {
                service.throttle.ensure_generation(generation)
            })
            .await;
            match result {
                Ok(()) => {
                    for (issue_key, _) in &changed {
                        service.throttle.mark_synced(&changelog_key(issue_key), generation);
                        service.events.status_history_synced(issue_key);
                    }
                }
                Err(DomainError::SyncCancelled) => {
                    log::info!("[CHANGELOG] Sincronização de changelogs descartada após troca de conta ou limpeza");
                }
                Err(e) => {
                    log::error!("[CHANGELOG] Erro ao sincronizar changelogs: {}", e);
                    for (issue_key, _) in &changed {
                        service.events.status_history_sync_failed(issue_key, &e.to_string());
                    }
                }
            }
        });
    }
}

fn changed_issues(
    fresh_issues: &[Issue],
    cached_issues: &[Issue],
    with_history: &HashSet<String>,
) -> Vec<(String, String)> {
    let cached_by_key: HashMap<&str, &Issue> = cached_issues.iter().map(|issue| (issue.key.as_str(), issue)).collect();
    let mut changed = Vec::new();

    for issue in fresh_issues {
        let has_changed = match cached_by_key.get(issue.key.as_str()) {
            None => {
                log::info!("[SYNC] {} é nova (não estava no cache)", issue.key);
                true
            }
            Some(cached_issue) => {
                let status_changed = issue.status_id != cached_issue.status_id;
                let assignee_changed = issue.assignee_account_id != cached_issue.assignee_account_id;
                let has_changelog = with_history.contains(&issue.id);

                if status_changed {
                    log::info!(
                        "[SYNC] {} mudou status: {} -> {}",
                        issue.key,
                        cached_issue.status_name,
                        issue.status_name
                    );
                }
                if assignee_changed {
                    log::info!("[SYNC] {} mudou de responsável", issue.key);
                }
                if !has_changelog {
                    log::info!("[SYNC] {} não possui histórico de changelog no cache", issue.key);
                }
                status_changed || assignee_changed || !has_changelog
            }
        };

        if has_changed {
            changed.push((issue.key.clone(), issue.id.clone()));
        }
    }

    changed
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
    let generation = throttle.generation();
    match perform_changelog_sync(
        provider,
        credentials,
        issues,
        changelog,
        throttle,
        generation,
        issue_key,
    )
    .await
    {
        Ok(()) => {
            throttle.mark_synced(&changelog_key(issue_key), generation);
            events.status_history_synced(issue_key)
        }
        Err(DomainError::SyncCancelled) => {}
        Err(err) => events.status_history_sync_failed(issue_key, &err.to_string()),
    }
}

async fn perform_changelog_sync<C: CredentialStore>(
    provider: &JiraProvider,
    credentials: &C,
    issues: &IssueRepository,
    changelog: &ChangelogRepository,
    throttle: &SyncThrottle,
    generation: u64,
    issue_key: &str,
) -> Result<(), DomainError> {
    let creds = credentials.get_jira_credentials().await?;
    let issue_id = issues.get_id_by_key(issue_key).await?;
    let fresh = provider.get_changelog(&creds, issue_key).await?;
    let changes = crate::infrastructure::integration::jira::mapper::map_changelogs(fresh);
    throttle.ensure_generation(generation)?;
    changelog.save_changes(&issue_id, &changes).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::credentials::JiraCredentials;
    use crate::domain::issue::Status;
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use crate::infrastructure::integration::jira::JiraClient;
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct SilentPublisher;

    impl EventPublisher for SilentPublisher {
        fn tasks_updated(&self) {}
        fn tasks_synced(&self) {}
        fn tasks_sync_failed(&self, _reason: &str) {}
        fn status_history_synced(&self, _issue_key: &str) {}
        fn status_history_sync_failed(&self, _issue_key: &str, _reason: &str) {}
        fn sync_started(&self, _key: &str) {}
        fn sync_finished(&self, _key: &str) {}
    }

    fn development_status() -> Status {
        Status {
            id: "3".to_string(),
            name: "Fazendo".to_string(),
            untranslated_name: None,
            category_key: "indeterminate".to_string(),
            category_name: "In Progress".to_string(),
            category_color: "yellow".to_string(),
            icon_url: None,
            is_development: true,
        }
    }

    fn jira_issue(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "key": format!("A-{id}"),
            "fields": {
                "summary": "Tarefa",
                "status": {"id": "3", "name": "Fazendo", "statusCategory": {"id": 4, "key": "indeterminate", "name": "In Progress", "colorName": "yellow"}},
                "assignee": {"accountId": "me", "displayName": "Eu"},
                "updated": "2026-11-17T12:00:00.000+0000"
            }
        })
    }

    #[tokio::test]
    async fn a_rejected_batch_is_rechecked_one_issue_at_a_time() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(body_partial_json(json!({"jql": "id in (1,2)"})))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"errorMessages": ["id inexistente"]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(body_partial_json(json!({"jql": "id = 1"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"issues": [jira_issue("1")]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(body_partial_json(json!({"jql": "id = 2"})))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"errorMessages": ["id inexistente"]})))
            .mount(&server)
            .await;

        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        let repo = IssueRepository::new(pool.clone());
        let mut first = issue("1", "A-1", "3");
        first.assignee_account_id = Some("me".to_string());
        let mut second = issue("2", "A-2", "3");
        second.assignee_account_id = Some("me".to_string());
        repo.save_issues_with_status(
            &[
                (first.clone(), development_status()),
                (second.clone(), development_status()),
            ],
            IssueDetail::Basic,
        )
        .await
        .unwrap();
        let throttle = SyncThrottle::new(5, Arc::new(SilentPublisher));
        let credentials = JiraCredentials {
            base_url: server.uri(),
            email: "me@example.com".to_string(),
            api_token: zeroize::Zeroizing::new("token".to_string()),
        };

        refresh_assigned_issues_outside_search(
            &JiraProvider::new(JiraClient::for_tests()),
            &credentials,
            &repo,
            &throttle,
            throttle.generation(),
            "me",
            &[],
            &[first, second],
        )
        .await
        .unwrap();

        let still_mine: Vec<String> = repo
            .get_all_for_user("me")
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(still_mine, vec!["1".to_string()]);
    }

    #[tokio::test]
    async fn a_single_rejected_issue_outside_the_search_loses_the_assignee() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(body_partial_json(json!({"jql": "id in (1)"})))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"errorMessages": ["id inexistente"]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .and(body_partial_json(json!({"jql": "id = 1"})))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"errorMessages": ["id inexistente"]})))
            .mount(&server)
            .await;

        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        let repo = IssueRepository::new(pool.clone());
        let mut only = issue("1", "A-1", "3");
        only.assignee_account_id = Some("me".to_string());
        repo.save_issues_with_status(&[(only.clone(), development_status())], IssueDetail::Basic)
            .await
            .unwrap();
        let throttle = SyncThrottle::new(5, Arc::new(SilentPublisher));
        let credentials = JiraCredentials {
            base_url: server.uri(),
            email: "me@example.com".to_string(),
            api_token: zeroize::Zeroizing::new("token".to_string()),
        };

        refresh_assigned_issues_outside_search(
            &JiraProvider::new(JiraClient::for_tests()),
            &credentials,
            &repo,
            &throttle,
            throttle.generation(),
            "me",
            &[],
            &[only],
        )
        .await
        .unwrap();

        assert!(repo.get_all_for_user("me").await.unwrap().is_empty());
    }

    fn issue(id: &str, key: &str, status_id: &str) -> Issue {
        Issue {
            id: id.to_string(),
            key: key.to_string(),
            summary: String::new(),
            status_id: status_id.to_string(),
            status_name: format!("S{status_id}"),
            status_category_key: "indeterminate".to_string(),
            assignee_account_id: None,
            assignee_display_name: None,
            priority_name: None,
            issue_type_name: None,
            due_date: None,
            browse_url: String::new(),
            jira_updated_at: String::new(),
            last_change_at: None,
            status_is_development: false,
        }
    }

    #[test]
    fn changed_issues_flags_an_assignee_change_without_status_change() {
        let mut fresh = issue("5", "A-5", "1");
        fresh.assignee_account_id = Some("other".to_string());
        let mut cached = issue("5", "A-5", "1");
        cached.assignee_account_id = Some("me".to_string());
        let with_history: HashSet<String> = ["5".to_string()].into_iter().collect();

        let changed = changed_issues(&[fresh], &[cached], &with_history);

        assert_eq!(changed, vec![("A-5".to_string(), "5".to_string())]);
    }

    #[test]
    fn only_development_or_recently_updated_issues_are_rechecked() {
        let mut developing = issue("1", "A-1", "1");
        developing.status_is_development = true;
        let mut updated = issue("2", "A-2", "1");
        updated.jira_updated_at = "2026-11-10T03:00:00.000Z".to_string();
        let mut stale_done = issue("3", "A-3", "1");
        stale_done.jira_updated_at = "2026-08-01T12:00:00.000Z".to_string();

        let ids: Vec<String> = worth_rechecking(&[developing, updated, stale_done], "2026-11-01T03:00:00.000Z")
            .into_iter()
            .map(|issue| issue.id)
            .collect();

        assert_eq!(ids, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn assigned_issues_missing_from_the_search_are_rechecked() {
        let fresh = vec![issue("1", "A-1", "1")];
        let cached = vec![issue("1", "A-1", "1"), issue("2", "A-2", "1"), issue("x'3", "A-3", "1")];

        assert_eq!(assigned_ids_outside_search(&fresh, &cached), vec!["2".to_string()]);
    }

    #[test]
    fn changed_issues_flags_new_status_changed_and_missing_history() {
        let fresh = vec![
            issue("1", "A-1", "1"),
            issue("2", "A-2", "2"),
            issue("3", "A-3", "1"),
            issue("4", "A-4", "1"),
        ];
        let cached = vec![issue("2", "A-2", "1"), issue("3", "A-3", "1"), issue("4", "A-4", "1")];
        let with_history: HashSet<String> = ["2".to_string(), "3".to_string()].into_iter().collect();

        let changed = changed_issues(&fresh, &cached, &with_history);

        assert_eq!(
            changed,
            vec![
                ("A-1".to_string(), "1".to_string()),
                ("A-2".to_string(), "2".to_string()),
                ("A-4".to_string(), "4".to_string()),
            ]
        );
    }
}
