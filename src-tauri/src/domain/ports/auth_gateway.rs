use crate::domain::credentials::JiraCredentials;
use crate::domain::current_user::CurrentUser;
use crate::domain::error::DomainError;

/// Port: verifying that a set of credentials is accepted by the issue
/// tracker. Implemented by infrastructure (JiraProvider) — domain and
/// application never know it means an HTTP GET to /myself.
pub trait AuthGateway: Send + Sync {
    async fn verify_credentials(&self, credentials: &JiraCredentials) -> Result<CurrentUser, DomainError>;
}
