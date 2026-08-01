use super::client::JiraClient;
use super::dto::{
    JiraBulkChangelogDto, JiraBulkChangelogRequestDto, JiraChangelogDto, JiraMyselfDto, JiraSearchRequestDto,
    JiraSearchResponseDto, JiraStatusListItemDto, JiraWorklogCreateDto, JiraWorklogDto, JiraWorklogPageDto,
};
use super::mapper::{map_issue, map_myself};
use crate::domain::credentials::JiraCredentials;
use crate::domain::current_user::CurrentUser;
use crate::domain::date_range::DateRange;
use crate::domain::error::DomainError;
use crate::domain::issue::{Issue, IssueDetail, Status};
use crate::domain::issue_key::validate_issue_id_or_key;
use crate::domain::ports::AuthGateway;
use crate::infrastructure::database::ChangelogRepository;

const MYSELF_PATH: &str = "/rest/api/3/myself";
const SEARCH_PATH: &str = "/rest/api/3/search/jql";
const STATUS_PATH: &str = "/rest/api/3/status";
const BULK_CHANGELOG_PATH: &str = "/rest/api/3/changelog/bulkfetch";
const MAX_RESULTS: u32 = 5;
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

    async fn get_changelog_page_bulk(
        &self,
        credentials: &JiraCredentials,
        issue_key: &str,
        page_token: Option<String>,
        max_results: usize,
    ) -> Result<JiraBulkChangelogDto, DomainError> {
        let issue_key = validate_issue_id_or_key(issue_key)?;
        let body = JiraBulkChangelogRequestDto {
            issue_ids_or_keys: vec![issue_key.to_string()],
            field_ids: vec!["status".to_string(), "assignee".to_string()],
            max_results,
            next_page_token: page_token,
        };

        self.client
            .post_json(credentials, BULK_CHANGELOG_PATH, &body)
            .await
            .map_err(Into::into)
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
                break;
            }
            let changelog = self
                .get_changelog_page(credentials, issue_key, start_at, max_results)
                .await?;
            let fetched = start_at + changelog.values.len();
            all_histories.extend(changelog.values);

            if fetched >= changelog.total {
                break;
            }

            start_at += max_results;
        }

        Ok(all_histories)
    }

    pub async fn get_changelog_incremental(
        &self,
        credentials: &JiraCredentials,
        issue_key: &str,
        repo: &ChangelogRepository,
        issue_id: &str,
    ) -> Result<Vec<super::dto::JiraChangeHistoryDto>, DomainError> {
        let mut new_histories = Vec::new();
        let mut page_token: Option<String> = None;
        let max_results = 10;
        let mut found_known_entry = false;
        let mut same_token_retry_count = 0;

        let mut pages = 0;
        while !found_known_entry {
            pages += 1;
            if pages > MAX_PAGES {
                log::warn!(
                    "[JIRA] limite de {} páginas atingido, interrompendo paginação",
                    MAX_PAGES
                );
                break;
            }
            let response = self
                .get_changelog_page_bulk(credentials, issue_key, page_token.clone(), max_results)
                .await?;

            let histories = response
                .issue_change_logs
                .first()
                .map(|log| log.change_histories.clone())
                .unwrap_or_default();

            if histories.is_empty() {
                break;
            }

            for entry in histories {
                if repo.changelog_exists(issue_id, &entry.id).await? {
                    found_known_entry = true;
                    break;
                }
                new_histories.push(entry);
            }

            if response.next_page_token.is_none() {
                break;
            }

            if response.next_page_token == page_token {
                if same_token_retry_count == 0 {
                    log::info!(
                        "[CHANGELOG] {} - API retornou o mesmo page_token, aguardando 3s antes de tentar novamente",
                        issue_key
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    same_token_retry_count += 1;
                    continue;
                } else {
                    log::warn!(
                        "[CHANGELOG] {} - API retornou o mesmo page_token após retry, parando paginação",
                        issue_key
                    );
                    break;
                }
            }

            same_token_retry_count = 0;
            page_token = response.next_page_token;
        }

        Ok(new_histories)
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
                break;
            }
            let path = format!(
                "/rest/api/3/issue/{}/worklog?startAt={}&maxResults={}{}",
                issue_id_or_key, start_at, max_results, filter
            );

            let page: JiraWorklogPageDto = self.client.get_json(credentials, &path).await?;
            let received = page.worklogs.len() as i32;
            let fetched = start_at + received;
            all_worklogs.extend(page.worklogs);

            if received == 0 || fetched >= page.total {
                break;
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
        let mut page_token: Option<String> = None;
        let mut same_token_retry_count = 0;

        let mut pages = 0;
        loop {
            pages += 1;
            if pages > MAX_PAGES {
                log::warn!(
                    "[JIRA] limite de {} páginas atingido, interrompendo paginação",
                    MAX_PAGES
                );
                break;
            }
            let (page, next_token) = self
                .search_issues_page(credentials, jql, detail, page_token.clone())
                .await?;
            all_issues.extend(page);

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
                break;
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
}
