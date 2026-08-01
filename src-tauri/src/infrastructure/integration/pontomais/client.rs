use crate::domain::error::DomainError;
use crate::domain::pontomais::credentials::{PontoMaisAuthFailure, PontoMaisCredentials, PontoMaisLogin};
use crate::infrastructure::integration::http::{build_client, send_with_retry, truncate_for_log};
use reqwest::header::{self, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

const BASE_URL: &str = "https://api.pontomais.com.br";
const APP_ORIGIN: &str = "https://app2.pontomais.com.br";
const APP_REFERER: &str = "https://app2.pontomais.com.br/";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
const DEVICE_PLATFORM: u8 = 4;
const DEVICE_MODEL: &str = "Chrome";
const DEVICE_PLATFORM_VERSION: &str = "148";

#[derive(Serialize)]
struct SignInBody<'a> {
    login: &'a str,
    password: &'a str,
}

#[derive(Clone)]
pub struct PontoMaisClient {
    client: Client,
    base_url: String,
}

impl PontoMaisClient {
    pub fn new() -> Self {
        Self {
            client: build_client(),
            base_url: BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn sign_in(
        &self,
        login: &PontoMaisLogin,
        uuid: &str,
    ) -> Result<PontoMaisCredentials, PontoMaisAuthFailure> {
        let url = format!("{}/api/auth/sign_in", self.base_url);
        log::info!("[PONTOMAIS] POST {}", url);

        let body = SignInBody {
            login: &login.login,
            password: &login.password,
        };

        let response = send_with_retry("PONTOMAIS", || {
            self.client.post(&url).headers(Self::base_headers()).json(&body)
        })
        .await
        .map_err(|e| {
            log::warn!("[PONTOMAIS] falha de rede no login: {}", e.without_url());
            PontoMaisAuthFailure::Network
        })?;

        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(PontoMaisAuthFailure::InvalidCredentials);
        }
        if !status.is_success() {
            log::warn!("[PONTOMAIS] login retornou status {}", status.as_u16());
            return Err(PontoMaisAuthFailure::Server(status.as_u16()));
        }

        let headers = response.headers();
        let credentials = PontoMaisCredentials::new(
            header_text(headers, "uid"),
            header_text(headers, "access-token"),
            header_text(headers, "client"),
            header_text(headers, "expiry"),
            uuid.to_string(),
        );

        if credentials.validate().is_err() {
            log::warn!(
                "[PONTOMAIS] login retornou {} sem os headers de sessão esperados",
                status.as_u16()
            );
            return Err(PontoMaisAuthFailure::Server(status.as_u16()));
        }

        Ok(credentials)
    }

    pub async fn register_device(&self, credentials: &PontoMaisCredentials) -> Result<(), DomainError> {
        let url = format!("{}/api/users/devices", self.base_url);
        log::info!("[PONTOMAIS] POST {}", url);

        let headers = Self::session_headers(credentials)?;
        let payload = serde_json::json!({
            "device": {
                "platform": DEVICE_PLATFORM,
                "device_model": DEVICE_MODEL,
                "platform_version": DEVICE_PLATFORM_VERSION
            }
        });

        let response = send_with_retry("PONTOMAIS", || {
            self.client.post(&url).headers(headers.clone()).json(&payload)
        })
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

        Self::ensure_success(response).await.map(|_| ())
    }

    pub async fn sign_out(&self, credentials: &PontoMaisCredentials) -> Result<(), DomainError> {
        let url = format!("{}/api/auth/sign_out", self.base_url);
        log::info!("[PONTOMAIS] DELETE {}", url);

        let headers = Self::session_headers(credentials)?;

        let response = send_with_retry("PONTOMAIS", || self.client.delete(&url).headers(headers.clone()))
            .await
            .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

        Self::ensure_success(response).await.map(|_| ())
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        credentials: &PontoMaisCredentials,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, DomainError> {
        if credentials.is_expired(chrono::Utc::now().timestamp()) {
            return Err(DomainError::SessionExpired);
        }

        let url = url::Url::parse_with_params(&format!("{}{}", self.base_url, path), query)
            .map_err(|e| DomainError::ExternalApiError(format!("URL do PontoMais inválida: {e}")))?;
        log::info!("[PONTOMAIS] GET {}", url);

        let headers = Self::session_headers(credentials)?;

        let response = send_with_retry("PONTOMAIS", || self.client.get(url.clone()).headers(headers.clone()))
            .await
            .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

        let body: serde_json::Value = Self::ensure_success(response)
            .await?
            .json()
            .await
            .map_err(|e| DomainError::ParseError(e.without_url().to_string()))?;

        if body.get("redirect_to_login").and_then(|v| v.as_bool()).unwrap_or(false) {
            log::info!("[PONTOMAIS] sessão recusada pela API (redirect_to_login)");
            return Err(DomainError::SessionExpired);
        }

        serde_json::from_value(body).map_err(|e| DomainError::ParseError(e.to_string()))
    }

    async fn ensure_success(response: Response) -> Result<Response, DomainError> {
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(DomainError::SessionExpired);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log::debug!(
                "[PONTOMAIS] status {} - corpo: {}",
                status.as_u16(),
                truncate_for_log(&body)
            );
            return Err(DomainError::ExternalApiError(format!(
                "PontoMais API retornou status {}",
                status.as_u16()
            )));
        }
        Ok(response)
    }

    fn base_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert(HeaderName::from_static("api-version"), HeaderValue::from_static("2"));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(header::ORIGIN, HeaderValue::from_static(APP_ORIGIN));
        headers.insert(header::REFERER, HeaderValue::from_static(APP_REFERER));
        headers.insert(header::USER_AGENT, HeaderValue::from_static(BROWSER_USER_AGENT));
        headers
    }

