use crate::domain::changelog::{ChangelogEntry, ChangelogItem, ChangelogResponse, IssueChange};
use crate::domain::error::DomainError;
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct ChangelogRepository {
    pool: SqlitePool,
}

impl ChangelogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn save_changes(&self, issue_id: &str, changes: &[IssueChange]) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        insert_changes(&mut tx, issue_id, changes).await?;
        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))
    }

    pub async fn save_changes_by_issue(&self, changes: &HashMap<String, Vec<IssueChange>>) -> Result<(), DomainError> {
        if changes.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        for (issue_id, issue_changes) in changes {
            insert_changes(&mut tx, issue_id, issue_changes).await?;
        }
        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))
    }

    pub async fn get_changelog(&self, issue_id: &str) -> Result<ChangelogResponse, DomainError> {
        let result = self.get(issue_id).await?;
        Ok(result.unwrap_or_else(|| ChangelogResponse {
            earliest_date: None,
            latest_date: None,
            entries: vec![],
        }))
    }

    pub async fn get(&self, issue_id: &str) -> Result<Option<ChangelogResponse>, DomainError> {
        let rows = sqlx::query(
            "SELECT changelog_id, author_account_id, author_display_name, jira_created_at,
                    field_id, field, field_type, from_id, from_string, to_id, to_string
             FROM issue_changes
             WHERE issue_id = ?
             ORDER BY jira_created_at DESC, item_index ASC",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }

        let mut entries_map: HashMap<String, ChangelogEntry> = HashMap::new();
        let mut earliest_date: Option<String> = None;
        let mut latest_date: Option<String> = None;

        for row in rows {
            let changelog_id: String = row.get("changelog_id");
            let jira_created_at: String = row.get("jira_created_at");

            if earliest_date.is_none() || Some(&jira_created_at) < earliest_date.as_ref() {
                earliest_date = Some(jira_created_at.clone());
            }
            if latest_date.is_none() || Some(&jira_created_at) > latest_date.as_ref() {
                latest_date = Some(jira_created_at.clone());
            }

            let entry = entries_map
                .entry(changelog_id.clone())
                .or_insert_with(|| ChangelogEntry {
                    changelog_id: changelog_id.clone(),
                    author_account_id: row.get("author_account_id"),
                    author_display_name: row.get("author_display_name"),
                    jira_created_at: jira_created_at.clone(),
                    items: Vec::new(),
                });

            entry.items.push(ChangelogItem {
                field_id: row.get("field_id"),
                field: row.get("field"),
                field_type: row.get("field_type"),
                from_id: row.get("from_id"),
                from_string: row.get("from_string"),
                to_id: row.get("to_id"),
                to_string: row.get("to_string"),
            });
        }

        let mut entries: Vec<ChangelogEntry> = entries_map.into_values().collect();
        entries.sort_by(|a, b| b.jira_created_at.cmp(&a.jira_created_at));

        Ok(Some(ChangelogResponse {
            earliest_date,
            latest_date,
            entries,
        }))
    }

    pub async fn issue_ids_with_changes(&self, issue_ids: &[String]) -> Result<HashSet<String>, DomainError> {
        if issue_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let rows = sqlx::query(
            "SELECT DISTINCT issue_id FROM issue_changes WHERE issue_id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids_json(issue_ids)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
        Ok(rows.into_iter().map(|row| row.get("issue_id")).collect())
    }

    pub async fn get_status_transitions_by_issue(
        &self,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Vec<StatusTransition>>, DomainError> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query(
            "SELECT c.issue_id,
                    c.jira_created_at,
                    COALESCE(f.is_development, 0) AS from_is_development,
                    COALESCE(s.is_development, 0) AS to_is_development
             FROM issue_changes c
             LEFT JOIN jira_statuses f ON f.id = c.from_id
             LEFT JOIN jira_statuses s ON s.id = c.to_id
             WHERE c.field_id = 'status' AND c.issue_id IN (SELECT value FROM json_each(?))
             ORDER BY c.issue_id, c.jira_created_at ASC, c.item_index ASC",
        )
        .bind(ids_json(issue_ids)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        let mut grouped: HashMap<String, Vec<StatusTransition>> = HashMap::new();
        for row in rows {
            grouped.entry(row.get("issue_id")).or_default().push(StatusTransition {
                jira_created_at: row.get("jira_created_at"),
                from_is_development: row.get("from_is_development"),
                to_is_development: row.get("to_is_development"),
            });
        }
        Ok(grouped)
    }

    pub async fn get_assignee_transitions_by_issue(
        &self,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Vec<AssigneeTransition>>, DomainError> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query(
            "SELECT issue_id, jira_created_at, from_id, to_id
             FROM issue_changes
             WHERE field_id = 'assignee' AND issue_id IN (SELECT value FROM json_each(?))
             ORDER BY issue_id, jira_created_at ASC, item_index ASC",
        )
        .bind(ids_json(issue_ids)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        let mut grouped: HashMap<String, Vec<AssigneeTransition>> = HashMap::new();
        for row in rows {
            grouped
                .entry(row.get("issue_id"))
                .or_default()
                .push(AssigneeTransition {
                    jira_created_at: row.get("jira_created_at"),
                    from_account_id: row.get("from_id"),
                    to_account_id: row.get("to_id"),
                });
        }
        Ok(grouped)
    }
}

async fn insert_changes(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    issue_id: &str,
    changes: &[IssueChange],
) -> Result<(), DomainError> {
    for change in changes {
        let duplicate: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM issue_changes
             WHERE changelog_id = ? AND issue_id = ? AND field = ?
               AND field_id IS ? AND from_id IS ? AND to_id IS ?
             LIMIT 1",
        )
        .bind(&change.changelog_id)
        .bind(issue_id)
        .bind(&change.field)
        .bind(&change.field_id)
        .bind(&change.from_id)
        .bind(&change.to_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
        if duplicate.is_some() {
            continue;
        }
        sqlx::query(
            "INSERT INTO issue_changes
             (changelog_id, item_index, issue_id, author_account_id, author_display_name,
              jira_created_at, field_id, field, field_type, from_id, from_string, to_id, to_string)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(changelog_id, item_index) DO NOTHING",
        )
        .bind(&change.changelog_id)
        .bind(change.item_index)
        .bind(issue_id)
        .bind(&change.author_account_id)
        .bind(&change.author_display_name)
        .bind(&change.jira_created_at)
        .bind(&change.field_id)
        .bind(&change.field)
        .bind(&change.field_type)
        .bind(&change.from_id)
        .bind(&change.from_string)
        .bind(&change.to_id)
        .bind(&change.to_string)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
    }
    Ok(())
}

fn ids_json(issue_ids: &[String]) -> Result<String, DomainError> {
    serde_json::to_string(issue_ids).map_err(|e| DomainError::Storage(e.to_string()))
}

#[derive(Debug, Clone)]
pub struct StatusTransition {
    pub jira_created_at: String,
    pub from_is_development: bool,
    pub to_is_development: bool,
}

#[derive(Debug, Clone)]
pub struct AssigneeTransition {
    pub jira_created_at: String,
    pub from_account_id: Option<String>,
    pub to_account_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::issue::{Issue, IssueDetail, Status};
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use crate::infrastructure::database::IssueRepository;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

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

    fn status(id: &str, is_development: bool) -> Status {
        Status {
            id: id.to_string(),
            name: format!("S{id}"),
            untranslated_name: None,
            category_key: "indeterminate".to_string(),
            category_name: "In Progress".to_string(),
            category_color: "yellow".to_string(),
            icon_url: None,
            is_development,
        }
    }

    fn issue(id: &str) -> Issue {
        Issue {
            id: id.to_string(),
            key: format!("GAMA-{id}"),
            summary: "x".to_string(),
            status_id: "1".to_string(),
            status_name: "S1".to_string(),
            status_category_key: "indeterminate".to_string(),
            assignee_account_id: Some("me".to_string()),
            assignee_display_name: None,
            priority_name: None,
            issue_type_name: None,
            due_date: None,
            browse_url: String::new(),
            jira_updated_at: "2026-10-01T12:00:00.000Z".to_string(),
            last_change_at: None,
            status_is_development: false,
        }
    }

    fn change(id: &str, index: i32, at: &str, field: &str, from: &str, to: &str) -> IssueChange {
        IssueChange {
            changelog_id: id.to_string(),
            item_index: index,
            author_account_id: None,
            author_display_name: None,
            jira_created_at: at.to_string(),
            field_id: Some(field.to_string()),
            field: field.to_string(),
            field_type: "jira".to_string(),
            from_id: Some(from.to_string()),
            from_string: None,
            to_id: Some(to.to_string()),
            to_string: None,
        }
    }

    #[tokio::test]
    async fn the_same_transition_from_two_endpoints_is_stored_once() {
        let pool = pool().await;
        IssueRepository::new(pool.clone())
            .save_issue_with_status(&issue("10"), &status("1", false), IssueDetail::Basic)
            .await
            .unwrap();
        let repo = ChangelogRepository::new(pool.clone());

        repo.save_changes("10", &[change("c1", 1, "2026-10-02T10:00:00.000Z", "status", "1", "2")])
            .await
            .unwrap();
        repo.save_changes("10", &[change("c1", 0, "2026-10-02T10:00:00.000Z", "status", "1", "2")])
            .await
            .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issue_changes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn issues_carry_last_status_or_assignee_change_and_development_flag() {
        let pool = pool().await;
        let issues = IssueRepository::new(pool.clone());
        let mut developing = issue("10");
        developing.status_id = "2".to_string();
        issues
            .save_issue_with_status(&developing, &status("2", true), IssueDetail::Basic)
            .await
            .unwrap();
        issues
            .save_issue_with_status(&issue("20"), &status("1", false), IssueDetail::Basic)
            .await
            .unwrap();
        sqlx::query("UPDATE jira_statuses SET is_development = 1 WHERE id = '2'")
            .execute(&pool)
            .await
            .unwrap();
        let repo = ChangelogRepository::new(pool);
        repo.save_changes(
            "10",
            &[
                change("a", 0, "2026-10-02T10:00:00.000Z", "status", "1", "2"),
                change("b", 0, "2026-10-05T10:00:00.000Z", "assignee", "other", "me"),
                change("c", 0, "2026-10-09T10:00:00.000Z", "summary", "x", "y"),
            ],
        )
        .await
        .unwrap();

        let mut saved = issues.get_all_for_user("me").await.unwrap();
        saved.sort_by(|a, b| a.id.cmp(&b.id));

        assert_eq!(saved[0].last_change_at.as_deref(), Some("2026-10-05T10:00:00.000Z"));
        assert!(saved[0].status_is_development);
        assert_eq!(saved[1].last_change_at, None);
        assert!(!saved[1].status_is_development);
    }

    async fn seeded() -> ChangelogRepository {
        let pool = pool().await;
        let issues = IssueRepository::new(pool.clone());
        issues
            .save_issue_with_status(&issue("10"), &status("1", false), IssueDetail::Basic)
            .await
            .unwrap();
        issues
            .save_issue_with_status(&issue("20"), &status("2", true), IssueDetail::Basic)
            .await
            .unwrap();
        issues
            .save_issue_with_status(&issue("30"), &status("1", false), IssueDetail::Basic)
            .await
            .unwrap();
        sqlx::query("UPDATE jira_statuses SET is_development = 1 WHERE id = '2'")
            .execute(&pool)
            .await
            .unwrap();

        let repo = ChangelogRepository::new(pool);
        repo.save_changes(
            "10",
            &[
                change("b", 0, "2026-10-02T10:00:00.000Z", "status", "2", "1"),
                change("a", 1, "2026-10-01T10:00:00.000Z", "status", "1", "2"),
                change("a", 0, "2026-10-01T10:00:00.000Z", "assignee", "other", "me"),
            ],
        )
        .await
        .unwrap();
        repo.save_changes("20", &[change("c", 0, "2026-10-03T10:00:00.000Z", "status", "1", "2")])
            .await
            .unwrap();
        repo.save_changes("30", &[change("d", 0, "2026-10-04T10:00:00.000Z", "status", "1", "2")])
            .await
            .unwrap();
        repo
    }

    #[tokio::test]
    async fn status_transitions_grouped_by_requested_issue_in_order() {
        let repo = seeded().await;
        let ids = vec!["10".to_string(), "20".to_string(), "40".to_string()];
        let map = repo.get_status_transitions_by_issue(&ids).await.unwrap();

        assert_eq!(map.len(), 2);
        let first = &map["10"];
        assert_eq!(
            first.iter().map(|t| t.jira_created_at.as_str()).collect::<Vec<_>>(),
            vec!["2026-10-01T10:00:00.000Z", "2026-10-02T10:00:00.000Z"]
        );
        assert!(!first[0].from_is_development && first[0].to_is_development);
        assert!(first[1].from_is_development && !first[1].to_is_development);
        assert_eq!(map["20"].len(), 1);
        assert!(!map.contains_key("30"));
    }

    #[tokio::test]
    async fn assignee_transitions_grouped_by_requested_issue() {
        let repo = seeded().await;
        let map = repo
            .get_assignee_transitions_by_issue(&["10".to_string(), "20".to_string()])
            .await
            .unwrap();

        assert_eq!(map.len(), 1);
        assert_eq!(map["10"][0].from_account_id.as_deref(), Some("other"));
        assert_eq!(map["10"][0].to_account_id.as_deref(), Some("me"));
    }

    #[tokio::test]
    async fn empty_issue_list_returns_empty_maps() {
        let repo = seeded().await;
        assert!(repo.get_status_transitions_by_issue(&[]).await.unwrap().is_empty());
        assert!(repo.get_assignee_transitions_by_issue(&[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn issue_ids_with_changes_returns_only_requested_ids_that_have_history() {
        let repo = seeded().await;
        let ids = vec!["10".to_string(), "20".to_string(), "40".to_string()];
        let found = repo.issue_ids_with_changes(&ids).await.unwrap();
        assert_eq!(found, ["10".to_string(), "20".to_string()].into_iter().collect());
        assert!(repo.issue_ids_with_changes(&[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_changes_by_issue_stores_every_issue_and_ignores_duplicates() {
        let repo = seeded().await;
        let mut changes = HashMap::new();
        changes.insert(
            "10".to_string(),
            vec![
                change("b", 0, "2026-10-02T10:00:00.000Z", "status", "2", "1"),
                change("e", 0, "2026-10-05T10:00:00.000Z", "status", "1", "2"),
            ],
        );
        changes.insert(
            "20".to_string(),
            vec![change("f", 0, "2026-10-06T10:00:00.000Z", "status", "2", "1")],
        );

        repo.save_changes_by_issue(&changes).await.unwrap();

        let map = repo
            .get_status_transitions_by_issue(&["10".to_string(), "20".to_string()])
            .await
            .unwrap();
        assert_eq!(map["10"].len(), 3);
        assert_eq!(map["20"].len(), 2);
    }
}
