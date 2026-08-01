use crate::application::jira_worklog_sync_service::to_synced_worklog;
use crate::domain::credentials::JiraCredentials;
use crate::domain::current_user::CurrentUser;
use crate::domain::date_range::parse_time_zone;
use crate::domain::error::DomainError;
use crate::domain::issue_key::validate_issue_id_or_key;
use crate::domain::worklog::WorklogEntry;
use crate::infrastructure::database::SqliteWorklogRepository;
use crate::infrastructure::integration::jira::JiraProvider;
use log::info;

const MAX_WORKLOGS_PER_SUBMIT: usize = 500;
const MAX_WORKLOG_HOURS: f64 = 24.0;
const MAX_COMMENT_CHARS: usize = 5000;

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

        let mut worklog_dto =
            crate::infrastructure::integration::jira::dto::JiraWorklogCreateDto::new(started, time_spent_seconds);
        if let Some(comment) = worklog.comment.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            worklog_dto = worklog_dto.with_comment(comment);
        }
        let created = self
            .jira_provider
            .submit_worklog(credentials, &worklog.issue_key, &worklog_dto)
            .await?;

        match created
            .issue_id
            .as_deref()
            .and_then(|issue_id| to_synced_worklog(&created, issue_id, &user.account_id, tz, "gama"))
        {
            Some(record) => {
                self.worklog_repository.upsert_synced(&[record]).await?;
            }
            None => log::warn!(
                "[WORKLOG] Worklog {} enviado, mas a resposta não trouxe issueId para gravar no cache",
                created.id
            ),
        }

        Ok(created.id)
    }
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
