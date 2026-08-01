use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorklogEntry {
    pub date: String,
    pub issue_key: String,
    pub issue_summary: String,
    pub status: String,
    pub hours: f64,
    pub start_timestamp: String,
    pub end_timestamp: String,
    #[serde(default)]
    pub comment: Option<String>,
}
