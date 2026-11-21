use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkDay {
    pub date: String,
    pub time_cards: Vec<TimeCard>,
    pub total_minutes: u32,
    pub total_hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeCard {
    pub id: u64,
    pub time: String,
    pub disabled: bool,
    pub date: String,
}

impl WorkDay {
    pub fn new(date: String, time_cards: Vec<TimeCard>) -> Self {
        let total_minutes = Self::calculate_worked_minutes(&time_cards);
        let total_hours = total_minutes as f64 / 60.0;

        Self {
            date,
            time_cards,
            total_minutes,
            total_hours,
        }
    }

    fn calculate_worked_minutes(cards: &[TimeCard]) -> u32 {
        let mut times: Vec<u32> = cards
            .iter()
            .filter(|c| !c.disabled)
            .filter_map(|card| Self::parse_time(&card.time))
            .collect();

        times.sort_unstable();

        times.chunks_exact(2).map(|pair| pair[1] - pair[0]).sum()
    }

    fn parse_time(time: &str) -> Option<u32> {
        let (hours, minutes) = time.trim().split_once(':')?;
        let hours: u32 = hours.parse().ok()?;
        let minutes: u32 = minutes.parse().ok()?;

        if hours > 23 || minutes > 59 {
            return None;
        }

        Some(hours * 60 + minutes)
    }
}
