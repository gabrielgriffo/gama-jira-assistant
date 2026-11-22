use std::collections::HashMap;

use super::client::JiraClient;
use super::dto::{
    JiraBulkChangelogDto, JiraBulkChangelogRequestDto, JiraChangeHistoryDto, JiraChangelogDto, JiraMyselfDto,
    JiraSearchRequestDto, JiraSearchResponseDto, JiraStatusListItemDto, JiraWorklogCreateDto, JiraWorklogDto,
    JiraWorklogPageDto,
};
use super::mapper::{map_issue, map_myself};
use crate::domain::credentials::JiraCredentials;
use crate::domain::current_user::CurrentUser;
use crate::domain::date_range::DateRange;
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail, Status};
use crate::domain::issue_key::validate_issue_id_or_key;
use crate::domain::ports::AuthGateway;

const MYSELF_PATH: &str = "/rest/api/3/myself";
const SEARCH_PATH: &str = "/rest/api/3/search/jql";
const STATUS_PATH: &str = "/rest/api/3/status";
const BULK_CHANGELOG_PATH: &str = "/rest/api/3/changelog/bulkfetch";
const MAX_RESULTS: u32 = 100;
const BULK_CHANGELOG_MAX_ISSUES: usize = 1000;
const BULK_CHANGELOG_PAGE_SIZE: usize = 1000;
pub const MAX_PAGES: usize = 1000;
const BASIC_FIELDS: &[&str] = &["summary", "status", "assignee", "updated"];
const FULL_FIELDS: &[&str] = &[
    "summary",
    "status",
    "assignee",
    "updated",
    "priority",
    "issuetype",
    "duedate",
];

#[derive(Clone)]
pub struct JiraProvider {
    client: JiraClient,
}

impl JiraProvider {
    pub fn new(client: JiraClient) -> Self {
        Self { client }
    }

    async fn get_changelog_page(
        &self,
        credentials: &JiraCredentials,
        issue_key: &str,
        start_at: usize,
        max_results: usize,
    ) -> Result<JiraChangelogDto, DomainError> {
        let issue_key = validate_issue_id_or_key(issue_key)?;
        let path = format!(
            "/rest/api/3/issue/{}/changelog?startAt={}&maxResults={}",
            issue_key, start_at, max_results
        );
        self.client.get_json(credentials, &path).await.map_err(Into::into)
    }

    pub async fn get_changelog(
        &self,
        credentials: &JiraCredentials,
        issue_key: &str,
    ) -> Result<Vec<super::dto::JiraChangeHistoryDto>, DomainError> {
        let mut all_histories = Vec::new();
        let mut start_at = 0;
        let max_results = 100;

        let mut pages = 0;
        loop {
            pages += 1;
            if pages > MAX_PAGES {
                log::warn!(
                    "[JIRA] limite de {} páginas atingido, interrompendo paginação",
                    MAX_PAGES
                );
                return Err(incomplete_pagination());
            }
            let changelog = self
                .get_changelog_page(credentials, issue_key, start_at, max_results)
                .await?;
            let received = changelog.values.len();
            let fetched = start_at + received;
            all_histories.extend(changelog.values);

            if fetched >= changelog.total {
                break;
            }
            if received == 0 {
                return Err(incomplete_pagination());
            }

            start_at += max_results;
        }

        Ok(all_histories)
    }

