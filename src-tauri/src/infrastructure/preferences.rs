use crate::domain::date_range::DateRange;
use crate::domain::error::DomainError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub const DEFAULT_MIN_SYNC_INTERVAL_MINUTES: u32 = 5;
pub const MAX_MIN_SYNC_INTERVAL_MINUTES: u32 = 24 * 60;
pub const MANUAL_SYNC_ONLY: u32 = u32::MAX;
const MAX_PRESET_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub version: u32,
    pub theme: Theme,
    pub min_sync_interval_minutes: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks_range: Option<TasksRangePreference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worklog_generate_range: Option<TasksRangePreference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worklog_history_range: Option<TasksRangePreference>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
    #[serde(skip)]
    unparsed: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TasksRangePreference {
    Preset { preset: String },
    Custom { from: String, to: String },
}

impl TasksRangePreference {
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            TasksRangePreference::Preset { preset } => {
                let valid = !preset.is_empty()
                    && preset.len() <= MAX_PRESET_LEN
                    && preset
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
                if valid {
                    Ok(())
                } else {
                    Err(DomainError::InvalidInput("preset de período inválido".to_string()))
                }
            }
            TasksRangePreference::Custom { from, to } => DateRange::parse(from, to).map(|_| ()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            theme: Theme::Light,
            min_sync_interval_minutes: DEFAULT_MIN_SYNC_INTERVAL_MINUTES,
            tasks_range: None,
            worklog_generate_range: None,
            worklog_history_range: None,
            extra: serde_json::Map::new(),
            unparsed: serde_json::Map::new(),
        }
    }
}

impl Preferences {
    fn parse_lenient(contents: &str) -> Self {
        let map = match serde_json::from_str::<serde_json::Value>(contents) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => {
                log::warn!("[PREFERENCES] preferences.json ilegível, usando os padrões");
                return Self::default();
            }
        };
        let mut prefs = Self::default();
        for (key, value) in map {
            let parsed = match key.as_str() {
                "version" => assign(&mut prefs.version, &value),
                "theme" => assign(&mut prefs.theme, &value),
                "minSyncIntervalMinutes" => assign(&mut prefs.min_sync_interval_minutes, &value),
                "tasksRange" => assign(&mut prefs.tasks_range, &value),
                "worklogGenerateRange" => assign(&mut prefs.worklog_generate_range, &value),
                "worklogHistoryRange" => assign(&mut prefs.worklog_history_range, &value),
                _ => {
                    prefs.extra.insert(key, value);
                    continue;
                }
            };
            if !parsed {
                log::warn!("[PREFERENCES] valor inválido em {key}, usando o padrão e preservando o original");
                prefs.unparsed.insert(key, value);
            }
        }
        prefs
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        if let serde_json::Value::Object(map) = &mut value {
            for (key, raw) in &self.unparsed {
                map.insert(key.clone(), raw.clone());
            }
        }
        Ok(value)
    }
}

fn assign<T: DeserializeOwned>(slot: &mut T, value: &serde_json::Value) -> bool {
    match serde_json::from_value(value.clone()) {
        Ok(parsed) => {
            *slot = parsed;
            true
        }
        Err(_) => false,
    }
}

pub struct PreferencesManager {
    file_path: PathBuf,
    write_lock: Mutex<()>,
}

impl PreferencesManager {
    pub fn new(app_config_dir: PathBuf) -> Self {
        Self {
            file_path: app_config_dir.join("preferences.json"),
            write_lock: Mutex::new(()),
        }
    }

