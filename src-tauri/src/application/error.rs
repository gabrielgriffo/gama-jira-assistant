use crate::domain::error::DomainError;

/// The one error type that crosses the Tauri IPC boundary. Domain never
/// references this — it only knows `DomainError`. `AppError` exists purely
/// to give `presentation/commands` a single, serializable return type.
#[derive(Debug)]
pub enum AppError {
    Domain(DomainError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Domain(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        AppError::Domain(err)
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Domain(err) => match err {
                DomainError::InvalidCredentials(_) => "invalid_credentials",
                DomainError::InvalidInput(_) => "invalid_input",
                DomainError::AuthenticationFailed => "authentication_failed",
                DomainError::CredentialsNotFound => "credentials_not_found",
                DomainError::NotFound(_) => "not_found",
                DomainError::Network(_) => "network",
                DomainError::UnexpectedResponse(_) => "unexpected_response",
                DomainError::Storage(_) => "storage",
                DomainError::ExternalApiError(_) => "external_api",
                DomainError::ParseError(_) => "parse",
                DomainError::SessionExpired => "session_expired",
                DomainError::SyncCancelled => "sync_cancelled",
            },
        }
    }

    fn message(&self) -> String {
        match self {
            AppError::Domain(DomainError::Storage(reason)) => {
                log::error!("[APP] falha de armazenamento local: {reason}");
                "falha ao ler/gravar dados locais".to_string()
            }
            other => other.to_string(),
        }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.message())?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_credentials_cross_the_ipc_with_a_stable_code() {
        let json = serde_json::to_value(AppError::from(DomainError::CredentialsNotFound)).unwrap();
        assert_eq!(json["code"], "credentials_not_found");
        assert_eq!(json["message"], "nenhuma credencial do Jira foi configurada");
    }

    #[test]
    fn storage_errors_hide_the_internal_reason() {
        let json = serde_json::to_value(AppError::from(DomainError::Storage("disk path".into()))).unwrap();
        assert_eq!(json["code"], "storage");
        assert_eq!(json["message"], "falha ao ler/gravar dados locais");
    }
}
