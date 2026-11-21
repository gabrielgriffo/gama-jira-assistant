use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Interval {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    pub fn seconds(&self) -> i64 {
        (self.end - self.start).num_seconds().max(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateChange {
    pub at: DateTime<Utc>,
    pub from_active: bool,
    pub to_active: bool,
}

pub fn active_at_start(window_start: DateTime<Utc>, changes: &[StateChange], current: bool) -> bool {
    match changes.iter().rev().find(|c| c.at <= window_start) {
        Some(last_before) => last_before.to_active,
        None => changes.first().map(|first| first.from_active).unwrap_or(current),
    }
}

pub fn active_intervals(window: Interval, changes: &[StateChange], current: bool) -> Vec<Interval> {
    let mut sorted: Vec<StateChange> = changes.to_vec();
    sorted.sort_by_key(|c| c.at);

    let mut result = Vec::new();
    let mut active = active_at_start(window.start, &sorted, current);
    let mut cursor = window.start;

    for change in sorted.iter().filter(|c| c.at > window.start && c.at < window.end) {
        if active && change.at > cursor {
            result.push(Interval::new(cursor, change.at));
        }
        cursor = change.at;
        active = change.to_active;
    }

    if active && window.end > cursor {
        result.push(Interval::new(cursor, window.end));
    }

    merge(result)
}

pub fn intersect(a: &[Interval], b: &[Interval]) -> Vec<Interval> {
    let mut result = Vec::new();
    for x in a {
        for y in b {
            let start = x.start.max(y.start);
            let end = x.end.min(y.end);
            if end > start {
                result.push(Interval::new(start, end));
            }
        }
    }
    merge(result)
}

pub fn total_seconds(intervals: &[Interval]) -> i64 {
    intervals.iter().map(Interval::seconds).sum()
}

fn merge(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_by_key(|i| i.start);
    let mut merged: Vec<Interval> = Vec::with_capacity(intervals.len());
    for interval in intervals {
        match merged.last_mut() {
            Some(last) if interval.start <= last.end => last.end = last.end.max(interval.end),
            _ => merged.push(interval),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc)
    }

    fn iv(a: &str, b: &str) -> Interval {
        Interval::new(at(a), at(b))
    }

    fn change(value: &str, from: bool, to: bool) -> StateChange {
        StateChange {
            at: at(value),
            from_active: from,
            to_active: to,
        }
    }

    #[test]
    fn documented_example() {
        let window = iv("2026-09-10T03:00:00Z", "2026-09-11T03:00:00Z");
        let status = vec![
            change("2026-09-10T12:30:00Z", false, true),
            change("2026-09-10T18:00:00Z", true, false),
        ];
        let dev = active_intervals(window, &status, false);
        let mine = active_intervals(window, &[], true);
        let punches = vec![
            iv("2026-09-10T11:00:00Z", "2026-09-10T15:00:00Z"),
            iv("2026-09-10T16:00:00Z", "2026-09-10T20:00:00Z"),
        ];

        let counted = intersect(&intersect(&dev, &mine), &punches);

        assert_eq!(total_seconds(&counted), 16_200);
    }

    #[test]
    fn state_before_window_uses_last_transition() {
        let window = iv("2026-09-10T03:00:00Z", "2026-09-11T03:00:00Z");
        let status = vec![change("2026-09-08T12:00:00Z", false, true)];
        assert_eq!(active_intervals(window, &status, false), vec![window]);
    }

    #[test]
    fn state_without_previous_transition_uses_first_from() {
        let window = iv("2026-09-10T03:00:00Z", "2026-09-11T03:00:00Z");
        let status = vec![change("2026-09-12T12:00:00Z", true, false)];
        assert_eq!(active_intervals(window, &status, false), vec![window]);
    }

    #[test]
    fn without_transitions_uses_current_state() {
        let window = iv("2026-09-10T03:00:00Z", "2026-09-11T03:00:00Z");
        assert!(active_intervals(window, &[], false).is_empty());
        assert_eq!(active_intervals(window, &[], true), vec![window]);
    }

    #[test]
    fn reassignment_cuts_time() {
        let window = iv("2026-09-10T03:00:00Z", "2026-09-11T03:00:00Z");
        let assignee = vec![change("2026-09-10T14:00:00Z", true, false)];
        let mine = active_intervals(window, &assignee, false);
        let punches = vec![iv("2026-09-10T11:00:00Z", "2026-09-10T20:00:00Z")];
        assert_eq!(total_seconds(&intersect(&mine, &punches)), 3 * 3600);
    }
}
