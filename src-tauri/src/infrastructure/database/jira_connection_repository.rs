use crate::domain::error::DomainError;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct JiraConnectionRecord {
    pub keyring_key: String,
    pub base_url: String,
    pub email: String,
    pub account_id: String,
    pub display_name: String,
    pub time_zone: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePurge {
    None,
    KeepStatuses,
    All,
}

async fn purge_jira_cache(
    tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>,
    keep_statuses: bool,
) -> Result<(), DomainError> {
    let mut statements = vec![
        "DELETE FROM worklogs",
        "DELETE FROM issue_changes",
        "DELETE FROM jira_issues",
    ];
    if !keep_statuses {
        statements.push("DELETE FROM jira_statuses");
    }
    for statement in statements {
        sqlx::query(statement)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
    }
    log::info!("[DATABASE] Cache local do Jira apagado");
    Ok(())
}

#[derive(Clone)]
pub struct JiraConnectionRepository {
    pool: SqlitePool,
}

impl JiraConnectionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[cfg(test)]
    pub async fn save(
        &self,
        keyring_key: &str,
        base_url: &str,
        email: &str,
        account_id: &str,
        display_name: &str,
        time_zone: &str,
    ) -> Result<(), DomainError> {
        self.save_with_purge(
            keyring_key,
            base_url,
            email,
            account_id,
            display_name,
            time_zone,
            CachePurge::None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_with_purge(
        &self,
        keyring_key: &str,
        base_url: &str,
        email: &str,
        account_id: &str,
        display_name: &str,
        time_zone: &str,
        purge: CachePurge,
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        match purge {
            CachePurge::None => {}
            CachePurge::KeepStatuses => purge_jira_cache(&mut tx, true).await?,
            CachePurge::All => purge_jira_cache(&mut tx, false).await?,
        }
        let previous_time_zone: Option<String> =
            sqlx::query_scalar("SELECT time_zone FROM jira_connection WHERE id = 1")
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?
                .flatten();

        sqlx::query(
            "INSERT INTO jira_connection (id, keyring_key, base_url, email, account_id, display_name, time_zone, is_active)
             VALUES (1, ?, ?, ?, ?, ?, ?, 1)
             ON CONFLICT(id) DO UPDATE SET
                keyring_key = excluded.keyring_key,
                base_url = excluded.base_url,
                email = excluded.email,
                account_id = excluded.account_id,
                display_name = excluded.display_name,
                time_zone = excluded.time_zone,
                is_active = 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(keyring_key)
        .bind(base_url)
        .bind(email)
        .bind(account_id)
        .bind(display_name)
        .bind(time_zone)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        if previous_time_zone
            .as_deref()
            .is_some_and(|previous| previous != time_zone)
        {
            relocate_worklog_dates(&mut tx, time_zone).await?;
        }

        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;
        if purge != CachePurge::None {
            self.truncate_wal().await;
        }
        Ok(())
    }

    pub async fn get_active(&self) -> Result<Option<JiraConnectionRecord>, DomainError> {
        let row = sqlx::query(
            "SELECT keyring_key, base_url, email, account_id, display_name, time_zone
             FROM jira_connection
             WHERE id = 1 AND is_active = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(row.map(|row| JiraConnectionRecord {
            keyring_key: row.get("keyring_key"),
            base_url: row.get("base_url"),
            email: row.get("email"),
            account_id: row.get("account_id"),
            display_name: row.get("display_name"),
            time_zone: row.get("time_zone"),
        }))
    }

    pub async fn get_keyring_key(&self) -> Result<Option<String>, DomainError> {
        let row = sqlx::query("SELECT keyring_key FROM jira_connection WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(row.map(|r| r.get("keyring_key")))
    }

    pub async fn get_identity(&self) -> Result<Option<(String, String)>, DomainError> {
        let row = sqlx::query("SELECT base_url, account_id FROM jira_connection WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(row.map(|r| (r.get("base_url"), r.get("account_id"))))
    }

    pub async fn purge_cached_jira_data(&self, keep_statuses: bool) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        purge_jira_cache(&mut tx, keep_statuses).await?;
        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;
        self.truncate_wal().await;
        Ok(())
    }

    async fn truncate_wal(&self) {
        if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await {
            log::warn!("[DATABASE] Não foi possível compactar o WAL após apagar dados: {e}");
        }
    }

    pub async fn purge_pontomais_data(&self) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM pontomais_entries")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        log::info!("[DATABASE] Batidas do PontoMais apagadas do cache local");
        self.truncate_wal().await;
        Ok(())
    }

    pub async fn activate(&self) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE jira_connection
             SET is_active = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = 1",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn deactivate(&self) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE jira_connection
             SET is_active = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = 1",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn get_time_zone(&self) -> Result<Option<String>, DomainError> {
        let row = sqlx::query("SELECT time_zone FROM jira_connection WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(row.map(|r| r.get("time_zone")))
    }
}

async fn relocate_worklog_dates(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    time_zone: &str,
) -> Result<(), DomainError> {
    let tz = crate::domain::date_range::parse_time_zone(time_zone);
    let rows = sqlx::query("SELECT id, started_at FROM worklogs")
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
    for row in rows {
        let id: i64 = row.get("id");
        let started_at: String = row.get("started_at");
        let Ok(started) = chrono::DateTime::parse_from_rfc3339(&started_at)
            .or_else(|_| chrono::DateTime::parse_from_str(&started_at, "%Y-%m-%dT%H:%M:%S%.f%z"))
        else {
            continue;
        };
        let local_date = started.with_timezone(&tz).date_naive().to_string();
        sqlx::query("UPDATE worklogs SET started_local_date = ? WHERE id = ?")
            .bind(local_date)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
    }
    log::info!("[DATABASE] Fuso do Jira mudou; datas locais dos worklogs recalculadas");
    Ok(())
}

#[cfg(test)]
pub(crate) async fn in_memory_pool() -> SqlitePool {
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
    SqlitePool::connect_with(options).await.unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::migrations::DatabaseMigrator;

    async fn repository() -> JiraConnectionRepository {
        let pool = in_memory_pool().await;
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        JiraConnectionRepository::new(pool)
    }

    async fn repository_with_cache() -> JiraConnectionRepository {
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
            "INSERT INTO jira_statuses (id, name, category_key, category_name, category_color, is_development)
             VALUES ('3', 'Fazendo', 'indeterminate', 'In Progress', 'yellow', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO jira_issues (id, key, summary, status_id, browse_url, jira_updated_at)
             VALUES ('10', 'GAMA-1', 'Tarefa', '3', '', '2026-11-17T12:00:00.000Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        JiraConnectionRepository::new(pool)
    }

    async fn count(repo: &JiraConnectionRepository, table: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(&repo.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn changing_the_time_zone_moves_worklogs_to_their_local_day() {
        let repo = repository_with_cache().await;
        repo.save(
            "k",
            "https://x.atlassian.net",
            "me@x.com",
            "me",
            "Eu",
            "America/Sao_Paulo",
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO worklogs (jira_worklog_id, issue_id, author_account_id, started_at, started_local_date,
                                   time_spent_seconds, state, origin, synced_at)
             VALUES ('1', '10', 'me', '2026-11-18T01:30:00.000Z', '2026-11-17', 3600, 'synced', 'jira', '2026-11-18T02:00:00.000Z')",
        )
        .execute(&repo.pool)
        .await
        .unwrap();

        repo.save("k", "https://x.atlassian.net", "me@x.com", "me", "Eu", "UTC")
            .await
            .unwrap();

        let day: String = sqlx::query_scalar("SELECT started_local_date FROM worklogs WHERE jira_worklog_id = '1'")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(day, "2026-11-18");
    }

    #[tokio::test]
    async fn saving_with_purge_replaces_the_cache_in_the_same_step() {
        let repo = repository_with_cache().await;

        repo.save_with_purge(
            "k",
            "https://y.atlassian.net",
            "me@y.com",
            "other",
            "Outro",
            "UTC",
            CachePurge::All,
        )
        .await
        .unwrap();

        assert_eq!(count(&repo, "jira_issues").await, 0);
        assert_eq!(count(&repo, "jira_statuses").await, 0);
        assert_eq!(repo.get_identity().await.unwrap().unwrap().1, "other");
    }

    #[tokio::test]
    async fn saving_without_purge_keeps_the_cache() {
        let repo = repository_with_cache().await;

        repo.save("k", "https://x.atlassian.net", "me@x.com", "me", "Eu", "UTC")
            .await
            .unwrap();

        assert_eq!(count(&repo, "jira_issues").await, 1);
        assert_eq!(count(&repo, "jira_statuses").await, 1);
    }

    #[tokio::test]
    async fn activate_restores_a_deactivated_connection() {
        let repo = repository_with_cache().await;
        repo.save("k", "https://x.atlassian.net", "me@x.com", "me", "Eu", "UTC")
            .await
            .unwrap();
        repo.deactivate().await.unwrap();
        assert!(repo.get_active().await.unwrap().is_none());

        repo.activate().await.unwrap();

        assert!(repo.get_active().await.unwrap().is_some());
    }

    #[tokio::test]
    async fn purge_keeping_statuses_preserves_the_development_configuration() {
        let repo = repository_with_cache().await;

        repo.purge_cached_jira_data(true).await.unwrap();

        assert_eq!(count(&repo, "jira_issues").await, 0);
        let dev: i64 = sqlx::query_scalar("SELECT is_development FROM jira_statuses WHERE id = '3'")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(dev, 1);
    }

    #[tokio::test]
    async fn purge_for_another_instance_also_removes_the_statuses() {
        let repo = repository_with_cache().await;

        repo.purge_cached_jira_data(false).await.unwrap();

        assert_eq!(count(&repo, "jira_issues").await, 0);
        assert_eq!(count(&repo, "jira_statuses").await, 0);
    }

    #[tokio::test]
    async fn get_active_returns_none_when_nothing_was_saved() {
        let repo = repository().await;
        let result = repo.get_active().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_then_get_round_trips_the_connection_as_active() {
        let repo = repository().await;
        let keyring_key = uuid::Uuid::new_v4().to_string();

        repo.save(
            &keyring_key,
            "https://acme.atlassian.net",
            "dev@acme.com",
            "acc123",
            "Dev User",
            "America/Sao_Paulo",
        )
        .await
        .unwrap();

        let record = repo.get_active().await.unwrap().unwrap();
        assert_eq!(record.keyring_key, keyring_key);
        assert_eq!(record.base_url, "https://acme.atlassian.net");
        assert_eq!(record.email, "dev@acme.com");
        assert_eq!(record.account_id, "acc123");
        assert_eq!(record.display_name, "Dev User");
        assert_eq!(record.time_zone, "America/Sao_Paulo");
    }

    #[tokio::test]
    async fn deactivate_keeps_data_but_clears_active_flag() {
        let repo = repository().await;
        let keyring_key = uuid::Uuid::new_v4().to_string();

        repo.save(
            &keyring_key,
            "https://acme.atlassian.net",
            "dev@acme.com",
            "acc123",
            "Dev User",
            "America/Sao_Paulo",
        )
        .await
        .unwrap();

        repo.deactivate().await.unwrap();

        assert!(repo.get_active().await.unwrap().is_none());

        let row = sqlx::query("SELECT account_id, is_active FROM jira_connection WHERE id = 1")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("account_id"), "acc123");
        assert_eq!(row.get::<i64, _>("is_active"), 0);
    }
}
