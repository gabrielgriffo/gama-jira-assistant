#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub summary: String,
    pub status_id: String,
    pub status_name: String,
    pub status_category_key: String,
    pub assignee_account_id: Option<String>,
    pub assignee_display_name: Option<String>,
    #[serde(alias = "priority")]
    pub priority_name: Option<String>,
    pub issue_type_name: Option<String>,
    pub due_date: Option<String>,
    pub browse_url: String,
    #[serde(alias = "updatedAt")]
    pub jira_updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueDetail {
    Full,
    Basic,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: String,
    pub name: String,
    pub untranslated_name: Option<String>,
    pub category_key: String,
    pub category_name: String,
    pub category_color: String,
    pub icon_url: Option<String>,
    #[serde(alias = "isDevelopmentStatus")]
    pub is_development: bool,
}
