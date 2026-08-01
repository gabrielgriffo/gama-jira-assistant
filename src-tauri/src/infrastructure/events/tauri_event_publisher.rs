use tauri::{AppHandle, Emitter};

use crate::domain::ports::EventPublisher;

/// Emits Tauri events so the Angular side can react to background task
/// syncs without polling. Thin adapter — the actual sync decision logic
/// lives in `application::JiraService`, tested there via a fake.
#[derive(Clone)]
pub struct TauriEventPublisher {
    app: AppHandle,
}

impl TauriEventPublisher {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventPublisher for TauriEventPublisher {
    fn tasks_updated(&self) {
        let _ = self.app.emit("tasks-updated", ());
    }

    fn tasks_synced(&self) {
        let _ = self.app.emit("tasks-synced", ());
    }

    fn tasks_sync_failed(&self, reason: &str) {
        let _ = self.app.emit("tasks-sync-failed", reason);
    }

    fn status_history_synced(&self, issue_key: &str) {
        let _ = self.app.emit("status-history-synced", issue_key);
    }

    fn status_history_sync_failed(&self, issue_key: &str, reason: &str) {
        #[derive(serde::Serialize, Clone)]
        struct Payload<'a> {
            issue_key: &'a str,
            reason: &'a str,
        }
        let _ = self
            .app
            .emit("status-history-sync-failed", Payload { issue_key, reason });
    }

    fn sync_started(&self, key: &str) {
        let _ = self.app.emit("sync-started", key);
    }

    fn sync_finished(&self, key: &str) {
        let _ = self.app.emit("sync-finished", key);
    }
}
