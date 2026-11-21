use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_submission_sent_by_the_frontend_in_camel_case() {
        let entry: WorklogEntry = serde_json::from_str(
            r#"{"date":"2026-11-17","issueKey":"GAMA-1","issueSummary":"Tarefa","status":"","hours":1.5,
                "startTimestamp":"09:00","endTimestamp":"","comment":"ok"}"#,
        )
        .unwrap();
        assert_eq!(entry.issue_key, "GAMA-1");
        assert_eq!(entry.start_timestamp, "09:00");
    }
}