    fn session_headers(credentials: &PontoMaisCredentials) -> Result<HeaderMap, DomainError> {
        credentials.validate()?;

        let mut headers = Self::base_headers();
        let dynamic = [
            ("uid", credentials.email.as_str()),
            ("uuid", credentials.uuid.as_str()),
            ("access-token", credentials.token.as_str()),
            ("token", credentials.token.as_str()),
            ("expiry", credentials.expiry.as_str()),
            ("client", credentials.client_id.as_str()),
        ];
        for (name, value) in dynamic {
            let mut value = HeaderValue::from_str(value)
                .map_err(|_| DomainError::InvalidCredentials(format!("header {name} do PontoMais inválido")))?;
            if matches!(name, "access-token" | "token" | "client") {
                value.set_sensitive(true);
            }
            headers.insert(HeaderName::from_static(name), value);
        }

        Ok(headers)
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

impl Default for PontoMaisClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use zeroize::Zeroizing;

    const UUID: &str = "8c2f8f9e-2f4b-4b8e-9a51-6a2c3e7b1d10";

    fn login() -> PontoMaisLogin {
        PontoMaisLogin::new("dev@acme.com".into(), Zeroizing::new("s3nha".into()))
    }

    fn credentials() -> PontoMaisCredentials {
        PontoMaisCredentials::new(
            "dev@acme.com".into(),
            "tok".into(),
            "cli".into(),
            "4102444800".into(),
            UUID.into(),
        )
    }

    #[tokio::test]
    async fn sign_in_reads_session_from_response_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/sign_in"))
            .and(header("api-version", "2"))
            .and(body_json(
                serde_json::json!({ "login": "dev@acme.com", "password": "s3nha" }),
            ))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("access-token", "tok")
                    .insert_header("client", "cli")
                    .insert_header("expiry", "4102444800")
                    .insert_header("uid", "dev@acme.com"),
            )
            .mount(&server)
            .await;

        let creds = PontoMaisClient::with_base_url(&server.uri())
            .sign_in(&login(), UUID)
            .await
            .unwrap();

        assert_eq!(creds.email, "dev@acme.com");
        assert_eq!(creds.token.as_str(), "tok");
        assert_eq!(creds.client_id.as_str(), "cli");
        assert_eq!(creds.uuid, UUID);
    }

    #[tokio::test]
    async fn sign_in_maps_401_to_invalid_credentials_and_other_errors_to_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = PontoMaisClient::with_base_url(&server.uri());
        assert_eq!(
            client.sign_in(&login(), UUID).await.unwrap_err(),
            PontoMaisAuthFailure::InvalidCredentials
        );
        assert_eq!(
            client.sign_in(&login(), UUID).await.unwrap_err(),
            PontoMaisAuthFailure::Server(503)
        );
    }

    #[tokio::test]
    async fn sign_in_rejects_success_without_session_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201).insert_header("uid", "dev@acme.com"))
            .mount(&server)
            .await;

        let failure = PontoMaisClient::with_base_url(&server.uri())
            .sign_in(&login(), UUID)
            .await
            .unwrap_err();
        assert_eq!(failure, PontoMaisAuthFailure::Server(201));
    }

    #[tokio::test]
    async fn get_json_turns_redirect_to_login_into_session_expired() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/session"))
            .and(header("access-token", "tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "redirect_to_login": true })))
            .mount(&server)
            .await;

        let result: Result<serde_json::Value, DomainError> = PontoMaisClient::with_base_url(&server.uri())
            .get_json(&credentials(), "/api/session", &[])
            .await;
        assert_eq!(result.unwrap_err(), DomainError::SessionExpired);
    }

    #[tokio::test]
    async fn get_json_treats_expired_token_as_session_expired_without_calling_api() {
        let server = MockServer::start().await;
        let mut creds = credentials();
        creds.expiry = "1".into();

        let result: Result<serde_json::Value, DomainError> = PontoMaisClient::with_base_url(&server.uri())
            .get_json(&creds, "/api/session", &[])
            .await;
        assert_eq!(result.unwrap_err(), DomainError::SessionExpired);
        assert!(server.received_requests().await.unwrap_or_default().is_empty());
    }
}
