use crate::application::jira_worklog_sync_service::to_synced_worklog;
use crate::domain::credentials::JiraCredentials;
use crate::domain::current_user::CurrentUser;
use crate::domain::date_range::parse_time_zone;
use crate::domain::error::DomainError;
use crate::domain::issue_key::validate_issue_id_or_key;
use crate::domain::worklog::WorklogEntry;
use crate::infrastructure::database::{SqliteWorklogRepository, SyncedWorklog};
use crate::infrastructure::integration::jira::{JiraProvider, JiraWorklogDto};
use log::info;

const MAX_WORKLOGS_PER_SUBMIT: usize = 500;
const MAX_WORKLOG_HOURS: f64 = 24.0;
const MAX_COMMENT_CHARS: usize = 5000;
const CONFIRM_WINDOW_MS: i64 = 60_000;

pub struct WorklogService {
    jira_provider: JiraProvider,
    worklog_repository: SqliteWorklogRepository,
}

impl WorklogService {
    pub fn new(jira_provider: JiraProvider, worklog_repository: SqliteWorklogRepository) -> Self {
        Self {
            jira_provider,
            worklog_repository,
        }
    }

    pub async fn submit_worklogs_to_jira(
        &self,
        credentials: &JiraCredentials,
        user: &CurrentUser,
        worklogs: &[WorklogEntry],
    ) -> Result<usize, DomainError> {
        if worklogs.len() > MAX_WORKLOGS_PER_SUBMIT {
            return Err(DomainError::InvalidInput(format!(
                "no máximo {MAX_WORKLOGS_PER_SUBMIT} worklogs por envio"
            )));
        }

        info!("[WORKLOG] Enviando {} worklogs para o JIRA", worklogs.len());

        let mut synced = 0;
        let mut last_error: Option<DomainError> = None;
        for worklog in worklogs {
            match self.submit_one(credentials, user, worklog).await {
                Ok(_) => {
                    synced += 1;
                    info!("[WORKLOG] Worklog para {} enviado com sucesso", worklog.issue_key);
                }
                Err(e) => {
                    log::error!("[WORKLOG] Erro ao enviar worklog para {}: {}", worklog.issue_key, e);
                    last_error = Some(e);
                }
            }
        }

        info!(
            "[WORKLOG] {} de {} worklogs enviados com sucesso",
            synced,
            worklogs.len()
        );

        match last_error {
            Some(err) if synced == 0 => Err(err),
            _ => Ok(synced),
        }
    }

    async fn submit_one(
        &self,
        credentials: &JiraCredentials,
        user: &CurrentUser,
        worklog: &WorklogEntry,
    ) -> Result<String, DomainError> {
        validate_issue_id_or_key(&worklog.issue_key)?;
        let tz = parse_time_zone(&user.time_zone);
        let started = jira_started(&worklog.date, &worklog.start_timestamp, tz)?;
        if !worklog.hours.is_finite() || worklog.hours <= 0.0 || worklog.hours > MAX_WORKLOG_HOURS {
            return Err(DomainError::InvalidInput(format!(
                "worklog de {} com duração inválida",
                worklog.issue_key
            )));
        }
        let time_spent_seconds = (worklog.hours * 3600.0).round() as u32;
        if time_spent_seconds < 60 {
            return Err(DomainError::ParseError(format!(
                "worklog de {} com menos de 1 minuto",
                worklog.issue_key
            )));
        }
        if worklog
            .comment
            .as_deref()
            .is_some_and(|c| c.chars().count() > MAX_COMMENT_CHARS)
        {
            return Err(DomainError::InvalidInput(format!(
                "comentário do worklog de {} excede {MAX_COMMENT_CHARS} caracteres",
                worklog.issue_key
            )));
        }

        let started_ms = timestamp_ms(&started)?;
        if let Some(id) = self
            .recover_unrecorded(credentials, user, &worklog.issue_key, started_ms, time_spent_seconds)
            .await
        {
            info!(
                "[WORKLOG] Worklog {} já existia no Jira para {}, envio não repetido",
                id, worklog.issue_key
            );
            return Ok(id);
        }

        let mut worklog_dto =
            crate::infrastructure::integration::jira::dto::JiraWorklogCreateDto::new(started, time_spent_seconds);
        if let Some(comment) = worklog.comment.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            worklog_dto = worklog_dto.with_comment(comment);
        }
        let created = match self
            .jira_provider
            .submit_worklog(credentials, &worklog.issue_key, &worklog_dto)
            .await
        {
            Ok(created) => created,
            Err(err) => {
                return match self
                    .recover_unrecorded(credentials, user, &worklog.issue_key, started_ms, time_spent_seconds)
                    .await
                {
                    Some(id) => {
                        log::warn!(
                            "[WORKLOG] Envio para {} falhou ({}), mas o worklog {} foi criado no Jira",
                            worklog.issue_key,
                            err,
                            id
                        );
                        Ok(id)
                    }
                    None => Err(err),
                };
            }
        };

