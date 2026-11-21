/// Port: notifies the outside world (the Angular frontend, via Tauri events
/// in practice) that a background task sync finished. Kept as a port so
/// `application/` can be unit-tested with a fake instead of a running Tauri
/// app, same as the other collaborators of `JiraService`.
pub trait EventPublisher: Send + Sync {
    fn tasks_updated(&self);
    fn tasks_synced(&self);
    fn tasks_sync_failed(&self, reason: &str);
    fn status_history_synced(&self, issue_key: &str);
    fn status_history_sync_failed(&self, issue_key: &str, reason: &str);
    fn sync_started(&self, key: &str);
    fn sync_finished(&self, key: &str);
}
