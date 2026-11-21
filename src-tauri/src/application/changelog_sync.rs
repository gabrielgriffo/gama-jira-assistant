use std::collections::HashMap;

use crate::domain::changelog::IssueChange;
use crate::domain::credentials::JiraCredentials;
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail, Status};
use crate::infrastructure::database::{ChangelogRepository, IssueRepository};
use crate::infrastructure::integration::jira::{mapper, JiraProvider};

pub async fn sync_changelogs_in_bulk(
    provider: &JiraProvider,
    credentials: &JiraCredentials,
    changelog: &ChangelogRepository,
    issue_ids: &[String],
    before_save: impl Fn() -> Result<(), DomainError>,
) -> Result<(), DomainError> {
    let changes = fetch_changelogs_in_bulk(provider, credentials, issue_ids).await?;
    before_save()?;
    changelog.save_changes_by_issue(&changes).await
}

pub async fn save_issues_with_changelogs(
    provider: &JiraProvider,
    credentials: &JiraCredentials,
    issues_repo: &IssueRepository,
    changelog: &ChangelogRepository,
    issues: &[(Issue, Status)],
    before_save: impl Fn() -> Result<(), DomainError>,
) -> Result<(), DomainError> {
    if issues.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = issues.iter().map(|(issue, _)| issue.id.clone()).collect();
    let changes = fetch_changelogs_in_bulk(provider, credentials, &ids).await?;
    before_save()?;
    issues_repo.save_issues_with_status(issues, IssueDetail::Basic).await?;
    changelog.save_changes_by_issue(&changes).await
}

async fn fetch_changelogs_in_bulk(
    provider: &JiraProvider,
    credentials: &JiraCredentials,
    issue_ids: &[String],
) -> Result<HashMap<String, Vec<IssueChange>>, DomainError> {
    let histories = provider.get_changelogs_bulk(credentials, issue_ids).await?;
    Ok(histories
        .into_iter()
        .map(|(issue_id, entries)| (issue_id, mapper::map_changelogs(entries)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use crate::infrastructure::integration::jira::JiraClient;
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::str::FromStr;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        pool
    }

    fn credentials(server: &MockServer) -> JiraCredentials {
        JiraCredentials {
            base_url: server.uri(),
            email: "me@example.com".to_string(),
            api_token: zeroize::Zeroizing::new("token".to_string()),
        }
    }

    fn issue() -> (Issue, Status) {
        let status = Status {
            id: "3".to_string(),
            name: "Fazendo".to_string(),
            untranslated_name: None,
            category_key: "indeterminate".to_string(),
            category_name: "In Progress".to_string(),
            category_color: "yellow".to_string(),
            icon_url: None,
            is_development: false,
        };
        let issue = Issue {
            id: "10".to_string(),
            key: "GAMA-1".to_string(),
            summary: "Tarefa".to_string(),
            status_id: "3".to_string(),
            status_name: "Fazendo".to_string(),
            status_category_key: "indeterminate".to_string(),
            assignee_account_id: Some("me".to_string()),
            assignee_display_name: None,
            priority_name: None,
            issue_type_name: None,
            due_date: None,
            browse_url: String::new(),
            jira_updated_at: "2026-11-17T12:00:00.000Z".to_string(),
            last_change_at: None,
            status_is_development: false,
        };
        (issue, status)
    }

    async fn issue_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM jira_issues")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn issues_are_not_cached_without_their_changelog_when_the_bulk_fetch_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/changelog/bulkfetch"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let pool = pool().await;
        let provider = JiraProvider::new(JiraClient::for_tests());

        let result = save_issues_with_changelogs(
            &provider,
            &credentials(&server),
            &IssueRepository::new(pool.clone()),
            &ChangelogRepository::new(pool.clone()),
            &[issue()],
            || Ok(()),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(issue_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn issues_and_their_changelog_are_cached_together() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/changelog/bulkfetch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issueChangeLogs": [{
                    "issueId": "10",
                    "changeHistories": [{
                        "id": "1",
                        "created": "2026-11-17T10:00:00.000+0000",
                        "items": [{"field": "status", "fieldId": "status", "fieldtype": "jira", "from": "1", "to": "3"}]
                    }]
                }]
            })))
            .mount(&server)
            .await;
        let pool = pool().await;
        let provider = JiraProvider::new(JiraClient::for_tests());
        let changelog = ChangelogRepository::new(pool.clone());

        save_issues_with_changelogs(
            &provider,
            &credentials(&server),
            &IssueRepository::new(pool.clone()),
            &changelog,
            &[issue()],
            || Ok(()),
        )
        .await
        .unwrap();

        assert_eq!(issue_count(&pool).await, 1);
        assert_eq!(changelog.get_changelog("10").await.unwrap().entries.len(), 1);
    }

    #[tokio::test]
    async fn nothing_is_cached_when_the_account_changes_during_the_fetch() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/changelog/bulkfetch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "issueChangeLogs": [] })))
            .mount(&server)
            .await;
        let pool = pool().await;
        let provider = JiraProvider::new(JiraClient::for_tests());

        let result = save_issues_with_changelogs(
            &provider,
            &credentials(&server),
            &IssueRepository::new(pool.clone()),
            &ChangelogRepository::new(pool.clone()),
            &[issue()],
            || Err(DomainError::SyncCancelled),
        )
        .await;

        assert_eq!(result, Err(DomainError::SyncCancelled));
        assert_eq!(issue_count(&pool).await, 0);
    }
}
