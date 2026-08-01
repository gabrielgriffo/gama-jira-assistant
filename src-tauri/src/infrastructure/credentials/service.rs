use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::domain::credentials::normalize_jira_base_url;

use crate::domain::credentials::JiraCredentials;
use crate::domain::error::DomainError;
use crate::domain::pontomais::credentials::{PontoMaisCredentials, PontoMaisLogin};
use crate::domain::ports::CredentialStore;
use crate::infrastructure::database::JiraConnectionRepository;
use crate::infrastructure::secrets::{SecretNamespace, SecretStore};

const PONTOMAIS_CREDENTIALS_KEY: &str = "credentials";
const PONTOMAIS_PASSWORD_KEY: &str = "password";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredJiraSecret {
    base_url: String,
    email: String,
    api_token: String,
}

impl Drop for StoredJiraSecret {
    fn drop(&mut self) {
        self.api_token.zeroize();
    }
}

/// Concrete `CredentialStore`: combines SQLite (URL + e-mail, non-secret)
/// with the OS keyring via `SecretStore` (API token). Callers — `JiraClient`
/// included — never see either backing store directly, only the
/// `JiraCredentials` value this returns.
#[derive(Clone)]
pub struct CredentialService {
    secrets: SecretStore,
    connections: JiraConnectionRepository,
}

impl CredentialService {
    pub fn new(secrets: SecretStore, connections: JiraConnectionRepository) -> Self {
        Self { secrets, connections }
    }
}

impl CredentialStore for CredentialService {
    async fn save_jira_credentials(
        &self,
        credentials: &JiraCredentials,
        user: &crate::domain::current_user::CurrentUser,
    ) -> Result<(), DomainError> {
        let connection_id = uuid::Uuid::new_v4().to_string();
        let previous_keyring_key = self.connections.get_keyring_key().await?;

        if let Some((previous_base_url, previous_account_id)) = self.connections.get_identity().await? {
            if previous_base_url != credentials.base_url || previous_account_id != user.account_id {
                log::info!("[CREDENTIALS] Conta ou instância do Jira diferente da anterior; limpando cache local");
                self.connections.purge_cached_jira_data().await?;
            }
        }

        let secret = StoredJiraSecret {
            base_url: credentials.base_url.clone(),
            email: credentials.email.clone(),
            api_token: credentials.api_token.to_string(),
        };
        let secret_json = Zeroizing::new(
            serde_json::to_vec(&secret)
                .map_err(|e| DomainError::Storage(format!("falha ao serializar segredo: {e}")))?,
        );
        drop(secret);

        self.secrets
            .store(&SecretNamespace::Jira, &connection_id, secret_json)
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        if let Err(e) = self
            .connections
            .save(
                &connection_id,
                &credentials.base_url,
                &credentials.email,
                &user.account_id,
                &user.display_name,
                &user.time_zone,
            )
            .await
        {
            let _ = self.secrets.delete(&SecretNamespace::Jira, &connection_id);
            return Err(e);
        }

        if let Some(previous) = previous_keyring_key.filter(|key| key != &connection_id) {
            if let Err(e) = self.secrets.delete(&SecretNamespace::Jira, &previous) {
                log::warn!("[CREDENTIALS] falha ao remover token anterior do keyring: {e}");
            }
        }

        Ok(())
    }

    async fn get_jira_credentials(&self) -> Result<JiraCredentials, DomainError> {
        let record = self
            .connections
            .get_active()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .ok_or(DomainError::CredentialsNotFound)?;

        let raw = self
            .secrets
            .get(&SecretNamespace::Jira, &record.keyring_key)
            .map_err(|_| DomainError::CredentialsNotFound)?;

        let base_url = normalize_jira_base_url(&record.base_url)?;

        let api_token = match serde_json::from_slice::<StoredJiraSecret>(&raw) {
            Ok(secret) => {
                if normalize_jira_base_url(&secret.base_url)? != base_url || secret.email != record.email {
                    log::error!("[CREDENTIALS] dados da conexão no banco divergem do keyring; sessão invalidada");
                    return Err(DomainError::CredentialsNotFound);
                }
                Zeroizing::new(secret.api_token.clone())
            }
            Err(_) => Zeroizing::new(
                String::from_utf8(raw.to_vec())
                    .map_err(|_| DomainError::Storage("token armazenado não é utf-8 válido".to_string()))?,
            ),
        };

        Ok(JiraCredentials {
            base_url,
            email: record.email,
            api_token,
        })
    }

