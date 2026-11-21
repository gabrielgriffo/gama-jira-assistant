use crate::domain::error::DomainError;
use crate::domain::pontomais::workday::TimeCard;
use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct PontoMaisEntryRepository {
    pool: SqlitePool,
}

impl PontoMaisEntryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn replace_range(&self, from_date: &str, to_date: &str, entries: &[TimeCard]) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;

        for entry in entries {
            sqlx::query(
                "INSERT INTO pontomais_entries (id, local_date, local_time, disabled, synced_at)
                 VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(id) DO UPDATE SET
                    local_date = excluded.local_date,
                    local_time = excluded.local_time,
                    disabled = excluded.disabled,
                    synced_at = excluded.synced_at",
            )
            .bind(entry.id as i64)
            .bind(&entry.date)
            .bind(&entry.time)
            .bind(entry.disabled as i32)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }

        let mut query_str = String::from(
            "DELETE FROM pontomais_entries
             WHERE local_date >= ? AND local_date <= ?",
        );
        if !entries.is_empty() {
            let placeholders = vec!["?"; entries.len()].join(",");
            query_str.push_str(&format!(" AND id NOT IN ({placeholders})"));
        }
        let mut query = sqlx::query(sqlx::AssertSqlSafe(query_str))
            .bind(from_date)
            .bind(to_date);
        for entry in entries {
            query = query.bind(entry.id as i64);
        }
        query.execute(&mut *tx).await.map_err(storage)?;

        tx.commit().await.map_err(storage)?;
        Ok(())
    }

    pub async fn get_by_date_range(&self, from_date: &str, to_date: &str) -> Result<Vec<TimeCard>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, local_date, local_time, disabled
             FROM pontomais_entries
             WHERE local_date >= ? AND local_date <= ?
             ORDER BY local_date, local_time",
        )
        .bind(from_date)
        .bind(to_date)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let id_i64: i64 = row.get("id");
                let disabled_i32: i32 = row.get("disabled");
                TimeCard {
                    id: id_i64 as u64,
                    date: row.get("local_date"),
                    time: row.get("local_time"),
                    disabled: disabled_i32 != 0,
                }
            })
            .collect())
    }
}

fn storage(e: sqlx::Error) -> DomainError {
    DomainError::Storage(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn repository() -> PontoMaisEntryRepository {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        PontoMaisEntryRepository::new(pool)
    }

    fn card(id: u64, date: &str, time: &str, disabled: bool) -> TimeCard {
        TimeCard {
            id,
            date: date.to_string(),
            time: time.to_string(),
            disabled,
        }
    }

    async fn stored(repo: &PontoMaisEntryRepository) -> Vec<(u64, String, bool)> {
        repo.get_by_date_range("2026-01-01", "2026-12-31")
            .await
            .unwrap()
            .into_iter()
            .map(|c| (c.id, c.date, c.disabled))
            .collect()
    }

    #[tokio::test]
    async fn reimport_applies_a_card_disregarded_later_in_pontomais() {
        let repo = repository().await;
        repo.replace_range("2026-11-17", "2026-11-17", &[card(1, "2026-11-17", "08:00", false)])
            .await
            .unwrap();

        repo.replace_range("2026-11-17", "2026-11-17", &[card(1, "2026-11-17", "08:00", true)])
            .await
            .unwrap();

        assert_eq!(stored(&repo).await, vec![(1, "2026-11-17".to_string(), true)]);
    }

    #[tokio::test]
    async fn reimport_removes_the_cards_of_a_day_left_without_any_card() {
        let repo = repository().await;
        repo.replace_range(
            "2026-11-16",
            "2026-11-17",
            &[
                card(1, "2026-11-16", "08:00", false),
                card(2, "2026-11-17", "08:00", false),
            ],
        )
        .await
        .unwrap();

        repo.replace_range("2026-11-16", "2026-11-17", &[card(1, "2026-11-16", "08:00", false)])
            .await
            .unwrap();

        assert_eq!(stored(&repo).await, vec![(1, "2026-11-16".to_string(), false)]);
    }

    #[tokio::test]
    async fn reimport_of_an_empty_period_clears_only_that_period() {
        let repo = repository().await;
        repo.replace_range(
            "2026-11-10",
            "2026-11-17",
            &[
                card(1, "2026-11-10", "08:00", false),
                card(2, "2026-11-17", "08:00", false),
            ],
        )
        .await
        .unwrap();

        repo.replace_range("2026-11-17", "2026-11-17", &[]).await.unwrap();

        assert_eq!(stored(&repo).await, vec![(1, "2026-11-10".to_string(), false)]);
    }

    #[tokio::test]
    async fn reimport_moves_a_card_corrected_to_another_day() {
        let repo = repository().await;
        repo.replace_range("2026-11-16", "2026-11-17", &[card(1, "2026-11-16", "18:00", false)])
            .await
            .unwrap();

        repo.replace_range("2026-11-16", "2026-11-17", &[card(1, "2026-11-17", "18:00", false)])
            .await
            .unwrap();

        assert_eq!(stored(&repo).await, vec![(1, "2026-11-17".to_string(), false)]);
    }
}
