use serde::{Deserialize, Serialize};

/// Raw shape of `GET /rest/api/3/myself`. Only the fields we actually use.
#[derive(Debug, Deserialize)]
pub struct JiraMyselfDto {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
    #[serde(rename = "timeZone")]
    pub time_zone: Option<String>,
}

/// Raw shape of `GET /rest/api/3/issue/{issueIdOrKey}/changelog`.
/// Endpoint dedicado que suporta paginação corretamente.
#[derive(Debug, Deserialize)]
pub struct JiraChangelogDto {
    pub total: usize,
    #[serde(default)]
    pub values: Vec<JiraChangeHistoryDto>,
}

/// Request body for `POST /rest/api/3/changelog/bulkfetch`.
#[derive(Debug, Serialize)]
pub struct JiraBulkChangelogRequestDto {
    #[serde(rename = "issueIdsOrKeys")]
    pub issue_ids_or_keys: Vec<String>,
    #[serde(rename = "fieldIds")]
    pub field_ids: Vec<String>,
    #[serde(rename = "maxResults")]
    pub max_results: usize,
    #[serde(rename = "nextPageToken", skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// Response of `POST /rest/api/3/changelog/bulkfetch`.
#[derive(Debug, Deserialize)]
pub struct JiraBulkChangelogDto {
    #[serde(rename = "issueChangeLogs")]
    pub issue_change_logs: Vec<JiraIssueChangeLogDto>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssueChangeLogDto {
    #[serde(rename = "changeHistories")]
    pub change_histories: Vec<JiraChangeHistoryDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraChangeHistoryDto {
    pub id: String,
    pub author: Option<JiraAuthorDto>,
    #[serde(deserialize_with = "deserialize_epoch_millis")]
    pub created: i64,
    #[serde(default)]
    pub items: Vec<JiraChangeItemDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraAuthorDto {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct JiraAssigneeDto {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraChangeItemDto {
    pub field: String,
    #[serde(rename = "fieldId")]
    pub field_id: Option<String>,
    #[serde(rename = "fieldtype")]
    pub field_type: String,
    pub from: Option<String>,
    #[serde(rename = "fromString")]
    pub from_string: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "toString")]
    pub to_string: Option<String>,
}

/// Raw shape of `POST /rest/api/3/search/jql`.
#[derive(Debug, Deserialize)]
pub struct JiraSearchResponseDto {
    #[serde(default)]
    pub issues: Vec<JiraIssueDto>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssueDto {
    pub id: String,
    pub key: String,
    pub fields: JiraIssueFieldsDto,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssueFieldsDto {
    pub summary: String,
    pub status: JiraStatusDto,
    pub assignee: Option<JiraAssigneeDto>,
    pub priority: Option<JiraPriorityDto>,
    #[serde(rename = "issuetype")]
    pub issue_type: Option<JiraIssueTypeDto>,
    #[serde(rename = "duedate")]
    pub due_date: Option<String>,
    pub updated: String,
}

#[derive(Debug, Deserialize)]
pub struct JiraStatusDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "statusCategory")]
    pub status_category: Option<JiraStatusCategoryDto>,
}

#[derive(Debug, Deserialize)]
pub struct JiraPriorityDto {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssueTypeDto {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct JiraSearchRequestDto {
    pub jql: String,
    #[serde(rename = "maxResults")]
    pub max_results: u32,
    pub fields: Vec<&'static str>,
    #[serde(rename = "nextPageToken", skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JiraWorklogCreateDto {
    pub started: String,
    #[serde(rename = "timeSpentSeconds")]
    pub time_spent_seconds: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<JiraWorklogCommentDto>,
}

#[derive(Debug, Serialize)]
pub struct JiraWorklogCommentDto {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub version: u32,
    pub content: Vec<JiraDocContentDto>,
}

#[derive(Debug, Serialize)]
pub struct JiraDocContentDto {
    #[serde(rename = "type")]
    pub content_type: String,
    pub content: Vec<JiraDocTextDto>,
}

#[derive(Debug, Serialize)]
pub struct JiraDocTextDto {
    #[serde(rename = "type")]
    pub text_type: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct JiraStatusListItemDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "untranslatedName")]
    pub untranslated_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "iconUrl")]
    pub icon_url: String,
    #[serde(rename = "statusCategory")]
    pub status_category: JiraStatusCategoryDto,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct JiraStatusCategoryDto {
    pub id: i32,
    pub key: String,
    pub name: String,
    #[serde(rename = "colorName")]
    pub color_name: String,
}

impl JiraWorklogCreateDto {
    pub fn new(started: String, time_spent_seconds: u32) -> Self {
        Self {
            started,
            time_spent_seconds,
            comment: None,
        }
    }

