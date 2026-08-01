use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail, Status};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct WorkCandidate {
    pub id: String,
    pub key: String,
    pub summary: String,
    pub assignee_account_id: Option<String>,
    pub current_is_development: bool,
}

#[derive(Clone)]
pub struct IssueRepository {
    pool: SqlitePool,
}

impl IssueRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn save_issue_with_status(
        &self,
        issue: &Issue,
        status: &Status,
        detail: IssueDetail,
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        sqlx::query(
            "INSERT INTO jira_statuses (id, name, untranslated_name, category_key, category_name, category_color, icon_url, synced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                name              = excluded.name,
                untranslated_name = COALESCE(excluded.untranslated_name, jira_statuses.untranslated_name),
                category_key      = excluded.category_key,
                category_name     = excluded.category_name,
                category_color    = excluded.category_color,
                icon_url          = COALESCE(excluded.icon_url, jira_statuses.icon_url),
                synced_at         = excluded.synced_at",
        )
        .bind(&status.id)
        .bind(&status.name)
        .bind(&status.untranslated_name)
        .bind(&status.category_key)
        .bind(&status.category_name)
        .bind(&status.category_color)
        .bind(&status.icon_url)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        let detail_updates = match detail {
            IssueDetail::Full => {
                "
                priority_name = excluded.priority_name,
                issue_type_name = excluded.issue_type_name,
                due_date = excluded.due_date,"
            }
            IssueDetail::Basic => "",
        };
        let query = format!(
            "INSERT INTO jira_issues (id, key, summary, status_id, assignee_account_id, assignee_display_name,
                                     priority_name, issue_type_name, due_date, browse_url, jira_updated_at, synced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                key = excluded.key,
                summary = excluded.summary,
                status_id = excluded.status_id,
                assignee_account_id = excluded.assignee_account_id,
                assignee_display_name = excluded.assignee_display_name,{detail_updates}
                browse_url = excluded.browse_url,
                jira_updated_at = excluded.jira_updated_at,
                synced_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"
        );

        sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(&issue.id)
            .bind(&issue.key)
            .bind(&issue.summary)
            .bind(&issue.status_id)
            .bind(&issue.assignee_account_id)
            .bind(&issue.assignee_display_name)
            .bind(&issue.priority_name)
            .bind(&issue.issue_type_name)
            .bind(&issue.due_date)
            .bind(&issue.browse_url)
            .bind(&issue.jira_updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn get_all_for_user(&self, account_id: &str) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query(
            "SELECT i.id, i.key, i.summary, i.status_id, i.assignee_account_id, i.assignee_display_name,
                    i.priority_name, i.issue_type_name, i.due_date, i.browse_url, i.jira_updated_at,
                    s.name AS status_name, s.category_key AS status_category_key
             FROM jira_issues i
             JOIN jira_statuses s ON s.id = i.status_id
             WHERE i.assignee_account_id = ?
             ORDER BY i.jira_updated_at DESC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows.into_iter().map(map_issue_row).collect())
    }

    pub async fn get_for_user_updated_between(
        &self,
        account_id: &str,
        from_utc: &str,
        to_utc: &str,
    ) -> Result<Vec<Issue>, DomainError> {
        let rows = sqlx::query(
            "SELECT i.id, i.key, i.summary, i.status_id, i.assignee_account_id, i.assignee_display_name,
                    i.priority_name, i.issue_type_name, i.due_date, i.browse_url, i.jira_updated_at,
                    s.name AS status_name, s.category_key AS status_category_key
             FROM jira_issues i
             JOIN jira_statuses s ON s.id = i.status_id
             WHERE i.assignee_account_id = ?
               AND i.jira_updated_at >= ?
               AND i.jira_updated_at < ?
             ORDER BY i.jira_updated_at DESC",
        )
        .bind(account_id)
        .bind(from_utc)
        .bind(to_utc)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows.into_iter().map(map_issue_row).collect())
    }

    pub async fn get_work_candidates(&self, account_id: &str) -> Result<Vec<WorkCandidate>, DomainError> {
        let rows = sqlx::query(
            "SELECT i.id, i.key, i.summary, i.assignee_account_id, COALESCE(s.is_development, 0) AS current_is_development
             FROM jira_issues i
             LEFT JOIN jira_statuses s ON s.id = i.status_id
             WHERE i.assignee_account_id = ?1
                OR EXISTS (
                    SELECT 1 FROM issue_changes c
                    WHERE c.issue_id = i.id AND c.field_id = 'assignee' AND (c.from_id = ?1 OR c.to_id = ?1)
                )
             ORDER BY i.key",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| WorkCandidate {
                id: row.get("id"),
                key: row.get("key"),
                summary: row.get("summary"),
                assignee_account_id: row.get("assignee_account_id"),
                current_is_development: row.get("current_is_development"),
            })
            .collect())
    }

    pub async fn get_id_by_key(&self, key: &str) -> Result<String, DomainError> {
        let row = sqlx::query("SELECT id FROM jira_issues WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .ok_or_else(|| DomainError::NotFound(format!("Issue with key {} not found", key)))?;

        Ok(row.get("id"))
    }

    pub async fn reconcile_assigned_issues_updated_between(
        &self,
        account_id: &str,
        current_ids: &[String],
        from_utc: &str,
        to_utc: &str,
    ) -> Result<(), DomainError> {
        use sqlx::AssertSqlSafe;

        let mut query_str = String::from(
            "UPDATE jira_issues
             SET assignee_account_id = NULL, assignee_display_name = NULL
             WHERE assignee_account_id = ?
               AND jira_updated_at >= ?
               AND jira_updated_at < ?",
        );
        if !current_ids.is_empty() {
            let placeholders = current_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query_str.push_str(&format!(" AND id NOT IN ({})", placeholders));
        }

        let mut query = sqlx::query(AssertSqlSafe(query_str.as_str()))
            .bind(account_id)
            .bind(from_utc)
            .bind(to_utc);
        for id in current_ids {
            query = query.bind(id);
        }

        query
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }
}

