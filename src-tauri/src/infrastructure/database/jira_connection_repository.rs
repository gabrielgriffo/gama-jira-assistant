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

    pub async fn save(
        &self,
        keyring_key: &str,
        base_url: &str,
        email: &str,
        account_id: &str,
        display_name: &str,
        time_zone: &str,
    ) -> Result<(), DomainError> {
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
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;
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

    pub async fn purge_cached_jira_data(&self) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        for statement in [
            "DELETE FROM worklogs",
            "DELETE FROM issue_changes",
            "DELETE FROM jira_issues",
            "DELETE FROM jira_statuses",
        ] {
            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Storage(e.to_string()))?;
        }
        tx.commit().await.map_err(|e| DomainError::Storage(e.to_string()))?;
        log::info!("[DATABASE] Cache local do Jira apagado");
        Ok(())
    }

    pub async fn purge_pontomais_data(&self) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM pontomais_entries")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        log::info!("[DATABASE] Batidas do PontoMais apagadas do cache local");
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
