use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

pub struct DatabaseMigrator {
    pool: SqlitePool,
}

impl DatabaseMigrator {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        let current_version = self.get_current_version().await?;
        log::info!("Database version: {}", current_version);

        if current_version < 1 {
            self.create_schema().await?;
        }

        Ok(())
    }

    async fn get_current_version(&self) -> Result<i32, Box<dyn std::error::Error>> {
        let row = sqlx::query("PRAGMA user_version").fetch_one(&self.pool).await?;
        Ok(row.get(0))
    }

    async fn set_version(&self, version: i32) -> Result<(), Box<dyn std::error::Error>> {
        use sqlx::AssertSqlSafe;
        let query_str = format!("PRAGMA user_version = {}", version);
        sqlx::query(AssertSqlSafe(query_str.as_str()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_schema(&self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Creating database schema");

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "CREATE TABLE jira_connection (
                id                      INTEGER PRIMARY KEY CHECK (id = 1),
                keyring_key             TEXT NOT NULL,
                base_url                TEXT NOT NULL,
                email                   TEXT NOT NULL,
                account_id              TEXT NOT NULL,
                display_name            TEXT NOT NULL,
                time_zone               TEXT NOT NULL,
                is_active               INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
                worklogs_updated_until  INTEGER,
                worklogs_deleted_until  INTEGER,
                created_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE jira_statuses (
                id                 TEXT PRIMARY KEY,
                name               TEXT NOT NULL,
                untranslated_name  TEXT,
                category_key       TEXT NOT NULL,
                category_name      TEXT NOT NULL,
                category_color     TEXT NOT NULL,
                icon_url           TEXT,
                is_development     INTEGER NOT NULL DEFAULT 0 CHECK (is_development IN (0, 1)),
                synced_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("CREATE INDEX idx_jira_statuses_category_key ON jira_statuses(category_key)")
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "CREATE TABLE jira_issues (
                id                     TEXT PRIMARY KEY,
                key                    TEXT NOT NULL UNIQUE,
                summary                TEXT NOT NULL,
                status_id              TEXT NOT NULL REFERENCES jira_statuses(id),
                assignee_account_id    TEXT,
                assignee_display_name  TEXT,
                priority_name          TEXT,
                issue_type_name        TEXT,
                due_date               TEXT,
                browse_url             TEXT NOT NULL,
                jira_updated_at        TEXT NOT NULL,
                synced_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX idx_jira_issues_assignee_updated ON jira_issues(assignee_account_id, jira_updated_at)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("CREATE INDEX idx_jira_issues_status ON jira_issues(status_id)")
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "CREATE TABLE issue_changes (
                changelog_id         TEXT NOT NULL,
                item_index           INTEGER NOT NULL,
                issue_id             TEXT NOT NULL REFERENCES jira_issues(id) ON DELETE CASCADE,
                author_account_id    TEXT,
                author_display_name  TEXT,
                jira_created_at      TEXT NOT NULL,
                field_id             TEXT,
                field                TEXT NOT NULL,
                field_type           TEXT NOT NULL,
                from_id              TEXT,
                from_string          TEXT,
                to_id                TEXT,
                to_string            TEXT,
                PRIMARY KEY (changelog_id, item_index)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX idx_issue_changes_issue_field
             ON issue_changes(issue_id, field_id, jira_created_at)",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE worklogs (
                id                  INTEGER PRIMARY KEY,
                jira_worklog_id     TEXT UNIQUE,
                issue_id            TEXT NOT NULL REFERENCES jira_issues(id),
                author_account_id   TEXT NOT NULL,
                started_at          TEXT NOT NULL,
                started_local_date  TEXT NOT NULL,
                time_spent_seconds  INTEGER NOT NULL CHECK (time_spent_seconds >= 0),
                comment_text        TEXT,
                state               TEXT NOT NULL CHECK (state IN ('pending', 'sending', 'failed', 'synced')),
                origin              TEXT NOT NULL CHECK (origin IN ('gama', 'jira')),
                attempts            INTEGER NOT NULL DEFAULT 0,
                last_error          TEXT,
                jira_created_at     TEXT,
                jira_updated_at     TEXT,
                created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                synced_at           TEXT,
                CHECK ((state = 'synced') = (jira_worklog_id IS NOT NULL)),
                CHECK (state = 'synced' OR time_spent_seconds >= 60)
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("CREATE INDEX idx_worklogs_author_date ON worklogs(author_account_id, started_local_date)")
            .execute(&mut *tx)
            .await?;

        sqlx::query("CREATE INDEX idx_worklogs_issue ON worklogs(issue_id)")
            .execute(&mut *tx)
            .await?;

        sqlx::query("CREATE INDEX idx_worklogs_unsent ON worklogs(state) WHERE state <> 'synced'")
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX idx_worklogs_unsent_unique
             ON worklogs(issue_id, started_at) WHERE state <> 'synced'",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE TABLE pontomais_entries (
                id          INTEGER PRIMARY KEY,
                local_date  TEXT NOT NULL,
                local_time  TEXT NOT NULL,
                disabled    INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
                synced_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("CREATE INDEX idx_pontomais_entries_date ON pontomais_entries(local_date, local_time)")
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        self.set_version(1).await?;
        log::info!("Database schema created");
        Ok(())
    }

    #[cfg(test)]
    pub async fn configure_connection(&self) -> Result<(), Box<dyn std::error::Error>> {
        sqlx::query("PRAGMA foreign_keys = ON").execute(&self.pool).await?;
        sqlx::query("PRAGMA journal_mode = WAL").execute(&self.pool).await?;
        sqlx::query("PRAGMA busy_timeout = 5000").execute(&self.pool).await?;
        sqlx::query("PRAGMA synchronous = NORMAL").execute(&self.pool).await?;
        Ok(())
    }
}

pub fn connect_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .pragma("secure_delete", "ON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    #[tokio::test]
    async fn creates_final_schema_once() {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        migrator.run_migrations().await.unwrap();

        assert_eq!(migrator.get_current_version().await.unwrap(), 1);

        let columns: Vec<String> = sqlx::query("SELECT name FROM pragma_table_info('jira_issues')")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get("name"))
            .collect();
        assert!(columns.contains(&"browse_url".to_string()));
        assert!(!columns.contains(&"project_key".to_string()));

        let indexes: Vec<String> = sqlx::query("SELECT name FROM pragma_index_list('jira_issues')")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get("name"))
            .collect();
        assert!(indexes.contains(&"idx_jira_issues_assignee_updated".to_string()));
    }
}
