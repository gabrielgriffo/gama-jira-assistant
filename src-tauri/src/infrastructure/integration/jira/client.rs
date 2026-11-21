use serde::{de::DeserializeOwned, Serialize};

use super::error::JiraError;
use crate::domain::credentials::JiraCredentials;
use crate::infrastructure::integration::http::{build_client, send_with_retry, truncate_for_log};

/// Thin, stateless HTTP client for the Jira Cloud REST API. Knows how to
/// authenticate, build URLs, and turn HTTP status codes into `JiraError` —
/// nothing about tasks, users or business rules. Credentials are passed
/// in per call, never stored: this client never asks where they came from.
#[derive(Clone)]
pub struct JiraClient {
    http: reqwest::Client,
}

impl JiraClient {
    pub fn new() -> Self {
        Self { http: build_client() }
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        credentials: &JiraCredentials,
        path: &str,
    ) -> Result<T, JiraError> {
        let url = build_url(credentials, path);

        let response = send_with_retry("JIRA", || {
            self.http
                .get(&url)
                .basic_auth(&credentials.email, Some(credentials.api_token.as_str()))
                .header("Accept", "application/json")
        })
        .await
        .map_err(|e| JiraError::Http(e.without_url().to_string()))?;

        Self::parse_response(response).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        credentials: &JiraCredentials,
        path: &str,
        body: &B,
    ) -> Result<T, JiraError> {
        let url = build_url(credentials, path);

        let response = send_with_retry("JIRA", || {
            self.http
                .post(&url)
                .basic_auth(&credentials.email, Some(credentials.api_token.as_str()))
                .header("Accept", "application/json")
                .json(body)
        })
        .await
        .map_err(|e| JiraError::Http(e.without_url().to_string()))?;

        Self::parse_response(response).await
    }

    async fn parse_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, JiraError> {
        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(JiraError::Unauthorized);
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(JiraError::Forbidden);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(JiraError::NotFound);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::debug!("[JIRA] status {} - corpo: {}", status.as_u16(), truncate_for_log(&body));
            return Err(match rejection_message(&body) {
                Some(message) => JiraError::Rejected(status.as_u16(), message),
                None => JiraError::UnexpectedStatus(status.as_u16()),
            });
        }

        response
            .json::<T>()
            .await
            .map_err(|e| JiraError::Deserialization(e.without_url().to_string()))
    }
}

impl Default for JiraClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct JiraErrorBody {
    #[serde(default)]
    error_messages: Vec<String>,
    #[serde(default)]
    errors: std::collections::BTreeMap<String, String>,
}

fn rejection_message(body: &str) -> Option<String> {
    let parsed: JiraErrorBody = serde_json::from_str(body).ok()?;
    let messages: Vec<String> = parsed
        .error_messages
        .into_iter()
        .chain(parsed.errors.into_values())
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    if messages.is_empty() {
        None
    } else {
        Some(truncate_for_log(&messages.join("; ")))
    }
}

fn build_url(credentials: &JiraCredentials, path: &str) -> String {
    format!("{}{}", credentials.base_url.trim_end_matches('/'), path)
}