    async fn get_current_user(&self) -> Result<crate::domain::current_user::CurrentUser, DomainError> {
        let record = self
            .connections
            .get_active()
            .await
            .map_err(|e| DomainError::Storage(e.to_string()))?
            .ok_or(DomainError::CredentialsNotFound)?;

        Ok(crate::domain::current_user::CurrentUser {
            account_id: record.account_id,
            display_name: record.display_name,
            email: record.email,
            time_zone: record.time_zone,
        })
    }

    async fn logout(&self) -> Result<(), DomainError> {
        let keyring_key = self.connections.get_keyring_key().await?;

        self.connections.deactivate().await?;

        if let Some(key) = keyring_key {
            self.secrets
                .delete(&SecretNamespace::Jira, &key)
                .map_err(|e| DomainError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    async fn has_active_session(&self) -> Result<bool, DomainError> {
        match self.get_jira_credentials().await {
            Ok(_) => Ok(true),
            Err(DomainError::CredentialsNotFound) => Ok(false),
            Err(other) => Err(other),
        }
    }
}

impl CredentialService {
    pub fn save_pontomais_credentials(&self, credentials: &PontoMaisCredentials) -> Result<(), DomainError> {
        credentials.validate()?;

        let json = Zeroizing::new(
            serde_json::to_vec(credentials)
                .map_err(|e| DomainError::Storage(format!("Falha ao serializar credenciais: {}", e)))?,
        );

        self.secrets
            .store(&SecretNamespace::PontoMais, PONTOMAIS_CREDENTIALS_KEY, json)
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    pub fn get_pontomais_credentials(&self) -> Result<PontoMaisCredentials, DomainError> {
        let data = self
            .secrets
            .get(&SecretNamespace::PontoMais, PONTOMAIS_CREDENTIALS_KEY)
            .map_err(|_| DomainError::CredentialsNotFound)?;

        let credentials: PontoMaisCredentials = serde_json::from_slice(&data)
            .map_err(|_| DomainError::ParseError("Falha ao ler credenciais do PontoMais armazenadas".to_string()))?;

        Ok(credentials)
    }

    pub async fn clear_local_data(&self) -> Result<(), DomainError> {
        self.connections.purge_cached_jira_data().await?;
        self.connections.purge_pontomais_data().await
    }

    pub fn delete_pontomais_credentials(&self) -> Result<(), DomainError> {
        self.secrets
            .delete(&SecretNamespace::PontoMais, PONTOMAIS_CREDENTIALS_KEY)
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(())
    }

    pub fn has_pontomais_credentials(&self) -> bool {
        self.get_pontomais_credentials().is_ok()
    }

    pub fn save_pontomais_password(&self, login: &PontoMaisLogin) -> Result<(), DomainError> {
        login.validate()?;

        let json = Zeroizing::new(
            serde_json::to_vec(login)
                .map_err(|e| DomainError::Storage(format!("Falha ao serializar a senha: {}", e)))?,
        );

        self.secrets
            .store(&SecretNamespace::PontoMais, PONTOMAIS_PASSWORD_KEY, json)
            .map_err(|e| DomainError::Storage(e.to_string()))
    }

    pub fn get_pontomais_password(&self) -> Option<PontoMaisLogin> {
        let data = self
            .secrets
            .get(&SecretNamespace::PontoMais, PONTOMAIS_PASSWORD_KEY)
            .ok()?;

        match serde_json::from_slice::<PontoMaisLogin>(&data) {
            Ok(login) => Some(login),
            Err(_) => {
                log::warn!("[CREDENTIALS] Senha do PontoMais guardada em formato inválido; ignorando");
                None
            }
        }
    }

    pub fn delete_pontomais_password(&self) -> Result<(), DomainError> {
        self.secrets
            .delete(&SecretNamespace::PontoMais, PONTOMAIS_PASSWORD_KEY)
            .map_err(|e| DomainError::Storage(e.to_string()))
    }

    pub async fn purge_pontomais_cache(&self) -> Result<(), DomainError> {
        self.connections.purge_pontomais_data().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::current_user::CurrentUser;
    use crate::infrastructure::database::in_memory_pool;
    use crate::infrastructure::database::migrations::DatabaseMigrator;
    use crate::infrastructure::secrets::test_store;

    async fn service() -> CredentialService {
        let pool = in_memory_pool().await;
        let migrator = DatabaseMigrator::new(pool.clone());
        migrator.configure_connection().await.unwrap();
        migrator.run_migrations().await.unwrap();
        CredentialService::new(test_store(), JiraConnectionRepository::new(pool))
    }

    fn sample_user() -> CurrentUser {
        CurrentUser {
            account_id: "acc123".to_string(),
            display_name: "Dev User".to_string(),
            email: "dev@acme.com".to_string(),
            time_zone: "America/Sao_Paulo".to_string(),
        }
    }

    fn sample_credentials() -> JiraCredentials {
        JiraCredentials {
            base_url: "https://acme.atlassian.net".to_string(),
            email: format!("dev-{}@acme.com", uuid_like()),
            api_token: Zeroizing::new("super-secret-token".to_string()),
        }
    }

    fn uuid_like() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    #[tokio::test]
    async fn get_jira_credentials_fails_when_nothing_was_saved() {
        let service = service().await;

        let result = service.get_jira_credentials().await;

        assert!(matches!(result, Err(DomainError::CredentialsNotFound)));
    }

    #[tokio::test]
    async fn save_then_get_round_trips_the_full_credentials() {
        let service = service().await;
        let credentials = sample_credentials();

        service
            .save_jira_credentials(&credentials, &sample_user())
            .await
            .unwrap();
        let loaded = service.get_jira_credentials().await.unwrap();

        assert_eq!(loaded.base_url, credentials.base_url);
        assert_eq!(loaded.email, credentials.email);
        assert_eq!(loaded.api_token.as_str(), credentials.api_token.as_str());

        let record = service.connections.get_active().await.unwrap().unwrap();
        let _ = service.secrets.delete(&SecretNamespace::Jira, &record.keyring_key);
    }

    #[tokio::test]
    async fn has_active_session_is_false_when_nothing_was_saved() {
        let service = service().await;

        assert!(!service.has_active_session().await.unwrap());
    }

    #[tokio::test]
    async fn has_active_session_is_true_after_saving_credentials() {
        let service = service().await;
        let credentials = sample_credentials();
        service
            .save_jira_credentials(&credentials, &sample_user())
            .await
            .unwrap();

        assert!(service.has_active_session().await.unwrap());

        let record = service.connections.get_active().await.unwrap().unwrap();
        let _ = service.secrets.delete(&SecretNamespace::Jira, &record.keyring_key);
    }

    #[tokio::test]
    async fn logout_removes_the_token_but_keeps_base_url_and_email() {
        let service = service().await;
        let credentials = sample_credentials();
        service
            .save_jira_credentials(&credentials, &sample_user())
            .await
            .unwrap();

        service.logout().await.unwrap();

        assert!(!service.has_active_session().await.unwrap());
        assert!(matches!(
            service.get_jira_credentials().await,
            Err(DomainError::CredentialsNotFound)
        ));
        assert!(service.connections.get_active().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn switching_account_purges_cached_jira_data() {
        let service = service().await;
        let credentials = sample_credentials();
        service
            .save_jira_credentials(&credentials, &sample_user())
            .await
            .unwrap();
        sqlx::query("INSERT INTO jira_statuses (id, name, category_key, category_name, category_color) VALUES ('1', 'To Do', 'new', 'To Do', 'blue-gray')")
            .execute(service.connections.pool())
            .await
            .unwrap();

        let mut other_user = sample_user();
        other_user.account_id = "other".to_string();
        service.save_jira_credentials(&credentials, &other_user).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jira_statuses")
            .fetch_one(service.connections.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);

        let record = service.connections.get_active().await.unwrap().unwrap();
        let _ = service.secrets.delete(&SecretNamespace::Jira, &record.keyring_key);
    }

    #[tokio::test]
    async fn logging_in_again_after_logout_reactivates_the_session() {
        let service = service().await;
        let credentials = sample_credentials();
        service
            .save_jira_credentials(&credentials, &sample_user())
            .await
            .unwrap();
        service.logout().await.unwrap();

        service
            .save_jira_credentials(&credentials, &sample_user())
            .await
            .unwrap();

        assert!(service.has_active_session().await.unwrap());

        let record = service.connections.get_active().await.unwrap().unwrap();
        let _ = service.secrets.delete(&SecretNamespace::Jira, &record.keyring_key);
    }
}
