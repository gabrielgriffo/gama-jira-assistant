use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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
    unreadable: AtomicBool,
    io_lock: Mutex<()>,
}

impl PontoMaisStateStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            file_path: app_data_dir.join("pontomais_state.json"),
            unreadable: AtomicBool::new(false),
            io_lock: Mutex::new(()),
        }
    }

    pub fn load(&self) -> PontoMaisState {
        let _guard = self.io_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.load_from(self.read())
    }

    pub fn save(&self, state: &PontoMaisState) {
        let _guard = self.io_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.save_unlocked(state);
    }

    fn load_from(&self, read: std::io::Result<Option<String>>) -> PontoMaisState {
        self.unreadable.store(read.is_err(), Ordering::SeqCst);
        let (state, persist) = resolve_state(read);
        if persist {
            self.save_unlocked(&state);
        }
        state
    }

    fn save_unlocked(&self, state: &PontoMaisState) {
        if self.unreadable.load(Ordering::SeqCst) {
            log::warn!(
                "[PONTOMAIS] estado local ilegível na última leitura; gravação ignorada para preservar o arquivo"
            );
            return;
        }
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
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
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
    fn saving_after_a_failed_read_keeps_the_file_untouched() {
        let dir = temp_dir();
        let saved = r#"{"uuid":"6f9619ff-8b86-4d01-b42d-00cf4fc964ff","account":"dev@acme.com","autoReconnect":true}"#;
        fs::write(dir.join("pontomais_state.json"), saved).unwrap();
        let store = PontoMaisStateStore::new(dir.clone());

        let mut state = store.load_from(Err(std::io::Error::other("arquivo bloqueado")));
        state.last_used_at = Some(1);
        store.save(&state);

        assert_eq!(fs::read_to_string(dir.join("pontomais_state.json")).unwrap(), saved);
        let reloaded = store.load();
        assert!(reloaded.auto_reconnect);
        store.save(&reloaded);
        assert!(store.load().auto_reconnect);

        fs::remove_dir_all(dir).ok();
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
