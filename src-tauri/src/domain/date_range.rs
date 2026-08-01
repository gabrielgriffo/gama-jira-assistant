use chrono::{DateTime, LocalResult, NaiveDate, NaiveTime, TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;

use super::error::DomainError;

const DATE_FORMAT: &str = "%Y-%m-%d";
const UTC_TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl DateRange {
    pub fn parse(from: &str, to: &str) -> Result<Self, DomainError> {
        let from = parse_date(from)?;
        let to = parse_date(to)?;
        if from > to {
            return Err(DomainError::ParseError(format!(
                "período inválido: {from} é depois de {to}"
            )));
        }
        Ok(Self { from, to })
    }

    pub fn end_exclusive(&self) -> NaiveDate {
        self.to.succ_opt().unwrap_or(self.to)
    }

    pub fn utc_bounds(&self, tz: Tz) -> (String, String) {
        (
            format_utc(start_of_day_utc(self.from, tz)),
            format_utc(start_of_day_utc(self.end_exclusive(), tz)),
        )
    }

    pub fn reaches_today(&self, tz: Tz) -> bool {
        self.to >= Utc::now().with_timezone(&tz).date_naive()
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, DomainError> {
    NaiveDate::parse_from_str(value, DATE_FORMAT)
        .map_err(|_| DomainError::ParseError(format!("data inválida: {value}")))
}

pub fn parse_time_zone(value: &str) -> Tz {
    value.parse().unwrap_or_else(|_| {
        log::warn!("[DATE] Fuso horário inválido '{}', usando UTC", value);
        Tz::UTC
    })
}

pub fn start_of_day_utc(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let midnight = date.and_time(NaiveTime::MIN);
    let local = match tz.from_local_datetime(&midnight) {
        LocalResult::Single(t) => t,
        LocalResult::Ambiguous(earliest, _) => earliest,
        LocalResult::None => tz
            .from_local_datetime(&(midnight + TimeDelta::hours(1)))
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&midnight)),
    };
    local.with_timezone(&Utc)
}

pub fn format_utc(value: DateTime<Utc>) -> String {
    value.format(UTC_TIMESTAMP_FORMAT).to_string()
}

pub fn normalize_jira_timestamp(value: &str) -> String {
    DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z")
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .map(|t| format_utc(t.with_timezone(&Utc)))
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, DATE_FORMAT).unwrap()
    }

    #[test]
    fn parse_rejects_invalid_format() {
        assert!(DateRange::parse("2026-13-01", "2026-10-05").is_err());
        assert!(DateRange::parse("01/10/2026", "2026-10-05").is_err());
    }

    #[test]
    fn parse_rejects_inverted_range() {
        assert!(DateRange::parse("2026-10-06", "2026-10-05").is_err());
    }

    #[test]
    fn end_exclusive_is_next_day() {
        let range = DateRange::parse("2026-10-01", "2026-10-05").unwrap();
        assert_eq!(range.end_exclusive(), date("2026-10-06"));
    }

    #[test]
    fn start_of_day_in_sao_paulo() {
        let tz: Tz = "America/Sao_Paulo".parse().unwrap();
        assert_eq!(
            format_utc(start_of_day_utc(date("2026-10-01"), tz)),
            "2026-10-01T03:00:00.000Z"
        );
    }

    #[test]
    fn start_of_day_when_midnight_does_not_exist() {
        let tz: Tz = "America/Sao_Paulo".parse().unwrap();
        assert_eq!(
            format_utc(start_of_day_utc(date("2018-11-04"), tz)),
            "2018-11-04T03:00:00.000Z"
        );
    }

    #[test]
    fn utc_bounds_use_exclusive_end() {
        let tz: Tz = "America/Sao_Paulo".parse().unwrap();
        let range = DateRange::parse("2026-10-01", "2026-10-05").unwrap();
        assert_eq!(
            range.utc_bounds(tz),
            (
                "2026-10-01T03:00:00.000Z".to_string(),
                "2026-10-06T03:00:00.000Z".to_string()
            )
        );
    }

    #[test]
    fn normalizes_jira_timestamp_to_utc() {
        assert_eq!(
            normalize_jira_timestamp("2026-10-05T22:15:30.123-0300"),
            "2026-10-06T01:15:30.123Z"
        );
        assert_eq!(
            normalize_jira_timestamp("2026-10-06T01:15:30.123Z"),
            "2026-10-06T01:15:30.123Z"
        );
        assert_eq!(normalize_jira_timestamp("invalid"), "invalid");
    }
}