        match created
            .issue_id
            .as_deref()
            .and_then(|issue_id| to_synced_worklog(&created, issue_id, &user.account_id, tz, "gama"))
        {
            Some(record) => {
                if let Err(e) = self.worklog_repository.insert_unconfirmed(&record).await {
                    log::warn!(
                        "[WORKLOG] Worklog {} enviado, mas não foi possível gravar no cache: {}",
                        record.jira_worklog_id,
                        e
                    );
                }
                if let Err(e) = self.confirm_in_jira(credentials, user, &record).await {
                    log::warn!(
                        "[WORKLOG] Worklog {} enviado, mas não foi possível confirmar no Jira: {}",
                        record.jira_worklog_id,
                        e
                    );
                }
            }
            None => log::warn!(
                "[WORKLOG] Worklog {} enviado, mas a resposta não trouxe issueId para gravar no cache",
                created.id
            ),
        }

        Ok(created.id)
    }

    async fn recover_unrecorded(
        &self,
        credentials: &JiraCredentials,
        user: &CurrentUser,
        issue_key: &str,
        started_ms: i64,
        time_spent_seconds: u32,
    ) -> Option<String> {
        match self
            .find_unrecorded(credentials, user, issue_key, started_ms, time_spent_seconds)
            .await
        {
            Ok(found) => found,
            Err(e) => {
                log::warn!(
                    "[WORKLOG] Não foi possível procurar worklog já existente em {}: {}",
                    issue_key,
                    e
                );
                None
            }
        }
    }

    async fn find_unrecorded(
        &self,
        credentials: &JiraCredentials,
        user: &CurrentUser,
        issue_key: &str,
        started_ms: i64,
        time_spent_seconds: u32,
    ) -> Result<Option<String>, DomainError> {
        let window = (started_ms - CONFIRM_WINDOW_MS, started_ms + CONFIRM_WINDOW_MS);
        let candidates = self
            .jira_provider
            .get_worklogs_started_between(credentials, issue_key, Some(window))
            .await?;
        let tz = parse_time_zone(&user.time_zone);

        for candidate in candidates
            .iter()
            .filter(|w| is_same_worklog(w, &user.account_id, started_ms, time_spent_seconds))
        {
            if self.worklog_repository.has_jira_worklog(&candidate.id).await? {
                if self.worklog_repository.adopt_as_gama(&candidate.id).await? {
                    return Ok(Some(candidate.id.clone()));
                }
                continue;
            }
            match candidate
                .issue_id
                .as_deref()
                .and_then(|issue_id| to_synced_worklog(candidate, issue_id, &user.account_id, tz, "gama"))
            {
                Some(record) => {
                    self.worklog_repository.upsert_synced(&[record]).await?;
                }
                None => log::warn!(
                    "[WORKLOG] Worklog {} encontrado no Jira sem issueId para gravar no cache",
                    candidate.id
                ),
            }
            return Ok(Some(candidate.id.clone()));
        }
        Ok(None)
    }

    async fn confirm_in_jira(
        &self,
        credentials: &JiraCredentials,
        user: &CurrentUser,
        sent: &SyncedWorklog,
    ) -> Result<(), DomainError> {
        let started_ms = chrono::DateTime::parse_from_rfc3339(&sent.started_at)
            .map_err(|e| DomainError::ParseError(format!("início do worklog {}: {e}", sent.jira_worklog_id)))?
            .timestamp_millis();
        let window = (started_ms - CONFIRM_WINDOW_MS, started_ms + CONFIRM_WINDOW_MS);
        let tz = parse_time_zone(&user.time_zone);
        let found = self
            .jira_provider
            .get_worklogs_started_between(credentials, &sent.issue_id, Some(window))
            .await?
            .into_iter()
            .find(|w| w.id == sent.jira_worklog_id)
            .and_then(|w| to_synced_worklog(&w, &sent.issue_id, &user.account_id, tz, "gama"));

        match found {
            Some(confirmed) => {
                self.worklog_repository.upsert_synced(&[confirmed]).await?;
                info!("[WORKLOG] Worklog {} confirmado no Jira", sent.jira_worklog_id);
            }
            None => log::warn!("[WORKLOG] Worklog {} ainda não aparece no Jira", sent.jira_worklog_id),
        }
        Ok(())
    }
}

