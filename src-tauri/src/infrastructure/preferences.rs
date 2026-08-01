use crate::domain::date_range::DateRange;
use crate::domain::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_MIN_SYNC_INTERVAL_MINUTES: u32 = 5;
pub const MAX_MIN_SYNC_INTERVAL_MINUTES: u32 = 24 * 60;
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
        }
    }
}

pub struct PreferencesManager {
    file_path: PathBuf,
}

impl PreferencesManager {
    pub fn new(app_config_dir: PathBuf) -> Self {
        Self {
            file_path: app_config_dir.join("preferences.json"),
        }
    }

    pub fn load(&self) -> Result<Preferences, DomainError> {
        if !self.file_path.exists() {
            return Ok(Preferences::default());
        }

        let contents = fs::read_to_string(&self.file_path)
            .map_err(|e| DomainError::Storage(format!("Failed to read preferences: {}", e)))?;

        serde_json::from_str(&contents)
            .map_err(|e| DomainError::ParseError(format!("Failed to parse preferences: {}", e)))
    }

    pub fn save(&self, preferences: &Preferences) -> Result<(), DomainError> {
        let temp_path = self.file_path.with_extension("json.tmp");

        let contents = serde_json::to_string_pretty(preferences)
            .map_err(|e| DomainError::Storage(format!("Failed to serialize preferences: {}", e)))?;

        fs::write(&temp_path, contents)
            .map_err(|e| DomainError::Storage(format!("Failed to write preferences temp file: {}", e)))?;

        fs::rename(&temp_path, &self.file_path)
            .map_err(|e| DomainError::Storage(format!("Failed to save preferences: {}", e)))?;

        Ok(())
    }

    pub fn update_theme(&self, theme: Theme) -> Result<(), DomainError> {
        let mut prefs = self.load()?;
        prefs.theme = theme;
        self.save(&prefs)
    }

    pub fn update_min_sync_interval(&self, minutes: u32) -> Result<(), DomainError> {
        if minutes > MAX_MIN_SYNC_INTERVAL_MINUTES {
            return Err(DomainError::ParseError(format!(
                "intervalo mínimo de sincronização inválido: {minutes}"
            )));
        }
        let mut prefs = self.load()?;
        prefs.min_sync_interval_minutes = minutes;
        self.save(&prefs)
    }

    pub fn update_range(&self, screen: &str, range: TasksRangePreference) -> Result<(), DomainError> {
        range.validate()?;
        let mut prefs = self.load()?;
        let slot = match screen {
            "tasks" => &mut prefs.tasks_range,
            "worklogGenerate" => &mut prefs.worklog_generate_range,
            "worklogHistory" => &mut prefs.worklog_history_range,
            other => return Err(DomainError::ParseError(format!("tela desconhecida: {other}"))),
        };
        *slot = Some(range);
        self.save(&prefs)
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
    fn keeps_unknown_keys_and_missing_range() {
        let prefs: Preferences = serde_json::from_str(r#"{"version":1,"theme":"light","futureKey":42}"#).unwrap();
        assert!(prefs.tasks_range.is_none());
        assert_eq!(prefs.min_sync_interval_minutes, DEFAULT_MIN_SYNC_INTERVAL_MINUTES);
        let json = serde_json::to_value(&prefs).unwrap();
        assert_eq!(json["futureKey"], 42);
        assert!(json.get("tasksRange").is_none());
    }
}
