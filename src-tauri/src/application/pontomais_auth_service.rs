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

        let auto_reconnect = auto_reconnect && self.store_password_for_reconnect(&login, auto_reconnect);

        state.account = Some(credentials.email.clone());
        state.auto_reconnect = auto_reconnect;
        state.failed_attempts = 0;
        mark_signed_in(&mut state, now());
        self.state.save(&state);

        self.register_device(&credentials, &mut state).await;
        log::info!(
            "[PONTOMAIS] Conta conectada pelo usuário (reconexão automática {})",
            if auto_reconnect { "ativada" } else { "desativada" }
        );
        Ok(())
    }

    fn store_password_for_reconnect(&self, login: &PontoMaisLogin, auto_reconnect: bool) -> bool {
        if !auto_reconnect {
            if let Err(e) = self.credentials.delete_pontomais_password() {
                log::error!("[PONTOMAIS] Falha ao remover a senha do cofre: {e}");
            }
            return false;
        }
        match self.credentials.save_pontomais_password(login) {
            Ok(()) => true,
            Err(e) => {
                log::error!("[PONTOMAIS] Falha ao guardar a senha no cofre; reconexão automática desativada: {e}");
                if let Err(e) = self.credentials.delete_pontomais_password() {
                    log::error!("[PONTOMAIS] Falha ao remover a senha anterior do cofre: {e}");
                }
                false
            }
        }
    }

    pub async fn disconnect(&self) -> Result<(), DomainError> {
        let _guard = self.session_lock.lock().await;

        if let Ok(credentials) = self.credentials.get_pontomais_credentials() {
            if let Err(e) = self.client.sign_out(&credentials).await {
                log::warn!("[PONTOMAIS] sign_out não confirmado: {e}");
            }
        }

        self.forget_session()?;
        log::info!("[PONTOMAIS] Conta desconectada pelo usuário");
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
        log::info!("[PONTOMAIS] {}", session_age_report(&state, failed, now()));
        let stored = if state.auto_reconnect {
            self.credentials.get_pontomais_password()?
        } else {
            None
        };

        let Some(login) = stored else {
            log::info!("[PONTOMAIS] Conta desconectada: sessão expirada e reconexão automática desativada");
            self.forget_session()?;
            return Err(DomainError::SessionExpired);
        };

        log::info!("[PONTOMAIS] Sessão expirada; reconectando com a senha guardada");
        match self.client.sign_in(&login, &state.uuid).await {
            Ok(credentials) => {
                self.credentials.save_pontomais_credentials(&credentials)?;
                state.account = Some(credentials.email.clone());
                state.failed_attempts = 0;
                mark_signed_in(&mut state, now());
                self.state.save(&state);
                self.register_device(&credentials, &mut state).await;
                log::info!("[PONTOMAIS] Conta reconectada automaticamente com a senha guardada");
                Ok(credentials)
            }
            Err(failure) if is_login_refusal(&failure) => {
                state.failed_attempts = state.failed_attempts.saturating_add(1);
                self.state.save(&state);

                if state.failed_attempts >= MAX_FAILED_RECONNECTS {
                    log::warn!(
                        "[PONTOMAIS] Conta desconectada: senha guardada recusada {MAX_FAILED_RECONNECTS} vezes na reconexão automática"
                    );
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

    pub async fn mark_session_used(&self) {
        let _guard = self.session_lock.lock().await;
        let mut state = self.state.load();
        state.last_used_at = Some(now());
        self.state.save(&state);
    }

    fn forget_session(&self) -> Result<(), DomainError> {
        self.credentials.delete_pontomais_credentials()?;
        self.credentials.delete_pontomais_password()?;

        let mut state = self.state.load();
        state.failed_attempts = 0;
        state.signed_in_at = None;
        state.last_used_at = None;
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

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn mark_signed_in(state: &mut PontoMaisState, at: i64) {
    state.signed_in_at = Some(at);
    state.last_used_at = Some(at);
}

fn hours_between(from: i64, to: i64) -> String {
    format!("{:.1} h", (to - from) as f64 / 3600.0)
}

fn session_age_report(state: &PontoMaisState, credentials: &PontoMaisCredentials, now: i64) -> String {
    let since_login = state
        .signed_in_at
        .map_or_else(|| "desconhecido".to_string(), |at| hours_between(at, now));
    let since_use = state
        .last_used_at
        .map_or_else(|| "desconhecido".to_string(), |at| hours_between(at, now));
    let deadline = match credentials.expiry.parse::<i64>() {
        Ok(expiry) if expiry > now => format!("faltavam {} para o vencimento", hours_between(now, expiry)),
        Ok(expiry) => format!("vencida há {}", hours_between(expiry, now)),
        Err(_) => "vencimento desconhecido".to_string(),
    };
    format!("Sessão recusada: {since_login} desde o login, {since_use} desde o último uso, {deadline}")
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

    fn credentials_expiring_at(expiry: i64) -> PontoMaisCredentials {
        PontoMaisCredentials::new(
            "dev@acme.com".into(),
            "tok".into(),
            "cli".into(),
            expiry.to_string(),
            "6f9619ff-8b86-4d01-b42d-00cf4fc964ff".into(),
        )
    }

    #[test]
    fn the_refusal_report_measures_the_session_age() {
        let login = 1_000_000;
        let mut state = PontoMaisState::default();
        mark_signed_in(&mut state, login);
        state.last_used_at = Some(login + 24 * 3600);
        let now = login + 81 * 3600 + 1800;

        let report = session_age_report(&state, &credentials_expiring_at(login + 180 * 3600), now);

        assert_eq!(
            report,
            "Sessão recusada: 81.5 h desde o login, 57.5 h desde o último uso, faltavam 98.5 h para o vencimento"
        );
    }

    #[test]
    fn the_refusal_report_handles_unknown_times_and_expired_sessions() {
        let now = 1_000_000;
        let report = session_age_report(&PontoMaisState::default(), &credentials_expiring_at(now - 7200), now);

        assert_eq!(
            report,
            "Sessão recusada: desconhecido desde o login, desconhecido desde o último uso, vencida há 2.0 h"
        );
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