fn map_issue_row(row: sqlx::sqlite::SqliteRow) -> Issue {
    Issue {
        id: row.get("id"),
        key: row.get("key"),
        summary: row.get("summary"),
        status_id: row.get("status_id"),
        status_name: row.get("status_name"),
        status_category_key: row.get("status_category_key"),
        assignee_account_id: row.get("assignee_account_id"),
        assignee_display_name: row.get("assignee_display_name"),
        priority_name: row.get("priority_name"),
        issue_type_name: row.get("issue_type_name"),
        due_date: row.get("due_date"),
        browse_url: row.get("browse_url"),
        jira_updated_at: row.get("jira_updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn repository() -> IssueRepository {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        IssueRepository::new(pool)
    }

    fn status() -> Status {
        Status {
            id: "1".to_string(),
            name: "Doing".to_string(),
            untranslated_name: None,
            category_key: "indeterminate".to_string(),
            category_name: "In Progress".to_string(),
            category_color: "yellow".to_string(),
            icon_url: None,
            is_development: false,
        }
    }

    fn issue(summary: &str, priority: Option<&str>, issue_type: Option<&str>, due_date: Option<&str>) -> Issue {
        Issue {
            id: "10".to_string(),
            key: "GAMA-1".to_string(),
            summary: summary.to_string(),
            status_id: "1".to_string(),
            status_name: "Doing".to_string(),
            status_category_key: "indeterminate".to_string(),
            assignee_account_id: Some("me".to_string()),
            assignee_display_name: Some("Eu".to_string()),
            priority_name: priority.map(str::to_string),
            issue_type_name: issue_type.map(str::to_string),
            due_date: due_date.map(str::to_string),
            browse_url: "https://x/browse/GAMA-1".to_string(),
            jira_updated_at: "2026-10-01T12:00:00.000Z".to_string(),
        }
    }

    #[tokio::test]
    async fn basic_upsert_keeps_detail_columns() {
        let repo = repository().await;
        repo.save_issue_with_status(
            &issue("Antes", Some("High"), Some("Task"), Some("2026-10-10")),
            &status(),
            IssueDetail::Full,
        )
        .await
        .unwrap();
        repo.save_issue_with_status(&issue("Depois", None, None, None), &status(), IssueDetail::Basic)
            .await
            .unwrap();

        let saved = repo.get_all_for_user("me").await.unwrap().remove(0);
        assert_eq!(saved.summary, "Depois");
        assert_eq!(saved.priority_name.as_deref(), Some("High"));
        assert_eq!(saved.issue_type_name.as_deref(), Some("Task"));
        assert_eq!(saved.due_date.as_deref(), Some("2026-10-10"));
    }

    #[tokio::test]
    async fn full_upsert_clears_removed_details() {
        let repo = repository().await;
        repo.save_issue_with_status(
            &issue("Antes", Some("High"), Some("Task"), Some("2026-10-10")),
            &status(),
            IssueDetail::Full,
        )
        .await
        .unwrap();
        repo.save_issue_with_status(&issue("Depois", None, Some("Bug"), None), &status(), IssueDetail::Full)
            .await
            .unwrap();

        let saved = repo.get_all_for_user("me").await.unwrap().remove(0);
        assert_eq!(saved.priority_name, None);
        assert_eq!(saved.issue_type_name.as_deref(), Some("Bug"));
        assert_eq!(saved.due_date, None);
    }
}
