use crate::domain::error::DomainError;
use crate::domain::issue::Status;
use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct StatusRepository {
    pool: SqlitePool,
}

impl StatusRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn sync_catalog(&self, statuses: &[Status]) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        for status in statuses {
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
        }

        if !statuses.is_empty() {
            use sqlx::AssertSqlSafe;

            let status_ids: Vec<String> = statuses.iter().map(|s| s.id.clone()).collect();
            let placeholders = status_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query_str = format!(
                "DELETE FROM jira_statuses
                 WHERE id NOT IN ({})
                   AND id NOT IN (SELECT status_id FROM jira_issues)
                   AND id NOT IN (SELECT from_id FROM issue_changes WHERE field_id = 'status' AND from_id IS NOT NULL)
                   AND id NOT IN (SELECT to_id FROM issue_changes WHERE field_id = 'status' AND to_id IS NOT NULL)",
                placeholders
            );

            let mut query = sqlx::query(AssertSqlSafe(query_str.as_str()));
            for id in &status_ids {
                query = query.bind(id);
            }

            query
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<Status>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, name, untranslated_name, category_key, category_name, category_color, icon_url, is_development
             FROM jira_statuses
             ORDER BY CASE category_key WHEN 'new' THEN 0 WHEN 'indeterminate' THEN 1 WHEN 'done' THEN 2 ELSE 3 END, name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| Status {
                id: row.get("id"),
                name: row.get("name"),
                untranslated_name: row.get("untranslated_name"),
                category_key: row.get("category_key"),
                category_name: row.get("category_name"),
                category_color: row.get("category_color"),
                icon_url: row.get("icon_url"),
                is_development: row.get("is_development"),
            })
            .collect())
    }

    pub async fn get_development_ids(&self) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query("SELECT id FROM jira_statuses WHERE is_development = 1 ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    pub async fn update_multiple_is_development(&self, updates: &[(String, bool)]) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        for (status_id, is_development) in updates {
            sqlx::query(
                "UPDATE jira_statuses
                 SET is_development = ?
                 WHERE id = ?",
            )
            .bind(is_development)
            .bind(status_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    pub async fn get_sync_timestamp(&self) -> Result<Option<String>, DomainError> {
        let row = sqlx::query("SELECT MAX(synced_at) as last_sync FROM jira_statuses")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(row.and_then(|r| r.get("last_sync")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    fn status(id: &str) -> Status {
        Status {
            id: id.to_string(),
            name: format!("S{id}"),
            untranslated_name: None,
            category_key: "indeterminate".to_string(),
            category_name: "In Progress".to_string(),
            category_color: "yellow".to_string(),
            icon_url: None,
            is_development: false,
        }
    }

    #[tokio::test]
    async fn catalog_sync_keeps_statuses_still_present_in_the_history() {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        let repo = StatusRepository::new(pool.clone());
        repo.sync_catalog(&[status("1"), status("2"), status("3")])
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO jira_issues (id, key, summary, status_id, browse_url, jira_updated_at)
             VALUES ('10', 'GAMA-1', 'x', '1', '', '2026-10-01T00:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO issue_changes (changelog_id, item_index, issue_id, jira_created_at, field_id, field, field_type, from_id, to_id)
             VALUES ('c1', 0, '10', '2026-10-01T00:00:00.000Z', 'status', 'status', 'jira', '2', '1')",
        )
        .execute(&pool)
        .await
        .unwrap();

        repo.sync_catalog(&[status("1")]).await.unwrap();

        let mut ids: Vec<String> = repo.get_all().await.unwrap().into_iter().map(|s| s.id).collect();
        ids.sort();
        assert_eq!(ids, vec!["1", "2"]);
    }
}
