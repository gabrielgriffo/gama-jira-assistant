use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::domain::error::DomainError;

const NOW_UTC: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

#[derive(Debug, Clone)]
pub struct SyncedWorklog {
    pub jira_worklog_id: String,
    pub issue_id: String,
    pub author_account_id: String,
    pub started_at: String,
    pub started_local_date: String,
    pub time_spent_seconds: i64,
    pub comment_text: Option<String>,
    pub origin: &'static str,
    pub jira_created_at: Option<String>,
    pub jira_updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredWorklog {
    pub id: i64,
    pub jira_worklog_id: Option<String>,
    pub issue_id: String,
    pub issue_key: String,
    pub issue_summary: String,
    pub issue_browse_url: String,
    pub author_account_id: String,
    pub started_at: String,
    pub started_local_date: String,
    pub time_spent_seconds: i64,
    pub comment_text: Option<String>,
    pub origin: String,
    pub jira_created_at: Option<String>,
    pub jira_updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DailyWorklogTotals {
    pub local_date: String,
    pub synced_seconds: i64,
    pub gama_synced_count: i64,
}

#[derive(Clone)]
pub struct SqliteWorklogRepository {
    pool: SqlitePool,
}

impl SqliteWorklogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_synced(&self, worklogs: &[SyncedWorklog]) -> Result<usize, DomainError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let mut upserted = 0;

        for worklog in worklogs {
            let query = format!(
                "INSERT INTO worklogs (jira_worklog_id, issue_id, author_account_id, started_at, started_local_date,
                                       time_spent_seconds, comment_text, state, origin, jira_created_at, jira_updated_at, synced_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 'synced', ?, ?, ?, {NOW_UTC})
                 ON CONFLICT(jira_worklog_id) DO UPDATE SET
                    issue_id = excluded.issue_id,
                    author_account_id = excluded.author_account_id,
                    started_at = excluded.started_at,
                    started_local_date = excluded.started_local_date,
                    time_spent_seconds = excluded.time_spent_seconds,
                    comment_text = excluded.comment_text,
                    jira_created_at = excluded.jira_created_at,
                    jira_updated_at = excluded.jira_updated_at,
                    updated_at = {NOW_UTC},
                    synced_at = {NOW_UTC}"
            );

            sqlx::query(sqlx::AssertSqlSafe(query))
                .bind(&worklog.jira_worklog_id)
                .bind(&worklog.issue_id)
                .bind(&worklog.author_account_id)
                .bind(&worklog.started_at)
                .bind(&worklog.started_local_date)
                .bind(worklog.time_spent_seconds)
                .bind(&worklog.comment_text)
                .bind(worklog.origin)
                .bind(&worklog.jira_created_at)
                .bind(&worklog.jira_updated_at)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            upserted += 1;
        }

        tx.commit().await.map_err(storage)?;
        Ok(upserted)
    }

    pub async fn delete_synced_missing(
        &self,
        author_account_id: &str,
        from_date: &str,
        to_date: &str,
        keep_jira_ids: &[String],
    ) -> Result<u64, DomainError> {
        let mut query = String::from(
            "DELETE FROM worklogs
             WHERE state = 'synced'
               AND author_account_id = ?
               AND started_local_date >= ?
               AND started_local_date <= ?",
        );
        if !keep_jira_ids.is_empty() {
            let placeholders = keep_jira_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND jira_worklog_id NOT IN ({placeholders})"));
        }

        let mut statement = sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(author_account_id)
            .bind(from_date)
            .bind(to_date);
        for id in keep_jira_ids {
            statement = statement.bind(id);
        }

        let result = statement.execute(&self.pool).await.map_err(storage)?;
        Ok(result.rows_affected())
    }

    pub async fn get_synced_for_author(
        &self,
        author_account_id: &str,
        from_date: &str,
        to_date: &str,
    ) -> Result<Vec<StoredWorklog>, DomainError> {
        let rows = sqlx::query(
            "SELECT w.id, w.jira_worklog_id, w.issue_id, i.key AS issue_key, i.summary AS issue_summary, i.browse_url AS issue_browse_url,
                    w.author_account_id, w.started_at, w.started_local_date, w.time_spent_seconds, w.comment_text,
                    w.origin, w.jira_created_at, w.jira_updated_at
             FROM worklogs w
             JOIN jira_issues i ON i.id = w.issue_id
             WHERE w.state = 'synced'
               AND w.author_account_id = ?
               AND w.started_local_date >= ?
               AND w.started_local_date <= ?
             ORDER BY w.started_at",
        )
        .bind(author_account_id)
        .bind(from_date)
        .bind(to_date)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;

        Ok(rows.into_iter().map(map_stored).collect())
    }

    pub async fn daily_totals(
        &self,
        author_account_id: &str,
        from_date: &str,
        to_date: &str,
    ) -> Result<Vec<DailyWorklogTotals>, DomainError> {
        let rows = sqlx::query(
            "SELECT started_local_date,
                    SUM(time_spent_seconds) AS synced_seconds,
                    SUM(CASE WHEN origin = 'gama' THEN 1 ELSE 0 END) AS gama_synced_count
             FROM worklogs
             WHERE state = 'synced'
               AND author_account_id = ?
               AND started_local_date >= ?
               AND started_local_date <= ?
             GROUP BY started_local_date",
        )
        .bind(author_account_id)
        .bind(from_date)
        .bind(to_date)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;

        Ok(rows
            .into_iter()
            .map(|row| DailyWorklogTotals {
                local_date: row.get("started_local_date"),
                synced_seconds: row.get("synced_seconds"),
                gama_synced_count: row.get("gama_synced_count"),
            })
            .collect())
    }

    pub async fn get_sync_timestamp(&self, author_account_id: &str) -> Result<Option<String>, DomainError> {
        let row = sqlx::query(
            "SELECT MAX(synced_at) AS last_sync FROM worklogs WHERE state = 'synced' AND author_account_id = ?",
        )
        .bind(author_account_id)
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.get("last_sync"))
    }
}

fn storage(e: sqlx::Error) -> DomainError {
    DomainError::Storage(e.to_string())
}

fn map_stored(row: SqliteRow) -> StoredWorklog {
    StoredWorklog {
        id: row.get("id"),
        jira_worklog_id: row.get("jira_worklog_id"),
        issue_id: row.get("issue_id"),
        issue_key: row.get("issue_key"),
        issue_summary: row.get("issue_summary"),
        issue_browse_url: row.get("issue_browse_url"),
        author_account_id: row.get("author_account_id"),
        started_at: row.get("started_at"),
        started_local_date: row.get("started_local_date"),
        time_spent_seconds: row.get("time_spent_seconds"),
        comment_text: row.get("comment_text"),
        origin: row.get("origin"),
        jira_created_at: row.get("jira_created_at"),
        jira_updated_at: row.get("jira_updated_at"),
    }
}
