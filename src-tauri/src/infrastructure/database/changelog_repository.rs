use crate::domain::changelog::{ChangelogEntry, ChangelogItem, ChangelogResponse, IssueChange};
use crate::domain::error::DomainError;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

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

        for change in changes {
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
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn changelog_exists(&self, issue_id: &str, changelog_id: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM issue_changes WHERE issue_id = ? AND changelog_id = ? LIMIT 1")
            .bind(issue_id)
            .bind(changelog_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(row.is_some())
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

    pub async fn get_status_transitions(&self, issue_id: &str) -> Result<Vec<StatusTransition>, DomainError> {
        let rows = sqlx::query(
            "SELECT c.jira_created_at,
                    COALESCE(f.is_development, 0) AS from_is_development,
                    COALESCE(s.is_development, 0) AS to_is_development
             FROM issue_changes c
             LEFT JOIN jira_statuses f ON f.id = c.from_id
             LEFT JOIN jira_statuses s ON s.id = c.to_id
             WHERE c.issue_id = ? AND c.field_id = 'status'
             ORDER BY c.jira_created_at ASC, c.item_index ASC",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| StatusTransition {
                jira_created_at: row.get("jira_created_at"),
                from_is_development: row.get("from_is_development"),
                to_is_development: row.get("to_is_development"),
            })
            .collect())
    }

    pub async fn get_assignee_transitions(&self, issue_id: &str) -> Result<Vec<AssigneeTransition>, DomainError> {
        let rows = sqlx::query(
            "SELECT jira_created_at, from_id, to_id
             FROM issue_changes
             WHERE issue_id = ? AND field_id = 'assignee'
             ORDER BY jira_created_at ASC, item_index ASC",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| AssigneeTransition {
                jira_created_at: row.get("jira_created_at"),
                from_account_id: row.get("from_id"),
                to_account_id: row.get("to_id"),
            })
            .collect())
    }
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
