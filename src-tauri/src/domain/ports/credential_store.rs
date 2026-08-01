use std::future::Future;

use crate::domain::credentials::JiraCredentials;
use crate::domain::current_user::CurrentUser;
use crate::domain::error::DomainError;

pub trait CredentialStore: Send + Sync {
    fn save_jira_credentials(
        &self,
        credentials: &JiraCredentials,
        user: &CurrentUser,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;
    fn get_jira_credentials(&self) -> impl Future<Output = Result<JiraCredentials, DomainError>> + Send;
    fn get_current_user(&self) -> impl Future<Output = Result<CurrentUser, DomainError>> + Send;
    fn logout(&self) -> impl Future<Output = Result<(), DomainError>> + Send;
    fn has_active_session(&self) -> impl Future<Output = Result<bool, DomainError>> + Send;
}
