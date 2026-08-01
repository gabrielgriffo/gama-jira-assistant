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

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AppError::Domain(DomainError::Storage(reason)) => {
                log::error!("[APP] falha de armazenamento local: {reason}");
                serializer.serialize_str("falha ao ler/gravar dados locais")
            }
            other => serializer.serialize_str(&other.to_string()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
