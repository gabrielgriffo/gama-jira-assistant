use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklog {
    pub id: String,
    pub issue_key: String,
    pub issue_summary: String,
    pub issue_browse_url: String,
    pub origin: String,
    pub author_account_id: String,
    pub author_display_name: Option<String>,
    pub started: String,
    pub time_spent_seconds: i64,
    pub comment: Option<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogDay {
    pub date: String,
    pub worklogs: Vec<JiraWorklog>,
    pub total_seconds: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogHistoryResponse {
    pub days: Vec<JiraWorklogDay>,
    pub total_seconds: i64,
    pub total_worklogs: usize,
    pub unique_issues: usize,
    pub last_synced_at: Option<String>,
}
