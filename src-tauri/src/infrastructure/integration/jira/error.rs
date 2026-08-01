use crate::domain::error::DomainError;

#[derive(Debug)]
pub enum JiraError {
    Http(String),
    Unauthorized,
    NotFound,
    UnexpectedStatus(u16),
    Deserialization(String),
}

impl std::fmt::Display for JiraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JiraError::Http(reason) => write!(f, "http request to jira failed: {reason}"),
            JiraError::Unauthorized => write!(f, "jira rejected the credentials"),
            JiraError::NotFound => write!(f, "jira resource not found"),
            JiraError::UnexpectedStatus(status) => write!(f, "unexpected status {status} from jira"),
            JiraError::Deserialization(reason) => {
                write!(f, "failed to parse jira response: {reason}")
            }
        }
    }
}

impl std::error::Error for JiraError {}

impl From<JiraError> for DomainError {
    fn from(err: JiraError) -> Self {
        match err {
            JiraError::Unauthorized => DomainError::AuthenticationFailed,
            JiraError::NotFound => DomainError::UnexpectedResponse("recurso não encontrado no Jira".to_string()),
            JiraError::Http(reason) => DomainError::Network(reason),
            JiraError::UnexpectedStatus(status) => DomainError::UnexpectedResponse(format!("status {status}")),
            JiraError::Deserialization(reason) => DomainError::UnexpectedResponse(reason),
        }
    }
}