    pub async fn get_changelogs_bulk(
        &self,
        credentials: &JiraCredentials,
        issue_ids: &[String],
    ) -> Result<HashMap<String, Vec<JiraChangeHistoryDto>>, DomainError> {
        let mut grouped: HashMap<String, Vec<JiraChangeHistoryDto>> = HashMap::new();

        for chunk in issue_ids.chunks(BULK_CHANGELOG_MAX_ISSUES) {
            let ids = chunk
                .iter()
                .map(|id| validate_issue_id_or_key(id).map(str::to_string))
                .collect::<Result<Vec<_>, _>>()?;
            let mut page_token: Option<String> = None;
            let mut same_token_retry_count = 0;
            let mut pages = 0;

            loop {
                pages += 1;
                if pages > MAX_PAGES {
                    return Err(incomplete_pagination());
                }
                let body = JiraBulkChangelogRequestDto {
                    issue_ids_or_keys: ids.clone(),
                    field_ids: vec!["status".to_string(), "assignee".to_string()],
                    max_results: BULK_CHANGELOG_PAGE_SIZE,
                    next_page_token: page_token.clone(),
                };
                let response: JiraBulkChangelogDto =
                    self.client.post_json(credentials, BULK_CHANGELOG_PATH, &body).await?;

                for log in response.issue_change_logs {
                    grouped.entry(log.issue_id).or_default().extend(log.change_histories);
                }

                if response.next_page_token.is_none() {
                    break;
                }
                if response.next_page_token == page_token {
                    if same_token_retry_count == 0 {
                        log::info!(
                            "[CHANGELOG] API retornou o mesmo page_token, aguardando 3s antes de tentar novamente"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                        same_token_retry_count += 1;
                        continue;
                    }
                    log::warn!("[CHANGELOG] API retornou o mesmo page_token após retry, parando paginação");
                    return Err(incomplete_pagination());
                }
                same_token_retry_count = 0;
                page_token = response.next_page_token;
            }
        }

        Ok(grouped)
    }

    pub async fn submit_worklog(
        &self,
        credentials: &JiraCredentials,
        issue_key: &str,
        worklog: &JiraWorklogCreateDto,
    ) -> Result<JiraWorklogDto, DomainError> {
        let issue_key = validate_issue_id_or_key(issue_key)?;
        let path = format!("/rest/api/3/issue/{}/worklog", issue_key);
        Ok(self.client.post_json(credentials, &path, worklog).await?)
    }

    pub async fn get_all_statuses(
        &self,
        credentials: &JiraCredentials,
    ) -> Result<Vec<JiraStatusListItemDto>, DomainError> {
        let statuses: Vec<JiraStatusListItemDto> = self.client.get_json(credentials, STATUS_PATH).await?;
        Ok(statuses)
    }

    pub async fn get_worklogs_started_between(
        &self,
        credentials: &JiraCredentials,
        issue_id_or_key: &str,
        started_between_ms: Option<(i64, i64)>,
    ) -> Result<Vec<JiraWorklogDto>, DomainError> {
        let issue_id_or_key = validate_issue_id_or_key(issue_id_or_key)?;
        let mut all_worklogs = Vec::new();
        let mut start_at = 0;
        let max_results = 100;
        let filter = started_between_ms
            .map(|(after, before)| format!("&startedAfter={}&startedBefore={}", after, before))
            .unwrap_or_default();

        let mut pages = 0;
        loop {
            pages += 1;
            if pages > MAX_PAGES {
                log::warn!(
                    "[JIRA] limite de {} páginas atingido, interrompendo paginação",
                    MAX_PAGES
                );
                return Err(incomplete_pagination());
            }
            let path = format!(
                "/rest/api/3/issue/{}/worklog?startAt={}&maxResults={}{}",
                issue_id_or_key, start_at, max_results, filter
            );

            let page: JiraWorklogPageDto = self.client.get_json(credentials, &path).await?;
            let received = page.worklogs.len() as i32;
            let fetched = start_at + received;
            all_worklogs.extend(page.worklogs);

            if fetched >= page.total {
                break;
            }
            if received == 0 {
                return Err(incomplete_pagination());
            }

            start_at += max_results;
        }

        Ok(all_worklogs)
    }

    pub async fn search_issues_page(
        &self,
        credentials: &JiraCredentials,
        jql: &str,
        detail: IssueDetail,
        page_token: Option<String>,
    ) -> Result<(Vec<(Issue, Status)>, Option<String>), DomainError> {
        let fields = match detail {
            IssueDetail::Full => FULL_FIELDS,
            IssueDetail::Basic => BASIC_FIELDS,
        };
        let request = JiraSearchRequestDto {
            jql: jql.to_string(),
            max_results: MAX_RESULTS,
            fields: fields.to_vec(),
            next_page_token: page_token,
        };

        let response: JiraSearchResponseDto = self.client.post_json(credentials, SEARCH_PATH, &request).await?;

        let issues = response
            .issues
            .into_iter()
            .map(|dto| map_issue(dto, &credentials.base_url))
            .collect();

        Ok((issues, response.next_page_token))
    }

    pub async fn search_issues(
        &self,
        credentials: &JiraCredentials,
        jql: &str,
        detail: IssueDetail,
    ) -> Result<Vec<(Issue, Status)>, DomainError> {
        let mut all_issues = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut page_token: Option<String> = None;
        let mut same_token_retry_count = 0;

        let mut pages = 0;
        loop {
            pages += 1;
            if pages > MAX_PAGES {
                return Err(incomplete_pagination());
            }
            let (page, next_token) = self
                .search_issues_page(credentials, jql, detail, page_token.clone())
                .await?;
            for item in page {
                if seen.insert(item.0.id.clone()) {
                    all_issues.push(item);
                }
            }

            if next_token.is_none() {
                break;
            }

            if next_token == page_token {
                if same_token_retry_count == 0 {
                    log::info!("[SEARCH] API retornou o mesmo page_token, aguardando 3s antes de tentar novamente");
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    same_token_retry_count += 1;
                    continue;
                }
                log::warn!("[SEARCH] API retornou o mesmo page_token após retry, parando paginação");
                return Err(incomplete_pagination());
            }

            same_token_retry_count = 0;
            page_token = next_token;
        }

        Ok(all_issues)
    }

    pub async fn search_my_issues_page(
        &self,
        credentials: &JiraCredentials,
        range: &DateRange,
        development_status_ids: &[String],
        page_token: Option<String>,
    ) -> Result<(Vec<(Issue, Status)>, Option<String>), DomainError> {
        self.search_issues_page(
            credentials,
            &issues_assigned_during_jql(range, development_status_ids),
            IssueDetail::Full,
            page_token,
        )
        .await
    }
}

pub fn incomplete_pagination() -> DomainError {
    DomainError::UnexpectedResponse("a paginação do Jira não terminou; os dados ficariam incompletos".to_string())
}

pub fn issues_with_my_worklogs_jql(range: &DateRange) -> String {
    format!(
        "worklogAuthor = currentUser() AND worklogDate >= \"{}\" AND worklogDate <= \"{}\" ORDER BY updated DESC",
        range.from, range.to,
    )
}

pub fn issues_assigned_during_jql(range: &DateRange, development_status_ids: &[String]) -> String {
    let during = format!("DURING (\"{}\", \"{}\")", range.from, range.end_exclusive());
    let safe_ids: Vec<&str> = development_status_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    let status_filter = if safe_ids.is_empty() {
        String::new()
    } else {
        format!(" AND status WAS IN ({}) {}", safe_ids.join(", "), during)
    };
    format!(
        "assignee WAS currentUser() {}{} ORDER BY priority DESC",
        during, status_filter
    )
}

impl AuthGateway for JiraProvider {
    async fn verify_credentials(&self, credentials: &JiraCredentials) -> Result<CurrentUser, DomainError> {
        let dto: JiraMyselfDto = self.client.get_json(credentials, MYSELF_PATH).await?;
        Ok(map_myself(dto))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worklogs_jql() {
        let range = DateRange::parse("2026-10-01", "2026-10-05").unwrap();
        assert_eq!(
            issues_with_my_worklogs_jql(&range),
            "worklogAuthor = currentUser() AND worklogDate >= \"2026-10-01\" AND worklogDate <= \"2026-10-05\" ORDER BY updated DESC"
        );
    }

    #[test]
    fn assigned_during_jql_filters_development_statuses() {
        let range = DateRange::parse("2026-10-01", "2026-10-05").unwrap();
        assert_eq!(
            issues_assigned_during_jql(&range, &["10001".to_string(), "10023".to_string()]),
            "assignee WAS currentUser() DURING (\"2026-10-01\", \"2026-10-06\") AND status WAS IN (10001, 10023) DURING (\"2026-10-01\", \"2026-10-06\") ORDER BY priority DESC"
        );
    }

    #[test]
    fn assigned_during_jql_ignores_non_numeric_status_ids() {
        let range = DateRange::parse("2026-10-01", "2026-10-05").unwrap();
        assert_eq!(
            issues_assigned_during_jql(&range, &["10001".to_string(), "1) OR project = X --".to_string()]),
            issues_assigned_during_jql(&range, &["10001".to_string()])
        );
    }

    #[test]
    fn assigned_during_jql_without_development_statuses() {
        let range = DateRange::parse("2026-10-01", "2026-10-05").unwrap();
        assert_eq!(
            issues_assigned_during_jql(&range, &[]),
            "assignee WAS currentUser() DURING (\"2026-10-01\", \"2026-10-06\") ORDER BY priority DESC"
        );
    }

    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_provider() -> JiraProvider {
        JiraProvider::new(JiraClient::for_tests())
    }

    fn test_credentials(server: &MockServer) -> JiraCredentials {
        JiraCredentials {
            base_url: server.uri(),
            email: "me@example.com".to_string(),
            api_token: zeroize::Zeroizing::new("token".to_string()),
        }
    }

    fn history(id: &str, created: &str) -> serde_json::Value {
        json!({
            "id": id,
            "created": created,
            "items": [{"field": "status", "fieldId": "status", "fieldtype": "jira", "from": "1", "to": "2"}]
        })
    }

    #[tokio::test]
    async fn search_that_never_finishes_paginating_is_an_error_not_a_partial_result() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [],
                "nextPageToken": "same"
            })))
            .mount(&server)
            .await;

        let result = test_provider()
            .search_issues(&test_credentials(&server), "project = GAMA", IssueDetail::Basic)
            .await;

        assert!(matches!(result, Err(DomainError::UnexpectedResponse(_))));
    }

    #[tokio::test]
    async fn worklog_page_that_ends_before_the_total_is_an_error_not_a_partial_result() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/10/worklog"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "startAt": 0,
                "maxResults": 100,
                "total": 250,
                "worklogs": []
            })))
            .mount(&server)
            .await;

        let result = test_provider()
            .get_worklogs_started_between(&test_credentials(&server), "10", None)
            .await;

        assert!(matches!(result, Err(DomainError::UnexpectedResponse(_))));
    }

    #[tokio::test]
    async fn changelog_page_that_ends_before_the_total_is_an_error_not_a_partial_result() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/A-1/changelog"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "startAt": 0,
                "maxResults": 100,
                "total": 250,
                "values": []
            })))
            .mount(&server)
            .await;

        let result = test_provider().get_changelog(&test_credentials(&server), "A-1").await;

        assert!(matches!(result, Err(DomainError::UnexpectedResponse(_))));
    }

    #[tokio::test]
    async fn missing_issue_is_reported_as_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/10/worklog"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let result = test_provider()
            .get_worklogs_started_between(&test_credentials(&server), "10", None)
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn forbidden_is_reported_as_missing_permission_not_as_bad_credentials() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(STATUS_PATH))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let err = test_provider()
            .get_all_statuses(&test_credentials(&server))
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::ExternalApiError(ref m) if m.contains("permissão")));
    }

    #[tokio::test]
    async fn rejection_carries_the_message_returned_by_jira() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(STATUS_PATH))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "errorMessages": ["Time tracking está desativado."],
                "errors": {"started": "Data inválida"}
            })))
            .mount(&server)
            .await;

        let err = test_provider()
            .get_all_statuses(&test_credentials(&server))
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("Time tracking está desativado."));
        assert!(err.contains("Data inválida"));
    }

    #[tokio::test]
    async fn bulk_changelogs_follow_pages_and_group_by_issue() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(BULK_CHANGELOG_PATH))
            .and(body_partial_json(json!({"nextPageToken": "t1"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issueChangeLogs": [{"issueId": "10", "changeHistories": [history("3", "2026-10-03T10:00:00.000+0000")]}]
            })))
            .with_priority(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(BULK_CHANGELOG_PATH))
            .and(body_partial_json(json!({
                "issueIdsOrKeys": ["10", "20"],
                "fieldIds": ["status", "assignee"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issueChangeLogs": [
                    {"issueId": "10", "changeHistories": [history("1", "2026-10-01T10:00:00.000+0000")]},
                    {"issueId": "20", "changeHistories": [history("2", "2026-10-02T10:00:00.000+0000")]}
                ],
                "nextPageToken": "t1"
            })))
            .with_priority(2)
            .expect(1)
            .mount(&server)
            .await;

        let histories = test_provider()
            .get_changelogs_bulk(&test_credentials(&server), &["10".to_string(), "20".to_string()])
            .await
            .unwrap();

        assert_eq!(histories.len(), 2);
        assert_eq!(
            histories["10"].iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            vec!["1", "3"]
        );
        assert_eq!(
            histories["20"].iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            vec!["2"]
        );
    }

    #[tokio::test]
    async fn bulk_changelogs_split_requests_in_chunks_of_1000_issues() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(BULK_CHANGELOG_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"issueChangeLogs": []})))
            .expect(2)
            .mount(&server)
            .await;

        let ids: Vec<String> = (1..=1001).map(|n| n.to_string()).collect();
        let histories = test_provider()
            .get_changelogs_bulk(&test_credentials(&server), &ids)
            .await
            .unwrap();

        assert!(histories.is_empty());
        let requests = server.received_requests().await.unwrap();
        let sizes: Vec<usize> = requests
            .iter()
            .map(|r| {
                r.body_json::<serde_json::Value>().unwrap()["issueIdsOrKeys"]
                    .as_array()
                    .unwrap()
                    .len()
            })
            .collect();
        assert_eq!(sizes, vec![1000, 1]);
    }

    #[tokio::test]
    async fn bulk_changelogs_skip_request_without_issues() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let histories = test_provider()
            .get_changelogs_bulk(&test_credentials(&server), &[])
            .await
            .unwrap();
        assert!(histories.is_empty());
    }
}
