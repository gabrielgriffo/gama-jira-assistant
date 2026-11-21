use super::client::PontoMaisClient;
use super::dto::{PontoMaisTimeCardDto, PontoMaisWorkDaysResponse};
use crate::domain::error::DomainError;
use crate::domain::pontomais::credentials::PontoMaisCredentials;
use crate::domain::pontomais::workday::{TimeCard, WorkDay};

#[derive(Clone)]
pub struct PontoMaisProvider {
    client: PontoMaisClient,
}

impl PontoMaisProvider {
    pub fn new(client: PontoMaisClient) -> Self {
        Self { client }
    }

    pub async fn get_work_days(
        &self,
        credentials: &PontoMaisCredentials,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<WorkDay>, DomainError> {
        let query = [
            ("start_date", start_date),
            ("end_date", end_date),
            ("attributes", "time_cards"),
        ];

        let response: PontoMaisWorkDaysResponse = self
            .client
            .get_json(credentials, "/api/time_cards/work_days/current", &query)
            .await?;

        let work_days = response
            .work_days
            .into_iter()
            .filter_map(|day_dto| {
                if day_dto.time_cards.is_empty() {
                    return None;
                }

                let date = Self::normalize_date_format(&day_dto.time_cards[0].date);
                let time_cards: Vec<TimeCard> = day_dto.time_cards.into_iter().map(Self::map_time_card).collect();

                Some(WorkDay::new(date, time_cards))
            })
            .collect();

        Ok(work_days)
    }

    fn map_time_card(dto: PontoMaisTimeCardDto) -> TimeCard {
        TimeCard {
            id: dto.id,
            time: dto.time,
            disabled: dto.disabled,
            date: Self::normalize_date_format(&dto.date),
        }
    }

    fn normalize_date_format(date_br: &str) -> String {
        if let Some((day, rest)) = date_br.split_once('/') {
            if let Some((month, year)) = rest.split_once('/') {
                return format!("{}-{}-{}", year, month, day);
            }
        }
        date_br.to_string()
    }
}

impl Default for PontoMaisProvider {
    fn default() -> Self {
        Self::new(PontoMaisClient::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_date_format_converts_br_to_iso() {
        assert_eq!(PontoMaisProvider::normalize_date_format("17/09/2026"), "2026-09-17");
        assert_eq!(PontoMaisProvider::normalize_date_format("01/01/2024"), "2024-01-01");
        assert_eq!(PontoMaisProvider::normalize_date_format("31/12/2025"), "2025-12-31");
    }

    #[test]
    fn normalize_date_format_returns_original_if_invalid() {
        assert_eq!(PontoMaisProvider::normalize_date_format("invalid"), "invalid");
        assert_eq!(PontoMaisProvider::normalize_date_format("2026-09-17"), "2026-09-17");
    }
}
