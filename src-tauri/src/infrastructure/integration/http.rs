use std::time::Duration;

use reqwest::{redirect, Client, RequestBuilder, Response, StatusCode};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RETRIES: u32 = 3;
const MAX_RETRY_AFTER_SECS: u64 = 30;
const MAX_LOGGED_BODY_CHARS: usize = 500;

pub fn build_client() -> Client {
    Client::builder()
        .https_only(true)
        .redirect(redirect::Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!("Gama/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build HTTP client")
}

pub async fn send_with_retry<F>(prefix: &str, build: F) -> Result<Response, reqwest::Error>
where
    F: Fn() -> RequestBuilder,
{
    let mut attempt = 0;
    loop {
        let response = build().send().await?;
        if response.status() != StatusCode::TOO_MANY_REQUESTS || attempt >= MAX_RETRIES {
            return Ok(response);
        }

        let wait = retry_after(&response).unwrap_or_else(|| Duration::from_secs(2u64.pow(attempt)));
        log::warn!("[{prefix}] 429 recebido, nova tentativa em {}s", wait.as_secs());
        tokio::time::sleep(wait).await;
        attempt += 1;
    }
}

fn retry_after(response: &Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|secs| Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS)))
}

pub fn truncate_for_log(body: &str) -> String {
    let mut truncated: String = body.chars().take(MAX_LOGGED_BODY_CHARS).collect();
    if truncated.len() < body.len() {
        truncated.push('…');
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_for_log_limits_size_without_splitting_chars() {
        let body = "ç".repeat(MAX_LOGGED_BODY_CHARS + 10);
        let truncated = truncate_for_log(&body);
        assert_eq!(truncated.chars().count(), MAX_LOGGED_BODY_CHARS + 1);
        assert_eq!(truncate_for_log("curto"), "curto");
    }
}
