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
        let mut state = self.read().unwrap_or_default();

        if uuid::Uuid::parse_str(&state.uuid).is_err() {
            state.uuid = uuid::Uuid::new_v4().to_string();
            state.registered_for = None;
            self.save(&state);
        }

        state
    }

    pub fn save(&self, state: &PontoMaisState) {
        if let Err(e) = self.write(state) {
            log::error!("[PONTOMAIS] falha ao gravar o estado local: {e}");
        }
    }

    fn read(&self) -> Option<PontoMaisState> {
        let metadata = fs::metadata(&self.file_path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_STATE_FILE_BYTES {
            return None;
        }
        let json = fs::read_to_string(&self.file_path).ok()?;
        serde_json::from_str(&json).ok()
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
}