    pub fn with_comment(mut self, text: &str) -> Self {
        self.comment = Some(JiraWorklogCommentDto {
            doc_type: "doc".to_string(),
            version: 1,
            content: vec![JiraDocContentDto {
                content_type: "paragraph".to_string(),
                content: vec![JiraDocTextDto {
                    text_type: "text".to_string(),
                    text: text.to_string(),
                }],
            }],
        });
        self
    }
}

#[derive(Debug, Deserialize)]
pub struct JiraWorklogPageDto {
    pub total: i32,
    #[serde(default)]
    pub worklogs: Vec<JiraWorklogDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraWorklogDto {
    pub id: String,
    #[serde(rename = "issueId")]
    pub issue_id: Option<String>,
    pub author: Option<JiraWorklogAuthorDto>,
    pub started: String,
    #[serde(rename = "timeSpentSeconds")]
    pub time_spent_seconds: i64,
    #[serde(default)]
    pub comment: Option<serde_json::Value>,
    pub created: Option<String>,
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraWorklogAuthorDto {
    #[serde(rename = "accountId")]
    pub account_id: String,
}

impl JiraWorklogDto {
    pub fn comment_text(&self) -> Option<String> {
        let mut paragraphs = Vec::new();
        if let Some(comment) = &self.comment {
            collect_adf_paragraphs(comment, &mut paragraphs);
        }
        let text = paragraphs
            .into_iter()
            .filter(|p| !p.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

fn collect_adf_paragraphs(node: &serde_json::Value, paragraphs: &mut Vec<String>) {
    if let Some(text) = node.as_str() {
        paragraphs.push(text.to_string());
        return;
    }
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if node_type == "paragraph" || node_type == "heading" {
        let mut line = String::new();
        collect_adf_text(node, &mut line);
        paragraphs.push(line);
        return;
    }
    if let Some(children) = node.get("content").and_then(|c| c.as_array()) {
        for child in children {
            collect_adf_paragraphs(child, paragraphs);
        }
    }
}

fn collect_adf_text(node: &serde_json::Value, out: &mut String) {
    if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
        out.push_str(text);
    }
    if node.get("type").and_then(|t| t.as_str()) == Some("hardBreak") {
        out.push(' ');
    }
    if let Some(children) = node.get("content").and_then(|c| c.as_array()) {
        for child in children {
            collect_adf_text(child, out);
        }
    }
}

fn deserialize_epoch_millis<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Instant {
        Millis(i64),
        Text(String),
    }

    match Instant::deserialize(deserializer)? {
        Instant::Millis(millis) => Ok(millis),
        Instant::Text(text) => chrono::DateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S%.f%z")
            .or_else(|_| chrono::DateTime::parse_from_rfc3339(&text))
            .map(|t| t.timestamp_millis())
            .map_err(|_| serde::de::Error::custom(format!("data inválida no changelog: {text}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_history_accepts_epoch_millis() {
        let dto: JiraChangeHistoryDto =
            serde_json::from_str(r#"{"id":"1","created":1759685531123,"items":[]}"#).unwrap();
        assert_eq!(dto.created, 1759685531123);
    }

    #[test]
    fn change_history_accepts_iso_text() {
        let dto: JiraChangeHistoryDto =
            serde_json::from_str(r#"{"id":"1","created":"2025-10-05T14:32:11.123-0300","items":[]}"#).unwrap();
        assert_eq!(dto.created, 1759685531123);
    }

    #[test]
    fn worklog_comment_text_from_adf() {
        let json = r#"{"id":"1","issueId":"10","author":{"accountId":"me"},"started":"2026-10-05T09:00:00.000-0300","timeSpentSeconds":3600,"comment":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Ajuste "},{"type":"text","text":"no login"}]},{"type":"paragraph","content":[{"type":"text","text":"Testes"}]}]}}"#;
        let dto: JiraWorklogDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.comment_text().as_deref(), Some("Ajuste no login\nTestes"));
        assert_eq!(dto.author.unwrap().account_id, "me");
    }

    #[test]
    fn changelog_page_from_issue_endpoint() {
        let json = r#"{"startAt":0,"maxResults":100,"total":1,"isLast":true,"values":[{"id":"10001","author":{"accountId":"abc","displayName":"Fulano"},"created":"2025-10-05T14:32:11.123-0300","items":[{"field":"status","fieldtype":"jira","from":"1","fromString":"A fazer","to":"3","toString":"Em andamento"}]}]}"#;
        let dto: JiraChangelogDto = serde_json::from_str(json).unwrap();
        assert_eq!(dto.values.len(), 1);
        assert_eq!(dto.values[0].created, 1759685531123);
        let item = &dto.values[0].items[0];
        assert_eq!(item.from.as_deref(), Some("1"));
        assert_eq!(item.to.as_deref(), Some("3"));
    }
}
