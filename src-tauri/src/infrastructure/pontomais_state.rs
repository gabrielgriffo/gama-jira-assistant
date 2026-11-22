use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const MAX_STATE_FILE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PontoMaisState {
    pub uuid: String,
    pub registered_for: Option<String>,
    pub account: Option<String>,
    pub auto_reconnect: bool,
    pub failed_attempts: u32,
    pub signed_in_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

pub struct PontoMaisStateStore {
    file_path: PathBuf,
}

impl PontoMaisStateStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            file_path: app_data_dir.join("pontomais_state.json"),
        }
    }

    pub fn load(&self) -> PontoMaisState {
        let (state, persist) = resolve_state(self.read());
        if persist {
            self.save(&state);
        }
        state
    }

    pub fn save(&self, state: &PontoMaisState) {
        if let Err(e) = self.write(state) {
            log::error!("[PONTOMAIS] falha ao gravar o estado local: {e}");
        }
    }

    fn read(&self) -> std::io::Result<Option<String>> {
        let metadata = match fs::metadata(&self.file_path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if !metadata.is_file() || metadata.len() > MAX_STATE_FILE_BYTES {
            return Ok(None);
        }
        fs::read_to_string(&self.file_path).map(Some)
    }

    fn write(&self, state: &PontoMaisState) -> std::io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = self.file_path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, &self.file_path)
    }
}

fn resolve_state(read: std::io::Result<Option<String>>) -> (PontoMaisState, bool) {
    let contents = match read {
        Ok(contents) => contents,
        Err(e) => {
            log::warn!("[PONTOMAIS] não foi possível ler o estado local; mantendo o arquivo como está: {e}");
            let state = PontoMaisState {
                uuid: uuid::Uuid::new_v4().to_string(),
                ..PontoMaisState::default()
            };
            return (state, false);
        }
    };
    let mut state: PontoMaisState = contents
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    if uuid::Uuid::parse_str(&state.uuid).is_ok() {
        return (state, false);
    }
    state.uuid = uuid::Uuid::new_v4().to_string();
    state.registered_for = None;
    (state, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gama-pontomais-state-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_and_persists_a_device_uuid() {
        let dir = temp_dir();
        let store = PontoMaisStateStore::new(dir.clone());

        let first = store.load();
        assert!(uuid::Uuid::parse_str(&first.uuid).is_ok());
        assert_eq!(store.load().uuid, first.uuid);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn corrupted_file_gets_a_new_uuid_and_drops_registration() {
        let dir = temp_dir();
        fs::write(
            dir.join("pontomais_state.json"),
            r#"{"uuid":"nope","registeredFor":"dev@acme.com","autoReconnect":true}"#,
        )
        .unwrap();
        let state = PontoMaisStateStore::new(dir.clone()).load();

        assert!(uuid::Uuid::parse_str(&state.uuid).is_ok());
        assert_eq!(state.registered_for, None);
        assert!(state.auto_reconnect);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_failed_read_does_not_overwrite_the_saved_state() {
        let (state, persist) = resolve_state(Err(std::io::Error::other("arquivo bloqueado")));

        assert!(!persist);
        assert!(uuid::Uuid::parse_str(&state.uuid).is_ok());
    }

    #[test]
    fn a_valid_state_is_kept_without_rewriting() {
        let saved = r#"{"uuid":"6f9619ff-8b86-4d01-b42d-00cf4fc964ff","account":"dev@acme.com","autoReconnect":true}"#;
        let (state, persist) = resolve_state(Ok(Some(saved.to_string())));

        assert!(!persist);
        assert_eq!(state.account.as_deref(), Some("dev@acme.com"));
        assert!(state.auto_reconnect);
    }
}
