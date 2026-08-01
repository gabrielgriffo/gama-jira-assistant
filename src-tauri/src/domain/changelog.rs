use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogResponse {
    pub earliest_date: Option<String>,
    pub latest_date: Option<String>,
    pub entries: Vec<ChangelogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogEntry {
    pub changelog_id: String,
    pub author_account_id: Option<String>,
    pub author_display_name: Option<String>,
    pub jira_created_at: String,
    pub items: Vec<ChangelogItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogItem {
    pub field_id: Option<String>,
    pub field: String,
    pub field_type: String,
    pub from_id: Option<String>,
    pub from_string: Option<String>,
    pub to_id: Option<String>,
    pub to_string: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IssueChange {
    pub changelog_id: String,
    pub item_index: i32,
    pub author_account_id: Option<String>,
    pub author_display_name: Option<String>,
    pub jira_created_at: String,
    pub field_id: Option<String>,
    pub field: String,
    pub field_type: String,
    pub from_id: Option<String>,
    pub from_string: Option<String>,
    pub to_id: Option<String>,
    pub to_string: Option<String>,
}
