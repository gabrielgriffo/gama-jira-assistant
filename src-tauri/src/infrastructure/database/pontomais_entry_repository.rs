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

    pub async fn save_for_date(&self, date: &str, entries: &[TimeCard]) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        for entry in entries {
            sqlx::query(
                "INSERT INTO pontomais_entries (id, local_date, local_time, disabled, synced_at)
                 VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(id) DO UPDATE SET
                    local_date = excluded.local_date,
                    local_time = excluded.local_time,
                    synced_at = excluded.synced_at",
            )
            .bind(entry.id as i64)
            .bind(&entry.date)
            .bind(&entry.time)
            .bind(entry.disabled as i32)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?;
        }

        let entry_ids: Vec<i64> = entries.iter().map(|e| e.id as i64).collect();
        if !entry_ids.is_empty() {
            use sqlx::AssertSqlSafe;

            let placeholders = entry_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query_str = format!(
                "DELETE FROM pontomais_entries
                 WHERE local_date = ?
                   AND id NOT IN ({})",
                placeholders
            );

            let mut query = sqlx::query(AssertSqlSafe(query_str.as_str())).bind(date);
            for id in &entry_ids {
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
