use std::sync::Arc;

use crate::application::error::AppResult;
use crate::application::SyncThrottle;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::preferences::{
    OnboardingProgress, Preferences, PreferencesManager, TasksRangePreference, Theme,
};
use tauri::State;

#[tauri::command]
pub async fn get_preferences(preferences_manager: State<'_, PreferencesManager>) -> AppResult<Preferences> {
    Ok(preferences_manager.load()?)
}

#[tauri::command]
pub async fn update_theme(theme: Theme, preferences_manager: State<'_, PreferencesManager>) -> AppResult<()> {
    Ok(preferences_manager.update_theme(theme)?)
}

#[tauri::command]
pub async fn update_range_preference(
    screen: String,
    range: TasksRangePreference,
    preferences_manager: State<'_, PreferencesManager>,
) -> AppResult<()> {
    Ok(preferences_manager.update_range(&screen, range)?)
}

#[tauri::command]
pub async fn save_onboarding(
    progress: OnboardingProgress,
    preferences_manager: State<'_, PreferencesManager>,
) -> AppResult<()> {
    Ok(preferences_manager.save_onboarding(progress)?)
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
) -> AppResult<()> {
    preferences_manager.update_min_sync_interval(minutes)?;
    throttle.set_min_interval_minutes(minutes);
    Ok(())
}

#[tauri::command]
pub async fn clear_local_data(
    credential_service: State<'_, CredentialService>,
    throttle: State<'_, Arc<SyncThrottle>>,
) -> AppResult<()> {
    throttle.clear();
    credential_service.clear_local_data().await?;
    throttle.clear();
    Ok(())
}
