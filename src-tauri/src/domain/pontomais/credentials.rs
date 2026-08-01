use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::domain::error::DomainError;

const MAX_FIELD_LEN: usize = 512;

#[derive(Clone, Serialize, Deserialize)]
pub struct PontoMaisCredentials {
    pub email: String,
    pub token: Zeroizing<String>,
    pub client_id: Zeroizing<String>,
    pub expiry: String,
    pub uuid: String,
}

impl std::fmt::Debug for PontoMaisCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PontoMaisCredentials")
            .field("email", &self.email)
            .field("token", &"<redacted>")
            .field("client_id", &"<redacted>")
            .field("expiry", &self.expiry)
            .field("uuid", &self.uuid)
            .finish()
    }
}

impl PontoMaisCredentials {
    pub fn new(email: String, token: String, client_id: String, expiry: String, uuid: String) -> Self {
        Self {
            email,
            token: Zeroizing::new(token),
            client_id: Zeroizing::new(client_id),
            expiry,
            uuid,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        let invalid = |field: &str| DomainError::InvalidCredentials(format!("campo {field} do PontoMais inválido"));

        if !is_header_safe(&self.email) {
            return Err(invalid("e-mail"));
        }
        if !is_header_safe(&self.token) {
            return Err(invalid("token"));
        }
        if !is_header_safe(&self.client_id) {
            return Err(invalid("client"));
        }
        if !is_header_safe(&self.expiry) || !self.expiry.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid("expiry"));
        }
        if uuid::Uuid::parse_str(&self.uuid).is_err() {
            return Err(invalid("uuid"));
        }
        Ok(())
    }

    pub fn is_expired(&self, now_unix_secs: i64) -> bool {
        self.expiry
            .parse::<i64>()
            .map(|expiry| expiry <= now_unix_secs)
            .unwrap_or(false)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PontoMaisLogin {
    pub login: String,
    pub password: Zeroizing<String>,
}

impl std::fmt::Debug for PontoMaisLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PontoMaisLogin")
            .field("login", &self.login)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl PontoMaisLogin {
    pub fn new(login: String, password: Zeroizing<String>) -> Self {
        Self {
            login: login.trim().to_string(),
            password,
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if !is_header_safe(&self.login) {
            return Err(DomainError::InvalidCredentials(
                "informe o e-mail do PontoMais".to_string(),
            ));
        }
        if self.password.is_empty()
            || self.password.len() > MAX_FIELD_LEN
            || self.password.chars().any(char::is_control)
        {
            return Err(DomainError::InvalidCredentials(
                "informe a senha do PontoMais".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PontoMaisAuthFailure {
    InvalidCredentials,
    Server(u16),
    Network,
}

impl From<PontoMaisAuthFailure> for DomainError {
    fn from(failure: PontoMaisAuthFailure) -> Self {
        match failure {
            PontoMaisAuthFailure::InvalidCredentials => {
                DomainError::InvalidCredentials("verifique o e-mail e a senha do PontoMais".to_string())
            }
            PontoMaisAuthFailure::Server(status) => DomainError::ExternalApiError(format!(
                "o PontoMais respondeu com status {status}; tente de novo em alguns minutos"
            )),
            PontoMaisAuthFailure::Network => DomainError::ExternalApiError(
                "não foi possível conectar ao PontoMais; verifique sua conexão".to_string(),
            ),
        }
    }
}

fn is_header_safe(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_FIELD_LEN && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PontoMaisCredentials {
        PontoMaisCredentials::new(
            "dev@acme.com".into(),
            "abcDEF123_-".into(),
            "client-1".into(),
            "1900000000".into(),
            "8c2f8f9e-2f4b-4b8e-9a51-6a2c3e7b1d10".into(),
        )
    }

    #[test]
    fn valid_credentials_pass_validation() {
        assert!(sample().validate().is_ok());
    }

    #[test]
    fn header_injection_and_bad_formats_are_rejected() {
        let mut creds = sample();
        creds.token = Zeroizing::new("abc\r\nx-evil: 1".into());
        assert!(creds.validate().is_err());

        let mut creds = sample();
        creds.uuid = "1234".into();
        assert!(creds.validate().is_err());

        let mut creds = sample();
        creds.expiry = "amanhã".into();
        assert!(creds.validate().is_err());
    }

    #[test]
    fn expiry_is_compared_with_now() {
        let creds = sample();
        assert!(!creds.is_expired(1_800_000_000));
        assert!(creds.is_expired(1_900_000_001));
    }

    #[test]
    fn login_requires_identifier_and_password() {
        assert!(
            PontoMaisLogin::new(" dev@acme.com ".into(), Zeroizing::new("s3nha forte".into()))
                .validate()
                .is_ok()
        );
        assert!(PontoMaisLogin::new("".into(), Zeroizing::new("x".into()))
            .validate()
            .is_err());
        assert!(PontoMaisLogin::new("dev@acme.com".into(), Zeroizing::new("".into()))
            .validate()
            .is_err());
        assert!(
            PontoMaisLogin::new("dev@acme.com\r\nx: 1".into(), Zeroizing::new("x".into()))
                .validate()
                .is_err()
        );
    }

    #[test]
    fn login_trims_identifier_and_hides_password() {
        let login = PontoMaisLogin::new("  dev@acme.com ".into(), Zeroizing::new("s3cr3t-pw".into()));
        assert_eq!(login.login, "dev@acme.com");
        assert!(!format!("{login:?}").contains("s3cr3t-pw"));
    }

    #[test]
    fn debug_does_not_leak_secrets() {
        let output = format!("{:?}", sample());
        assert!(!output.contains("abcDEF123"));
        assert!(!output.contains("client-1"));
    }
}
