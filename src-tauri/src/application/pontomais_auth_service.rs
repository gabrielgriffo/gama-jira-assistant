use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::domain::error::DomainError;
use crate::domain::pontomais::credentials::{PontoMaisAuthFailure, PontoMaisCredentials, PontoMaisLogin};
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::integration::pontomais::PontoMaisClient;
use crate::infrastructure::pontomais_state::{PontoMaisState, PontoMaisStateStore};

pub const MAX_FAILED_RECONNECTS: u32 = 2;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PontoMaisStatus {
    pub connected: bool,
    pub account: Option<String>,
    pub auto_reconnect: bool,
}

#[derive(Clone)]
pub struct PontoMaisAuthService {
    client: PontoMaisClient,
    credentials: CredentialService,
    state: Arc<PontoMaisStateStore>,
    session_lock: Arc<Mutex<()>>,
}

impl PontoMaisAuthService {
    pub fn new(client: PontoMaisClient, credentials: CredentialService, state: PontoMaisStateStore) -> Self {
        Self {
            client,
            credentials,
            state: Arc::new(state),
            session_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn status(&self) -> PontoMaisStatus {
        let state = self.state.load();
        status_from(self.credentials.get_pontomais_credentials(), state)
    }

    pub fn is_connected(&self) -> bool {
        self.credentials.has_pontomais_credentials()
    }

    pub fn credentials(&self) -> Result<PontoMaisCredentials, DomainError> {
        self.credentials.get_pontomais_credentials()
    }

    pub async fn connect(&self, login: PontoMaisLogin, auto_reconnect: bool) -> Result<(), DomainError> {
        login.validate()?;
        let _guard = self.session_lock.lock().await;

        let mut state = self.state.load();
        let credentials = self.client.sign_in(&login, &state.uuid).await?;

        let previous_account = self
            .credentials
            .get_pontomais_credentials()
            .ok()
            .map(|c| c.email)
            .or_else(|| state.account.clone());
        if previous_account.is_some_and(|previous| !previous.eq_ignore_ascii_case(&credentials.email)) {
            log::info!("[PONTOMAIS] Conta diferente da anterior; limpando batidas em cache");
            self.credentials.purge_pontomais_cache().await?;
        }

        self.credentials.save_pontomais_credentials(&credentials)?;

        let password_result = if auto_reconnect {
            self.credentials.save_pontomais_password(&login)
        } else {
            self.credentials.delete_pontomais_password()
        };
        if let Err(e) = password_result {
            log::error!("[PONTOMAIS] Falha ao atualizar a senha no cofre: {e}");
        }

        state.account = Some(credentials.email.clone());
        state.auto_reconnect = auto_reconnect;
        state.failed_attempts = 0;
        self.state.save(&state);

        self.register_device(&credentials, &mut state).await;
        log::info!("[PONTOMAIS] Conta conectada");
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), DomainError> {
        let _guard = self.session_lock.lock().await;

        if let Ok(credentials) = self.credentials.get_pontomais_credentials() {
            if let Err(e) = self.client.sign_out(&credentials).await {
                log::warn!("[PONTOMAIS] sign_out não confirmado: {e}");
            }
        }

        self.forget_session()?;
        log::info!("[PONTOMAIS] Conta desconectada");
        Ok(())
    }

    pub async fn reconnect(&self, failed: &PontoMaisCredentials) -> Result<PontoMaisCredentials, DomainError> {
        let _guard = self.session_lock.lock().await;

        let current = match self.credentials.get_pontomais_credentials() {
            Ok(current) => Some(current),
            Err(DomainError::CredentialsNotFound) => None,
            Err(other) => return Err(other),
        };
        match current {
            Some(current) if current.token.as_str() != failed.token.as_str() => return Ok(current),
            None => return Err(DomainError::SessionExpired),
            Some(_) => {}
        }

        let mut state = self.state.load();
        let stored = if state.auto_reconnect {
            self.credentials.get_pontomais_password()?
        } else {
            None
        };

        let Some(login) = stored else {
            log::info!("[PONTOMAIS] Sessão expirada e sem senha guardada; desconectando");
            self.forget_session()?;
            return Err(DomainError::SessionExpired);
        };

        log::info!("[PONTOMAIS] Sessão expirada; reconectando com a senha guardada");
        match self.client.sign_in(&login, &state.uuid).await {
            Ok(credentials) => {
                self.credentials.save_pontomais_credentials(&credentials)?;
                state.account = Some(credentials.email.clone());
                state.failed_attempts = 0;
                self.state.save(&state);
                self.register_device(&credentials, &mut state).await;
                Ok(credentials)
            }
            Err(failure) if is_login_refusal(&failure) => {
                state.failed_attempts = state.failed_attempts.saturating_add(1);
                self.state.save(&state);

                if state.failed_attempts >= MAX_FAILED_RECONNECTS {
                    log::warn!("[PONTOMAIS] Senha guardada recusada {MAX_FAILED_RECONNECTS} vezes; desconectando");
                    self.forget_session()?;
                    Err(DomainError::SessionExpired)
                } else {
                    log::warn!("[PONTOMAIS] Senha guardada recusada; nova tentativa na próxima sincronização");
                    Err(DomainError::ExternalApiError(
                        "o PontoMais recusou a senha guardada; nova tentativa na próxima sincronização".to_string(),
                    ))
                }
            }
            Err(failure) => {
                log::warn!("[PONTOMAIS] Reconexão adiada: {failure:?}");
                Err(DomainError::ExternalApiError(
                    "não foi possível reconectar ao PontoMais agora; tente de novo em alguns minutos".to_string(),
                ))
            }
        }
    }

    fn forget_session(&self) -> Result<(), DomainError> {
        self.credentials.delete_pontomais_credentials()?;
        self.credentials.delete_pontomais_password()?;

        let mut state = self.state.load();
        state.failed_attempts = 0;
        self.state.save(&state);
        Ok(())
    }

    async fn register_device(&self, credentials: &PontoMaisCredentials, state: &mut PontoMaisState) {
        if state.registered_for.as_deref() == Some(credentials.email.as_str()) && state.uuid == credentials.uuid {
            return;
        }

        match self.client.register_device(credentials).await {
            Ok(()) => {
                state.registered_for = Some(credentials.email.clone());
                self.state.save(state);
            }
            Err(e) => log::warn!("[PONTOMAIS] Registro do dispositivo não confirmado: {e}"),
        }
    }
}

fn is_login_refusal(failure: &PontoMaisAuthFailure) -> bool {
    match failure {
        PontoMaisAuthFailure::InvalidCredentials => true,
        PontoMaisAuthFailure::Server(status) => (400..500).contains(status) && *status != 408 && *status != 429,
        PontoMaisAuthFailure::Network => false,
    }
}

fn status_from(credentials: Result<PontoMaisCredentials, DomainError>, state: PontoMaisState) -> PontoMaisStatus {
    let (connected, email) = match credentials {
        Ok(credentials) => (true, Some(credentials.email)),
        Err(DomainError::CredentialsNotFound) => (false, None),
        Err(e) => {
            log::warn!("[PONTOMAIS] não foi possível ler as credenciais salvas: {e}");
            (true, None)
        }
    };
    PontoMaisStatus {
        connected,
        account: email.or(state.account),
        auto_reconnect: state.auto_reconnect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_vault_keeps_the_status_connected_like_the_sync() {
        let state = PontoMaisState {
            account: Some("dev@acme.com".to_string()),
            ..PontoMaisState::default()
        };

        let unreadable = status_from(Err(DomainError::Storage("cofre bloqueado".into())), state.clone());
        let missing = status_from(Err(DomainError::CredentialsNotFound), state);

        assert!(unreadable.connected);
        assert_eq!(unreadable.account.as_deref(), Some("dev@acme.com"));
        assert!(!missing.connected);
    }

    #[test]
    fn login_refusals_count_toward_the_reconnect_limit() {
        assert!(is_login_refusal(&PontoMaisAuthFailure::InvalidCredentials));
        assert!(is_login_refusal(&PontoMaisAuthFailure::Server(403)));
        assert!(is_login_refusal(&PontoMaisAuthFailure::Server(422)));
        assert!(!is_login_refusal(&PontoMaisAuthFailure::Server(429)));
        assert!(!is_login_refusal(&PontoMaisAuthFailure::Server(503)));
        assert!(!is_login_refusal(&PontoMaisAuthFailure::Network));
    }
}
