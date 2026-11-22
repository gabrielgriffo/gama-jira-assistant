use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::domain::date_range::DateRange;
use crate::domain::ports::EventPublisher;
use crate::infrastructure::preferences::MANUAL_SYNC_ONLY;

pub struct SyncThrottle {
    min_interval_secs: AtomicU64,
    last_synced: Mutex<HashMap<String, Instant>>,
    active: Mutex<HashMap<String, usize>>,
    generation: AtomicU64,
    events: Arc<dyn EventPublisher>,
}

pub struct SyncGuard {
    throttle: Arc<SyncThrottle>,
    key: String,
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        self.throttle.finish(&self.key);
    }
}

impl SyncThrottle {
    pub fn new(min_interval_minutes: u32, events: Arc<dyn EventPublisher>) -> Self {
        Self {
            min_interval_secs: AtomicU64::new(u64::from(min_interval_minutes) * 60),
            last_synced: Mutex::new(HashMap::new()),
            active: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            events,
        }
    }

    pub fn set_min_interval_minutes(&self, minutes: u32) {
        self.min_interval_secs.store(u64::from(minutes) * 60, Ordering::SeqCst);
    }

    pub fn should_sync(&self, key: &str, force: bool) -> bool {
        if self.active.lock().unwrap_or_else(|e| e.into_inner()).contains_key(key) {
            log::info!("[SYNC] {} já está sincronizando, aguardando a sync em andamento", key);
            return false;
        }
        self.is_due(key, force)
    }