    fn update(&self, change: impl FnOnce(&mut Preferences)) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut prefs = self.load()?;
        change(&mut prefs);
        self.save(&prefs)
    }

    pub fn load(&self) -> Result<Preferences, DomainError> {
        if !self.file_path.exists() {
            return Ok(Preferences::default());
        }

        let contents = fs::read_to_string(&self.file_path)
            .map_err(|e| DomainError::Storage(format!("Failed to read preferences: {}", e)))?;

        Ok(Preferences::parse_lenient(&contents))
    }

    pub fn save(&self, preferences: &Preferences) -> Result<(), DomainError> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| DomainError::Storage(format!("Failed to create preferences directory: {}", e)))?;
        }
        let temp_path = self.file_path.with_extension("json.tmp");

        let contents = preferences
            .to_json()
            .and_then(|value| serde_json::to_string_pretty(&value))
            .map_err(|e| DomainError::Storage(format!("Failed to serialize preferences: {}", e)))?;

        fs::write(&temp_path, contents)
            .map_err(|e| DomainError::Storage(format!("Failed to write preferences temp file: {}", e)))?;

        fs::rename(&temp_path, &self.file_path)
            .map_err(|e| DomainError::Storage(format!("Failed to save preferences: {}", e)))?;

        Ok(())
    }

    pub fn update_theme(&self, theme: Theme) -> Result<(), DomainError> {
        self.update(|prefs| {
            prefs.unparsed.remove("theme");
            prefs.theme = theme;
        })
    }

    pub fn update_min_sync_interval(&self, minutes: u32) -> Result<(), DomainError> {
        if minutes > MAX_MIN_SYNC_INTERVAL_MINUTES && minutes != MANUAL_SYNC_ONLY {
            return Err(DomainError::ParseError(format!(
                "intervalo mínimo de sincronização inválido: {minutes}"
            )));
        }
        self.update(|prefs| {
            prefs.unparsed.remove("minSyncIntervalMinutes");
            prefs.min_sync_interval_minutes = minutes;
        })
    }

    pub fn update_range(&self, screen: &str, range: TasksRangePreference) -> Result<(), DomainError> {
        range.validate()?;
        let key = match screen {
            "tasks" => "tasksRange",
            "worklogGenerate" => "worklogGenerateRange",
            "worklogHistory" => "worklogHistoryRange",
            other => return Err(DomainError::ParseError(format!("tela desconhecida: {other}"))),
        };
        self.update(|prefs| {
            prefs.unparsed.remove(key);
            let slot = match key {
                "tasksRange" => &mut prefs.tasks_range,
                "worklogGenerateRange" => &mut prefs.worklog_generate_range,
                _ => &mut prefs.worklog_history_range,
            };
            *slot = Some(range);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_preset_and_custom_ranges() {
        let preset: Preferences =
            serde_json::from_str(r#"{"version":1,"theme":"light","tasksRange":{"preset":"30d"}}"#).unwrap();
        assert_eq!(
            preset.tasks_range,
            Some(TasksRangePreference::Preset { preset: "30d".into() })
        );

        let custom: Preferences = serde_json::from_str(
            r#"{"version":1,"theme":"dark","tasksRange":{"from":"2026-08-20","to":"2026-10-03"}}"#,
        )
        .unwrap();
        assert_eq!(
            custom.tasks_range,
            Some(TasksRangePreference::Custom {
                from: "2026-08-20".into(),
                to: "2026-10-03".into()
            })
        );
    }

    #[test]
    fn range_preference_validation() {
        assert!(TasksRangePreference::Preset {
            preset: "prev-month".into()
        }
        .validate()
        .is_ok());
        assert!(TasksRangePreference::Preset {
            preset: "<script>".into()
        }
        .validate()
        .is_err());
        assert!(TasksRangePreference::Custom {
            from: "2026-10-01".into(),
            to: "2026-10-05".into()
        }
        .validate()
        .is_ok());
        assert!(TasksRangePreference::Custom {
            from: "2026-10-05".into(),
            to: "x".into()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn accepts_manual_sync_and_rejects_out_of_range_interval() {
        let dir = std::env::temp_dir().join(format!("gama-prefs-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let manager = PreferencesManager::new(dir.clone());
        manager.update_min_sync_interval(MANUAL_SYNC_ONLY).unwrap();
        assert_eq!(manager.load().unwrap().min_sync_interval_minutes, MANUAL_SYNC_ONLY);
        assert!(manager
            .update_min_sync_interval(MAX_MIN_SYNC_INTERVAL_MINUTES + 1)
            .is_err());
        manager.update_min_sync_interval(MAX_MIN_SYNC_INTERVAL_MINUTES).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_file_falls_back_to_defaults_and_can_be_overwritten() {
        let dir = std::env::temp_dir().join(format!("gama-prefs-invalid-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("preferences.json"), r#"{"version":1,"theme":"neon"}"#).unwrap();
        let manager = PreferencesManager::new(dir.clone());

        assert_eq!(
            manager.load().unwrap().min_sync_interval_minutes,
            DEFAULT_MIN_SYNC_INTERVAL_MINUTES
        );
        manager.update_min_sync_interval(5).unwrap();
        assert_eq!(manager.load().unwrap().min_sync_interval_minutes, 5);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn one_invalid_value_keeps_the_other_preferences_and_the_original_value() {
        let dir = std::env::temp_dir().join(format!("gama-prefs-partial-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("preferences.json"),
            r#"{"version":1,"theme":"neon","tasksRange":{"preset":"7d"},"minSyncIntervalMinutes":30,"futureKey":1}"#,
        )
        .unwrap();
        let manager = PreferencesManager::new(dir.clone());

        let loaded = manager.load().unwrap();
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(
            loaded.tasks_range,
            Some(TasksRangePreference::Preset { preset: "7d".into() })
        );
        assert_eq!(loaded.min_sync_interval_minutes, 30);

        manager.update_min_sync_interval(15).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("preferences.json")).unwrap()).unwrap();
        assert_eq!(saved["theme"], "neon");
        assert_eq!(saved["tasksRange"]["preset"], "7d");
        assert_eq!(saved["futureKey"], 1);
        assert_eq!(saved["minSyncIntervalMinutes"], 15);

        manager.update_theme(Theme::Dark).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("preferences.json")).unwrap()).unwrap();
        assert_eq!(saved["theme"], "dark");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn concurrent_updates_do_not_lose_each_other() {
        let dir = std::env::temp_dir().join(format!("gama-prefs-concurrent-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let manager = std::sync::Arc::new(PreferencesManager::new(dir.clone()));

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let manager = std::sync::Arc::clone(&manager);
                std::thread::spawn(move || {
                    if i % 2 == 0 {
                        manager.update_theme(Theme::Dark)
                    } else {
                        manager.update_range("tasks", TasksRangePreference::Preset { preset: "7d".into() })
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        let loaded = manager.load().unwrap();
        assert_eq!(loaded.theme, Theme::Dark);
        assert_eq!(
            loaded.tasks_range,
            Some(TasksRangePreference::Preset { preset: "7d".into() })
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_creates_the_missing_config_directory() {
        let dir = std::env::temp_dir()
            .join(format!("gama-prefs-missing-{}", std::process::id()))
            .join("config");
        let manager = PreferencesManager::new(dir.clone());

        manager.update_min_sync_interval(15).unwrap();

        assert_eq!(manager.load().unwrap().min_sync_interval_minutes, 15);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn keeps_unknown_keys_and_missing_range() {
        let prefs: Preferences = serde_json::from_str(r#"{"version":1,"theme":"light","futureKey":42}"#).unwrap();
        assert!(prefs.tasks_range.is_none());
        assert_eq!(prefs.min_sync_interval_minutes, DEFAULT_MIN_SYNC_INTERVAL_MINUTES);
        let json = serde_json::to_value(&prefs).unwrap();
        assert_eq!(json["futureKey"], 42);
        assert!(json.get("tasksRange").is_none());
    }
}
