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

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        credentials: &JiraCredentials,
        path: &str,
    ) -> Result<T, JiraError> {
        let url = build_url(credentials, path);
        log::info!("[JIRA] GET {}", url);

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
        log::info!("[JIRA] POST {}", url);

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

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(JiraError::Unauthorized);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(JiraError::NotFound);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::debug!("[JIRA] status {} - corpo: {}", status.as_u16(), truncate_for_log(&body));
            return Err(JiraError::UnexpectedStatus(status.as_u16()));
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

fn build_url(credentials: &JiraCredentials, path: &str) -> String {
    format!("{}{}", credentials.base_url.trim_end_matches('/'), path)
}
