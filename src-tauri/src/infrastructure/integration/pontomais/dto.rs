use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PontoMaisWorkDaysResponse {
    pub work_days: Vec<PontoMaisWorkDayDto>,
}

#[derive(Debug, Deserialize)]
pub struct PontoMaisWorkDayDto {
    pub time_cards: Vec<PontoMaisTimeCardDto>,
}

#[derive(Debug, Deserialize)]
pub struct PontoMaisTimeCardDto {
    pub id: u64,
    pub disabled: bool,
    pub date: String,
    pub time: String,
}
