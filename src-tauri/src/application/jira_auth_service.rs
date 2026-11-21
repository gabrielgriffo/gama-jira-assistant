use zeroize::Zeroizing;

use super::error::AppResult;
use crate::domain::credentials::{normalize_jira_base_url, JiraCredentials};
use crate::domain::current_user::CurrentUser;
use crate::domain::error::DomainError;
use crate::domain::ports::{AuthGateway, CredentialStore};

const MAX_EMAIL_LEN: usize = 254;
const MAX_API_TOKEN_LEN: usize = 1024;

pub struct JiraAuthService<G: AuthGateway, C: CredentialStore> {
    gateway: G,
    credentials: C,
}

impl<G: AuthGateway, C: CredentialStore> JiraAuthService<G, C> {
    pub fn new(gateway: G, credentials: C) -> Self {
        Self { gateway, credentials }
    }

    pub async fn login(&self, base_url: &str, email: &str, api_token: &str) -> AppResult<CurrentUser> {
        let credentials = build_credentials(base_url, email, api_token)?;

        let user = self.gateway.verify_credentials(&credentials).await?;
        self.credentials.save_jira_credentials(&credentials, &user).await?;

        Ok(user)
    }

    pub async fn logout(&self) -> AppResult<()> {
        self.credentials.logout().await?;
        Ok(())
    }

    pub async fn connection_status(&self) -> AppResult<bool> {
        Ok(self.credentials.has_active_session().await?)
    }
}

fn build_credentials(base_url: &str, email: &str, api_token: &str) -> Result<JiraCredentials, DomainError> {
    let base_url = base_url.trim();
    let email = email.trim();
    let api_token = api_token.trim();

    if base_url.is_empty() || email.is_empty() || api_token.is_empty() {
        return Err(DomainError::InvalidCredentials(
            "URL, e-mail e token são obrigatórios".to_string(),
        ));
    }
    if !email.contains('@') || email.len() > MAX_EMAIL_LEN {
        return Err(DomainError::InvalidCredentials("E-mail inválido".to_string()));
    }
    if api_token.len() > MAX_API_TOKEN_LEN {
        return Err(DomainError::InvalidCredentials("Token inválido".to_string()));
    }

    Ok(JiraCredentials {
        base_url: normalize_jira_base_url(base_url)?,
        email: email.to_string(),
        api_token: Zeroizing::new(api_token.to_string()),
    })
}
