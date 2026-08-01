use std::sync::Arc;

use crate::application::error::AppResult;
use crate::application::SyncThrottle;
use crate::domain::error::DomainError;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::preferences::{Preferences, PreferencesManager, TasksRangePreference, Theme};
use tauri::State;

#[tauri::command]
pub async fn get_preferences(preferences_manager: State<'_, PreferencesManager>) -> Result<Preferences, String> {
    preferences_manager.load().map_err(|e: DomainError| e.to_string())
}

#[tauri::command]
pub async fn update_theme(theme: String, preferences_manager: State<'_, PreferencesManager>) -> Result<(), String> {
    let theme_enum = match theme.as_str() {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        "system" => Theme::System,
        _ => return Err("Invalid theme".to_string()),
    };

    preferences_manager
        .update_theme(theme_enum)
        .map_err(|e: DomainError| e.to_string())
}

#[tauri::command]
pub async fn update_range_preference(
    screen: String,
    range: TasksRangePreference,
    preferences_manager: State<'_, PreferencesManager>,
) -> Result<(), String> {
    preferences_manager
        .update_range(&screen, range)
        .map_err(|e: DomainError| e.to_string())
}

#[tauri::command]
pub fn get_active_syncs(throttle: State<'_, Arc<SyncThrottle>>) -> Vec<String> {
    throttle.active_keys()
}

#[tauri::command]
pub async fn update_min_sync_interval(
    minutes: u32,
    preferences_manager: State<'_, PreferencesManager>,
    throttle: State<'_, Arc<SyncThrottle>>,
) -> Result<(), String> {
    preferences_manager
        .update_min_sync_interval(minutes)
        .map_err(|e: DomainError| e.to_string())?;
    throttle.set_min_interval_minutes(minutes);
    Ok(())
}

#[tauri::command]
pub async fn clear_local_data(
    credential_service: State<'_, CredentialService>,
    throttle: State<'_, Arc<SyncThrottle>>,
) -> AppResult<()> {
    credential_service.clear_local_data().await?;
    throttle.clear();
    Ok(())
}
