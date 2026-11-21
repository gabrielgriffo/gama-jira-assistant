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
    pub issue_last_change_at: Option<String>,
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
    pub confirmed_seconds: i64,
    pub unconfirmed_seconds: i64,
    pub gama_synced_count: i64,
}

pub struct SyncedCleanup<'a> {
    pub author_account_id: &'a str,
    pub from_date: &'a str,
    pub to_date: &'a str,
    pub keep_jira_ids: &'a [String],
    pub skip_issue_ids: &'a [String],
    pub updated_before: &'a str,
}

#[derive(Clone)]
pub struct SqliteWorklogRepository {
    pool: SqlitePool,
}

impl SqliteWorklogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[cfg(test)]
    pub(crate) async fn for_tests() -> Self {
        use crate::infrastructure::database::migrations::DatabaseMigrator;
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::str::FromStr;

        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        sqlx::query(
            "INSERT INTO jira_statuses (id, name, category_key, category_name, category_color)
             VALUES ('1', 'Fazendo', 'indeterminate', 'In Progress', 'yellow')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO jira_issues (id, key, summary, status_id, browse_url, jira_updated_at)
             VALUES ('10', 'GAMA-1', 'Tarefa', '1', '', '2026-11-17T12:00:00.000Z'),
                    ('20', 'GAMA-2', 'Outra', '1', '', '2026-11-17T12:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        Self::new(pool)
    }

    pub async fn upsert_synced(&self, worklogs: &[SyncedWorklog]) -> Result<usize, DomainError> {
        self.upsert(worklogs, true).await
    }

    pub async fn insert_unconfirmed(&self, worklog: &SyncedWorklog) -> Result<(), DomainError> {
        self.upsert(std::slice::from_ref(worklog), false).await.map(|_| ())
    }

    async fn upsert(&self, worklogs: &[SyncedWorklog], confirmed: bool) -> Result<usize, DomainError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let mut upserted = 0;
        let synced_at = if confirmed { NOW_UTC } else { "NULL" };
        let on_conflict = if confirmed {
            format!(
                "DO UPDATE SET
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
            )
        } else {
            "DO NOTHING".to_string()
        };

        for worklog in worklogs {
            let query = format!(
                "INSERT INTO worklogs (jira_worklog_id, issue_id, author_account_id, started_at, started_local_date,
                                       time_spent_seconds, comment_text, state, origin, jira_created_at, jira_updated_at, synced_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 'synced', ?, ?, ?, {synced_at})
                 ON CONFLICT(jira_worklog_id) {on_conflict}"
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

    pub async fn delete_synced_missing(&self, cleanup: &SyncedCleanup<'_>) -> Result<u64, DomainError> {
        let mut query = String::from(
            "DELETE FROM worklogs
             WHERE state = 'synced'
               AND author_account_id = ?
               AND started_local_date >= ?
               AND started_local_date <= ?
               AND updated_at < ?",
        );
        if !cleanup.keep_jira_ids.is_empty() {
            query.push_str(&format!(
                " AND jira_worklog_id NOT IN ({})",
                placeholders(cleanup.keep_jira_ids.len())
            ));
        }
        if !cleanup.skip_issue_ids.is_empty() {
            query.push_str(&format!(
                " AND issue_id NOT IN ({})",
                placeholders(cleanup.skip_issue_ids.len())
            ));
        }

        let mut statement = sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(cleanup.author_account_id)
            .bind(cleanup.from_date)
            .bind(cleanup.to_date)
            .bind(cleanup.updated_before);
        for id in cleanup.keep_jira_ids.iter().chain(cleanup.skip_issue_ids) {
            statement = statement.bind(id);
        }

        let result = statement.execute(&self.pool).await.map_err(storage)?;
        Ok(result.rows_affected())
    }

    pub async fn issue_ids_with_worklogs(
        &self,
        author_account_id: &str,
        from_date: &str,
        to_date: &str,
    ) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            "SELECT DISTINCT issue_id FROM worklogs
             WHERE state = 'synced'
               AND author_account_id = ?
               AND started_local_date >= ?
               AND started_local_date <= ?",
        )
        .bind(author_account_id)
        .bind(from_date)
        .bind(to_date)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        Ok(rows.into_iter().map(|row| row.get("issue_id")).collect())
    }

    pub async fn has_jira_worklog(&self, jira_worklog_id: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT EXISTS(SELECT 1 FROM worklogs WHERE jira_worklog_id = ?) AS found")
            .bind(jira_worklog_id)
            .fetch_one(&self.pool)
            .await
            .map_err(storage)?;
        Ok(row.get::<i64, _>("found") == 1)
    }

    pub async fn adopt_as_gama(&self, jira_worklog_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE worklogs SET origin = 'gama', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE jira_worklog_id = ? AND origin = 'jira'",
        )
        .bind(jira_worklog_id)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(result.rows_affected() > 0)
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
                    w.origin, w.jira_created_at, w.jira_updated_at,
                    (SELECT MAX(c.jira_created_at) FROM issue_changes c
                     WHERE c.issue_id = i.id AND c.field_id IN ('status', 'assignee')) AS issue_last_change_at
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
                    SUM(CASE WHEN synced_at IS NOT NULL THEN time_spent_seconds ELSE 0 END) AS confirmed_seconds,
                    SUM(CASE WHEN synced_at IS NULL THEN time_spent_seconds ELSE 0 END) AS unconfirmed_seconds,
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
                confirmed_seconds: row.get("confirmed_seconds"),
                unconfirmed_seconds: row.get("unconfirmed_seconds"),
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

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(",")
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
        issue_last_change_at: row.get("issue_last_change_at"),
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn repository() -> SqliteWorklogRepository {
        SqliteWorklogRepository::for_tests().await
    }

    fn worklog(id: &str, seconds: i64, origin: &'static str) -> SyncedWorklog {
        SyncedWorklog {
            jira_worklog_id: id.to_string(),
            issue_id: "10".to_string(),
            author_account_id: "me".to_string(),
            started_at: "2026-11-17T11:00:00.000Z".to_string(),
            started_local_date: "2026-11-17".to_string(),
            time_spent_seconds: seconds,
            comment_text: None,
            origin,
            jira_created_at: None,
            jira_updated_at: None,
        }
    }

    async fn totals(repo: &SqliteWorklogRepository) -> (i64, i64) {
        let day = repo.daily_totals("me", "2026-11-17", "2026-11-17").await.unwrap();
        assert_eq!(day.len(), 1);
        (day[0].confirmed_seconds, day[0].gama_synced_count)
    }

    fn on_issue(mut worklog: SyncedWorklog, issue_id: &str) -> SyncedWorklog {
        worklog.issue_id = issue_id.to_string();
        worklog
    }

    async fn stored_ids(repo: &SqliteWorklogRepository) -> Vec<String> {
        let mut ids: Vec<String> = repo
            .get_synced_for_author("me", "2026-11-17", "2026-11-17")
            .await
            .unwrap()
            .into_iter()
            .filter_map(|w| w.jira_worklog_id)
            .collect();
        ids.sort();
        ids
    }

    async fn cleanup(repo: &SqliteWorklogRepository, keep: &[&str], skip_issues: &[&str], updated_before: &str) {
        let keep: Vec<String> = keep.iter().map(|s| s.to_string()).collect();
        let skip: Vec<String> = skip_issues.iter().map(|s| s.to_string()).collect();
        repo.delete_synced_missing(&SyncedCleanup {
            author_account_id: "me",
            from_date: "2026-11-17",
            to_date: "2026-11-17",
            keep_jira_ids: &keep,
            skip_issue_ids: &skip,
            updated_before,
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cleanup_removes_worklogs_missing_from_jira_written_before_the_sync_started() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("1", 600, "jira"), worklog("2", 600, "jira")])
            .await
            .unwrap();

        cleanup(&repo, &["1"], &[], "9999-01-01T00:00:00.000Z").await;

        assert_eq!(stored_ids(&repo).await, vec!["1"]);
    }

    #[tokio::test]
    async fn cleanup_keeps_a_worklog_sent_while_the_sync_was_running() {
        let repo = repository().await;
        repo.insert_unconfirmed(&worklog("3", 600, "gama")).await.unwrap();

        cleanup(&repo, &[], &[], "2000-01-01T00:00:00.000Z").await;

        assert_eq!(stored_ids(&repo).await, vec!["3"]);
        assert_eq!(totals(&repo).await, (0, 1));
    }

    #[tokio::test]
    async fn cleanup_keeps_worklogs_of_issues_that_could_not_be_checked() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("4", 600, "jira"), on_issue(worklog("5", 600, "jira"), "20")])
            .await
            .unwrap();

        cleanup(&repo, &[], &["20"], "9999-01-01T00:00:00.000Z").await;

        assert_eq!(stored_ids(&repo).await, vec!["5"]);
    }

    #[tokio::test]
    async fn lists_issues_with_worklogs_and_finds_known_jira_ids() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("6", 600, "jira"), on_issue(worklog("7", 600, "jira"), "20")])
            .await
            .unwrap();

        let mut issues = repo
            .issue_ids_with_worklogs("me", "2026-11-17", "2026-11-17")
            .await
            .unwrap();
        issues.sort();
        assert_eq!(issues, vec!["10", "20"]);
        assert!(repo.has_jira_worklog("6").await.unwrap());
        assert!(!repo.has_jira_worklog("8").await.unwrap());
    }

    #[tokio::test]
    async fn daily_totals_report_time_still_waiting_for_confirmation() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("9", 1200, "jira")]).await.unwrap();
        repo.insert_unconfirmed(&worklog("11", 600, "gama")).await.unwrap();

        let day = repo.daily_totals("me", "2026-11-17", "2026-11-17").await.unwrap();
        assert_eq!((day[0].confirmed_seconds, day[0].unconfirmed_seconds), (1200, 600));
    }

    #[tokio::test]
    async fn sent_worklog_does_not_count_as_in_jira_until_confirmed_by_a_fetch() {
        let repo = repository().await;
        repo.insert_unconfirmed(&worklog("100", 3600, "gama")).await.unwrap();
        assert_eq!(totals(&repo).await, (0, 1));

        repo.upsert_synced(&[worklog("100", 3600, "jira")]).await.unwrap();
        assert_eq!(totals(&repo).await, (3600, 1));
    }

    #[tokio::test]
    async fn worklogs_fetched_from_jira_count_as_in_jira() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("200", 1800, "jira")]).await.unwrap();
        assert_eq!(totals(&repo).await, (1800, 0));
    }

    #[tokio::test]
    async fn stored_worklog_carries_the_latest_status_or_assignee_change_of_its_issue() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("400", 600, "jira")]).await.unwrap();
        for (id, at, field) in [
            ("c1", "2026-11-10T10:00:00.000Z", "status"),
            ("c2", "2026-11-12T10:00:00.000Z", "assignee"),
            ("c3", "2026-11-15T10:00:00.000Z", "description"),
        ] {
            sqlx::query(
                "INSERT INTO issue_changes (changelog_id, item_index, issue_id, jira_created_at, field_id, field, field_type)
                 VALUES (?, 0, '10', ?, ?, ?, 'jira')",
            )
            .bind(id)
            .bind(at)
            .bind(field)
            .bind(field)
            .execute(&repo.pool)
            .await
            .unwrap();
        }

        let stored = repo
            .get_synced_for_author("me", "2026-11-17", "2026-11-17")
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].issue_last_change_at.as_deref(),
            Some("2026-11-12T10:00:00.000Z")
        );
    }

    #[tokio::test]
    async fn unconfirmed_insert_keeps_an_already_confirmed_worklog_confirmed() {
        let repo = repository().await;
        repo.upsert_synced(&[worklog("300", 1200, "jira")]).await.unwrap();
        repo.insert_unconfirmed(&worklog("300", 1200, "gama")).await.unwrap();
        assert_eq!(totals(&repo).await, (1200, 0));
    }
}