fn is_same_worklog(worklog: &JiraWorklogDto, account_id: &str, started_ms: i64, time_spent_seconds: u32) -> bool {
    worklog.author.as_ref().is_some_and(|a| a.account_id == account_id)
        && worklog.time_spent_seconds == i64::from(time_spent_seconds)
        && timestamp_ms(&worklog.started).is_ok_and(|ms| ms / 1000 == started_ms / 1000)
}

fn timestamp_ms(value: &str) -> Result<i64, DomainError> {
    chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z")
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(value))
        .map(|d| d.timestamp_millis())
        .map_err(|_| DomainError::ParseError(format!("horário inválido: {value}")))
}

fn jira_started(date: &str, start: &str, tz: chrono_tz::Tz) -> Result<String, DomainError> {
    use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone};

    if let Ok(parsed) =
        DateTime::parse_from_str(start, "%Y-%m-%dT%H:%M:%S%.f%z").or_else(|_| DateTime::parse_from_rfc3339(start))
    {
        return Ok(parsed.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string());
    }

    let time_part = start.rsplit('T').next().unwrap_or(start);
    let hh_mm = time_part.get(..time_part.len().min(5)).unwrap_or(time_part);
    let time = NaiveTime::parse_from_str(hh_mm, "%H:%M")
        .map_err(|_| DomainError::ParseError(format!("horário inválido: {start}")))?;
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| DomainError::ParseError(format!("data inválida: {date}")))?;
    let local = tz
        .from_local_datetime(&day.and_time(time))
        .earliest()
        .ok_or_else(|| DomainError::ParseError(format!("horário inexistente no fuso: {date} {start}")))?;

    Ok(local.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::integration::jira::JiraClient;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const WORKLOG_PATH: &str = "/rest/api/3/issue/GAMA-1/worklog";

    async fn service() -> (WorklogService, SqliteWorklogRepository) {
        let repo = SqliteWorklogRepository::for_tests().await;
        let service = WorklogService::new(JiraProvider::new(JiraClient::for_tests()), repo.clone());
        (service, repo)
    }

    fn credentials(server: &MockServer) -> JiraCredentials {
        JiraCredentials {
            base_url: server.uri(),
            email: "me@example.com".to_string(),
            api_token: zeroize::Zeroizing::new("token".to_string()),
        }
    }

    fn user() -> CurrentUser {
        CurrentUser {
            account_id: "me".to_string(),
            display_name: "Eu".to_string(),
            email: "me@example.com".to_string(),
            time_zone: "America/Sao_Paulo".to_string(),
        }
    }

    fn entry() -> WorklogEntry {
        WorklogEntry {
            date: "2026-11-17".to_string(),
            issue_key: "GAMA-1".to_string(),
            issue_summary: "Tarefa".to_string(),
            status: "Fazendo".to_string(),
            hours: 1.0,
            start_timestamp: "09:00".to_string(),
            end_timestamp: "10:00".to_string(),
            comment: None,
        }
    }

    fn jira_worklog(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "issueId": "10",
            "author": {"accountId": "me"},
            "started": "2026-11-17T09:00:00.000-0300",
            "timeSpentSeconds": 3600
        })
    }

    fn page(worklogs: Vec<serde_json::Value>) -> ResponseTemplate {
        let total = worklogs.len();
        ResponseTemplate::new(200).set_body_json(json!({"total": total, "worklogs": worklogs}))
    }

    async fn gama_rows(repo: &SqliteWorklogRepository) -> Vec<(String, String)> {
        repo.get_synced_for_author("me", "2026-11-17", "2026-11-17")
            .await
            .unwrap()
            .into_iter()
            .map(|w| (w.jira_worklog_id.unwrap_or_default(), w.origin))
            .collect()
    }

    #[tokio::test]
    async fn failed_post_that_was_created_in_jira_is_recorded_instead_of_failing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(WORKLOG_PATH))
            .respond_with(page(vec![]))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(WORKLOG_PATH))
            .respond_with(page(vec![jira_worklog("900")]))
            .with_priority(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(WORKLOG_PATH))
            .respond_with(ResponseTemplate::new(504))
            .expect(1)
            .mount(&server)
            .await;
        let (service, repo) = service().await;

        let sent = service
            .submit_worklogs_to_jira(&credentials(&server), &user(), &[entry()])
            .await
            .unwrap();

        assert_eq!(sent, 1);
        assert_eq!(gama_rows(&repo).await, vec![("900".to_string(), "gama".to_string())]);
    }

    #[tokio::test]
    async fn resending_a_worklog_already_in_jira_but_not_in_the_cache_does_not_post_again() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(WORKLOG_PATH))
            .respond_with(page(vec![jira_worklog("901")]))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(WORKLOG_PATH))
            .respond_with(ResponseTemplate::new(201).set_body_json(jira_worklog("999")))
            .expect(0)
            .mount(&server)
            .await;
        let (service, repo) = service().await;

        let sent = service
            .submit_worklogs_to_jira(&credentials(&server), &user(), &[entry()])
            .await
            .unwrap();

        assert_eq!(sent, 1);
        assert_eq!(gama_rows(&repo).await, vec![("901".to_string(), "gama".to_string())]);
    }

    #[tokio::test]
    async fn a_worklog_already_synced_from_jira_after_a_failed_send_is_not_posted_again() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(WORKLOG_PATH))
            .respond_with(page(vec![jira_worklog("904")]))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(WORKLOG_PATH))
            .respond_with(ResponseTemplate::new(201).set_body_json(jira_worklog("905")))
            .expect(0)
            .mount(&server)
            .await;
        let (service, repo) = service().await;
        repo.upsert_synced(&[SyncedWorklog {
            jira_worklog_id: "904".to_string(),
            issue_id: "10".to_string(),
            author_account_id: "me".to_string(),
            started_at: "2026-11-17T12:00:00.000Z".to_string(),
            started_local_date: "2026-11-17".to_string(),
            time_spent_seconds: 3600,
            comment_text: None,
            origin: "jira",
            jira_created_at: None,
            jira_updated_at: None,
        }])
        .await
        .unwrap();

        service
            .submit_worklogs_to_jira(&credentials(&server), &user(), &[entry()])
            .await
            .unwrap();

        assert_eq!(gama_rows(&repo).await, vec![("904".to_string(), "gama".to_string())]);
    }

    #[tokio::test]
    async fn registering_again_creates_a_new_worklog_when_the_existing_one_is_already_cached() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(WORKLOG_PATH))
            .respond_with(page(vec![jira_worklog("902")]))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(WORKLOG_PATH))
            .respond_with(ResponseTemplate::new(201).set_body_json(jira_worklog("903")))
            .expect(1)
            .mount(&server)
            .await;
        let (service, repo) = service().await;
        repo.upsert_synced(&[SyncedWorklog {
            jira_worklog_id: "902".to_string(),
            issue_id: "10".to_string(),
            author_account_id: "me".to_string(),
            started_at: "2026-11-17T12:00:00.000Z".to_string(),
            started_local_date: "2026-11-17".to_string(),
            time_spent_seconds: 3600,
            comment_text: None,
            origin: "gama",
            jira_created_at: None,
            jira_updated_at: None,
        }])
        .await
        .unwrap();

        service
            .submit_worklogs_to_jira(&credentials(&server), &user(), &[entry()])
            .await
            .unwrap();

        let ids: Vec<String> = gama_rows(&repo).await.into_iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&"903".to_string()));
    }

    #[tokio::test]
    async fn failed_post_without_a_worklog_in_jira_is_reported_as_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(WORKLOG_PATH))
            .respond_with(page(vec![]))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(WORKLOG_PATH))
            .respond_with(ResponseTemplate::new(400))
            .expect(1)
            .mount(&server)
            .await;
        let (service, repo) = service().await;

        let result = service
            .submit_worklogs_to_jira(&credentials(&server), &user(), &[entry()])
            .await;

        assert!(result.is_err());
        assert!(gama_rows(&repo).await.is_empty());
    }

    #[test]
    fn jira_started_uses_time_zone_offset() {
        let tz: chrono_tz::Tz = "America/Sao_Paulo".parse().unwrap();
        assert_eq!(
            jira_started("2026-10-05", "09:30", tz).unwrap(),
            "2026-10-05T09:30:00.000-0300"
        );
        assert_eq!(
            jira_started("2026-10-05", "2026-10-05T09:30", tz).unwrap(),
            "2026-10-05T09:30:00.000-0300"
        );
        assert_eq!(
            jira_started("2026-10-05", "2026-10-05T12:30:00.000+0000", tz).unwrap(),
            "2026-10-05T12:30:00.000+0000"
        );
    }
}