    pub fn try_begin(self: &Arc<Self>, key: String, force: bool) -> Option<SyncGuard> {
        {
            let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
            if active.contains_key(&key) {
                log::info!("[SYNC] {} já está sincronizando, aguardando a sync em andamento", key);
                return None;
            }
            if !self.is_due(&key, force) {
                return None;
            }
            active.insert(key.clone(), 1);
        }
        self.events.sync_started(&key);
        Some(SyncGuard {
            throttle: Arc::clone(self),
            key,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn ensure_generation(&self, generation: u64) -> Result<(), crate::domain::error::DomainError> {
        if self.generation() == generation {
            Ok(())
        } else {
            log::info!("[SYNC] conta ou dados locais mudaram durante a sincronização; resultado descartado");
            Err(crate::domain::error::DomainError::SyncCancelled)
        }
    }

    fn is_due(&self, key: &str, force: bool) -> bool {
        if force {
            return true;
        }
        if self.is_manual_only() {
            log::info!(
                "[SYNC] {} sincronização automática desativada, usando apenas o cache",
                key
            );
            return false;
        }
        if !self.is_fresh(key) {
            return true;
        }
        log::info!(
            "[SYNC] {} sincronizado há menos de {} min, usando apenas o cache",
            key,
            self.min_interval_secs.load(Ordering::SeqCst) / 60
        );
        false
    }

    pub fn mark_synced(&self, key: &str, generation: u64) {
        let mut last_synced = self.last_synced.lock().unwrap_or_else(|e| e.into_inner());
        if self.generation() == generation {
            last_synced.insert(key.to_string(), Instant::now());
        }
    }

    pub fn clear(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.last_synced.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn begin(self: &Arc<Self>, key: String) -> SyncGuard {
        let first = {
            let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
            let count = active.entry(key.clone()).or_insert(0);
            *count += 1;
            *count == 1
        };
        if first {
            self.events.sync_started(&key);
        }
        SyncGuard {
            throttle: Arc::clone(self),
            key,
        }
    }

    pub fn is_active(&self, key: &str) -> bool {
        self.active.lock().unwrap_or_else(|e| e.into_inner()).contains_key(key)
    }

    pub async fn wait_until_idle(&self, key: &str) {
        while self.is_active(key) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub fn active_keys(&self) -> Vec<String> {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    fn finish(&self, key: &str) {
        let last = {
            let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
            match active.get_mut(key) {
                Some(count) if *count > 1 => {
                    *count -= 1;
                    false
                }
                Some(_) => {
                    active.remove(key);
                    true
                }
                None => false,
            }
        };
        if last {
            self.events.sync_finished(key);
        }
    }

    fn is_manual_only(&self) -> bool {
        self.min_interval_secs.load(Ordering::SeqCst) == u64::from(MANUAL_SYNC_ONLY) * 60
    }

    fn is_fresh(&self, key: &str) -> bool {
        let interval = Duration::from_secs(self.min_interval_secs.load(Ordering::SeqCst));
        if interval.is_zero() {
            return false;
        }
        self.last_synced
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .is_some_and(|at| at.elapsed() < interval)
    }
}

pub fn issues_key(range: &DateRange) -> String {
    format!("issues:{}..{}", range.from, range.to)
}

pub fn changelog_key(issue_key: &str) -> String {
    format!("changelog:{}", issue_key)
}

pub fn worklogs_key(range: &DateRange) -> String {
    format!("worklogs:{}..{}", range.from, range.to)
}

pub fn period_key(range: &DateRange) -> String {
    format!("period:{}..{}", range.from, range.to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingPublisher {
        events: Mutex<Vec<String>>,
    }

    impl EventPublisher for RecordingPublisher {
        fn tasks_updated(&self) {}
        fn tasks_synced(&self) {}
        fn tasks_sync_failed(&self, _reason: &str) {}
        fn status_history_synced(&self, _issue_key: &str) {}
        fn status_history_sync_failed(&self, _issue_key: &str, _reason: &str) {}
        fn sync_started(&self, key: &str) {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("started {key}"));
        }
        fn sync_finished(&self, key: &str) {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("finished {key}"));
        }
    }

    fn throttle(minutes: u32) -> (Arc<SyncThrottle>, Arc<RecordingPublisher>) {
        let publisher = Arc::new(RecordingPublisher::default());
        (Arc::new(SyncThrottle::new(minutes, publisher.clone())), publisher)
    }

    #[test]
    fn try_begin_reserves_the_key_until_the_guard_is_dropped() {
        let (throttle, publisher) = throttle(5);
        let guard = throttle.try_begin("period:a".to_string(), true);
        assert!(guard.is_some());
        assert!(throttle.try_begin("period:a".to_string(), true).is_none());
        drop(guard);
        assert!(throttle.try_begin("period:a".to_string(), true).is_some());
        assert_eq!(
            publisher.events.lock().unwrap().as_slice(),
            [
                "started period:a",
                "finished period:a",
                "started period:a",
                "finished period:a"
            ]
        );
    }

    #[tokio::test]
    async fn waiting_for_an_active_key_returns_once_its_sync_finishes() {
        let (throttle, _) = throttle(5);
        let guard = throttle.try_begin("worklogs:a".to_string(), true).unwrap();
        let waiter = {
            let throttle = Arc::clone(&throttle);
            tokio::spawn(async move { throttle.wait_until_idle("worklogs:a").await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiter.is_finished());

        drop(guard);

        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(!throttle.is_active("worklogs:a"));
    }

    #[test]
    fn try_begin_respects_the_minimum_interval() {
        let (throttle, _) = throttle(5);
        throttle.mark_synced("period:a", throttle.generation());
        assert!(throttle.try_begin("period:a".to_string(), false).is_none());
        assert!(throttle.try_begin("period:a".to_string(), true).is_some());
    }

    #[test]
    fn clearing_invalidates_syncs_started_before() {
        let (throttle, _) = throttle(5);
        let generation = throttle.generation();
        assert!(throttle.ensure_generation(generation).is_ok());
        throttle.clear();
        assert!(throttle.ensure_generation(generation).is_err());
        assert!(throttle.ensure_generation(throttle.generation()).is_ok());
    }

    #[test]
    fn a_sync_from_before_clear_does_not_mark_the_key_as_fresh() {
        let (throttle, _) = throttle(5);
        let generation = throttle.generation();
        throttle.clear();
        throttle.mark_synced("issues:a", generation);
        assert!(throttle.should_sync("issues:a", false));
    }

    #[test]
    fn syncs_when_never_synced_or_forced() {
        let (throttle, _) = throttle(5);
        assert!(throttle.should_sync("issues:a", false));
        throttle.mark_synced("issues:a", throttle.generation());
        assert!(!throttle.should_sync("issues:a", false));
        assert!(throttle.should_sync("issues:a", true));
        assert!(throttle.should_sync("issues:b", false));
    }

    #[test]
    fn zero_interval_always_syncs() {
        let (throttle, _) = throttle(0);
        throttle.mark_synced("issues:a", throttle.generation());
        assert!(throttle.should_sync("issues:a", false));
    }

    #[test]
    fn manual_only_syncs_just_when_forced() {
        let (throttle, _) = throttle(MANUAL_SYNC_ONLY);
        assert!(!throttle.should_sync("issues:a", false));
        assert!(throttle.should_sync("issues:a", true));
        throttle.mark_synced("issues:a", throttle.generation());
        assert!(!throttle.should_sync("issues:a", false));
        throttle.set_min_interval_minutes(5);
        assert!(!throttle.should_sync("issues:a", false));
        assert!(throttle.should_sync("issues:b", false));
        throttle.set_min_interval_minutes(MANUAL_SYNC_ONLY);
        assert!(!throttle.should_sync("issues:b", false));
    }

    #[test]
    fn clear_and_interval_change_take_effect() {
        let (throttle, _) = throttle(5);
        throttle.mark_synced("issues:a", throttle.generation());
        throttle.set_min_interval_minutes(0);
        assert!(throttle.should_sync("issues:a", false));
        throttle.set_min_interval_minutes(5);
        assert!(!throttle.should_sync("issues:a", false));
        throttle.clear();
        assert!(throttle.should_sync("issues:a", false));
    }

    #[test]
    fn overlapping_guards_emit_once_per_key() {
        let (throttle, publisher) = throttle(5);
        let first = throttle.begin("issues:a".into());
        let second = throttle.begin("issues:a".into());
        assert_eq!(throttle.active_keys(), vec!["issues:a".to_string()]);
        drop(first);
        assert_eq!(throttle.active_keys().len(), 1);
        drop(second);
        assert!(throttle.active_keys().is_empty());
        let guard = throttle.begin("issues:b".into());
        assert!(!throttle.should_sync("issues:b", true));
        drop(guard);
        assert!(throttle.should_sync("issues:b", true));
        assert_eq!(
            *publisher.events.lock().unwrap_or_else(|e| e.into_inner()),
            vec![
                "started issues:a".to_string(),
                "finished issues:a".to_string(),
                "started issues:b".to_string(),
                "finished issues:b".to_string()
            ]
        );
    }
}
