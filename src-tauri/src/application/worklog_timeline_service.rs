use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use super::error::AppResult;
use super::worked_time_calculator::{active_intervals, intersect, total_seconds, Interval, StateChange};
use crate::domain::date_range::{parse_time_zone, start_of_day_utc, DateRange};
use crate::domain::pontomais::workday::TimeCard;
use crate::domain::ports::CredentialStore;
use crate::infrastructure::credentials::CredentialService;
use crate::infrastructure::database::{
    AssigneeTransition, ChangelogRepository, IssueRepository, PontoMaisEntryRepository, SqliteWorklogRepository,
    StatusTransition, WorkCandidate,
};

#[derive(Debug, Serialize)]
pub struct MinuteRange {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Serialize)]
pub struct TimelineEntry {
    pub time: String,
    pub entry_type: String,
    pub disabled: bool,
}

#[derive(Debug, Serialize)]
pub struct TaskWorkedInfo {
    pub key: String,
    pub issue_id: String,
    pub summary: String,
    pub worked_minutes: i64,
    pub start_time: Option<String>,
    pub dev_segments: Vec<MinuteRange>,
    pub counted_segments: Vec<MinuteRange>,
}

#[derive(Debug, Serialize)]
pub struct WorkDayTimeline {
    pub date: String,
    pub is_today: bool,
    pub total_hours: f64,
    pub total_minutes: i64,
    pub entries: Vec<TimelineEntry>,
    pub punch_intervals: Vec<MinuteRange>,
    pub incomplete: bool,
    pub open: bool,
    pub tasks_worked: Vec<TaskWorkedInfo>,
    pub registered_minutes: i64,
    pub gama_synced_count: i64,
    pub has_synced_worklog: bool,
}

struct CandidateHistory {
    candidate: WorkCandidate,
    currently_mine: bool,
    status: Vec<StateChange>,
    assignee: Vec<StateChange>,
}

pub struct WorklogTimelineService {
    pontomais_repo: Arc<PontoMaisEntryRepository>,
    changelog_repo: Arc<ChangelogRepository>,
    issue_repo: Arc<IssueRepository>,
    worklog_repo: Arc<SqliteWorklogRepository>,
    credential_service: Arc<CredentialService>,
}

impl WorklogTimelineService {
    pub fn new(
        pontomais_repo: PontoMaisEntryRepository,
        changelog_repo: ChangelogRepository,
        issue_repo: IssueRepository,
        worklog_repo: SqliteWorklogRepository,
        credential_service: CredentialService,
    ) -> Self {
        Self {
            pontomais_repo: Arc::new(pontomais_repo),
            changelog_repo: Arc::new(changelog_repo),
            issue_repo: Arc::new(issue_repo),
            worklog_repo: Arc::new(worklog_repo),
            credential_service: Arc::new(credential_service),
        }
    }

    pub async fn build(&self, range: &DateRange) -> AppResult<Vec<WorkDayTimeline>> {
        let user = self.credential_service.get_current_user().await?;
        let tz = parse_time_zone(&user.time_zone);
        let now = Utc::now();
        let today = now.with_timezone(&tz).date_naive();
        let last_day = range.to.min(today);
        if range.from > last_day {
            return Ok(Vec::new());
        }

        let from = range.from.to_string();
        let to = last_day.to_string();

        let mut cards_by_day: HashMap<String, Vec<TimeCard>> = HashMap::new();
        for card in self.pontomais_repo.get_by_date_range(&from, &to).await? {
            let date = card.date.split('T').next().unwrap_or(&card.date).to_string();
            cards_by_day.entry(date).or_default().push(card);
        }

        let totals: HashMap<String, (i64, i64)> = self
            .worklog_repo
            .daily_totals(&user.account_id, &from, &to)
            .await?
            .into_iter()
            .map(|t| (t.local_date, (t.synced_seconds, t.gama_synced_count)))
            .collect();

        let histories = self.load_histories(&user.account_id).await?;

        let mut days = Vec::new();
        let mut day = last_day;
        loop {
            let date = day.to_string();
            let cards = cards_by_day.remove(&date).unwrap_or_default();
            let (registered_seconds, gama_count) = totals.get(&date).copied().unwrap_or((0, 0));
            let timeline = build_day(
                day,
                day == today,
                now,
                tz,
                cards,
                &histories,
                registered_seconds,
                gama_count,
            );

            if !matches!(day.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
                days.push(timeline);
            }

            if day <= range.from {
                break;
            }
            day = day.pred_opt().unwrap_or(range.from);
        }

        Ok(days)
    }

    async fn load_histories(&self, account_id: &str) -> AppResult<Vec<CandidateHistory>> {
        let candidates = self.issue_repo.get_work_candidates(account_id).await?;
        let mut histories = Vec::with_capacity(candidates.len());

        for candidate in candidates {
            let status = self.changelog_repo.get_status_transitions(&candidate.id).await?;
            let assignee = self.changelog_repo.get_assignee_transitions(&candidate.id).await?;
            histories.push(CandidateHistory {
                currently_mine: candidate.assignee_account_id.as_deref() == Some(account_id),
                status: status_changes(&status),
                assignee: assignee_changes(&assignee, account_id),
                candidate,
            });
        }

        Ok(histories)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_day(
    day: NaiveDate,
    is_today: bool,
    now: DateTime<Utc>,
    tz: Tz,
    mut cards: Vec<TimeCard>,
    histories: &[CandidateHistory],
    registered_seconds: i64,
    gama_count: i64,
) -> WorkDayTimeline {
    cards.sort_by(|a, b| a.time.cmp(&b.time));
    let day_start = start_of_day_utc(day, tz);
    let day_end = day
        .succ_opt()
        .map(|next| start_of_day_utc(next, tz))
        .unwrap_or(day_start);
    let window_end = if is_today { now.min(day_end) } else { day_end };

    let entries: Vec<TimelineEntry> = cards
        .iter()
        .enumerate()
        .map(|(index, card)| TimelineEntry {
            time: card.time.clone(),
            entry_type: match (index % 2, index) {
                (0, 0) => "Entrada".to_string(),
                (0, _) => "Retorno".to_string(),
                _ => "Saída".to_string(),
            },
            disabled: card.disabled,
        })
        .collect();

    let punches: Vec<DateTime<Utc>> = cards
        .iter()
        .filter(|c| !c.disabled)
        .filter_map(|c| local_instant(day, &c.time, tz))
        .collect();

    let mut punch_intervals = Vec::new();
    for pair in punches.chunks(2) {
        if let [start, end] = pair {
            if end > start {
                punch_intervals.push(Interval::new(*start, *end));
            }
        }
    }

    let odd = punches.len() % 2 == 1;
    let open = odd && is_today;
    let incomplete = odd && !is_today;
    if open {
        if let Some(last) = punches.last() {
            if now > *last {
                punch_intervals.push(Interval::new(*last, now));
            }
        }
    }

    let ponto_seconds = total_seconds(&punch_intervals);
    let window = Interval::new(day_start, window_end.max(day_start));

    let mut tasks_worked: Vec<TaskWorkedInfo> = histories
        .iter()
        .filter_map(|history| {
            let dev = active_intervals(window, &history.status, history.candidate.current_is_development);
            let mine = active_intervals(window, &history.assignee, history.currently_mine);
            let dev_mine = intersect(&dev, &mine);
            if dev_mine.is_empty() {
                return None;
            }
            let counted = intersect(&dev_mine, &punch_intervals);
            let start = counted
                .first()
                .or(dev_mine.first())
                .map(|i| i.start.with_timezone(&tz).format("%H:%M").to_string());

            Some(TaskWorkedInfo {
                key: history.candidate.key.clone(),
                issue_id: history.candidate.id.clone(),
                summary: history.candidate.summary.clone(),
                worked_minutes: (total_seconds(&counted) as f64 / 60.0).round() as i64,
                start_time: start,
                dev_segments: dev_mine.iter().map(|i| minute_range(i, day, tz)).collect(),
                counted_segments: counted.iter().map(|i| minute_range(i, day, tz)).collect(),
            })
        })
        .collect();

    tasks_worked.sort_by(|a, b| b.worked_minutes.cmp(&a.worked_minutes).then_with(|| a.key.cmp(&b.key)));

    let total_minutes = (ponto_seconds as f64 / 60.0).round() as i64;

    WorkDayTimeline {
        date: day.to_string(),
        is_today,
        total_hours: total_minutes as f64 / 60.0,
        total_minutes,
        entries,
        punch_intervals: punch_intervals.iter().map(|i| minute_range(i, day, tz)).collect(),
        incomplete,
        open,
        tasks_worked,
        registered_minutes: (registered_seconds as f64 / 60.0).round() as i64,
        gama_synced_count: gama_count,
        has_synced_worklog: gama_count > 0,
    }
}

fn status_changes(transitions: &[StatusTransition]) -> Vec<StateChange> {
    transitions
        .iter()
        .filter_map(|t| {
            Some(StateChange {
                at: parse_utc(&t.jira_created_at)?,
                from_active: t.from_is_development,
                to_active: t.to_is_development,
            })
        })
        .collect()
}

fn assignee_changes(transitions: &[AssigneeTransition], account_id: &str) -> Vec<StateChange> {
    transitions
        .iter()
        .filter_map(|t| {
            Some(StateChange {
                at: parse_utc(&t.jira_created_at)?,
                from_active: t.from_account_id.as_deref() == Some(account_id),
                to_active: t.to_account_id.as_deref() == Some(account_id),
            })
        })
        .collect()
}

fn parse_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn local_instant(day: NaiveDate, time: &str, tz: Tz) -> Option<DateTime<Utc>> {
    let time = NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    tz.from_local_datetime(&day.and_time(time))
        .earliest()
        .map(|t| t.with_timezone(&Utc))
}

fn minute_of_day(instant: DateTime<Utc>, day: NaiveDate, tz: Tz) -> i64 {
    let local = instant.with_timezone(&tz);
    let offset_days = (local.date_naive() - day).num_days();
    (offset_days * 1440 + local.hour() as i64 * 60 + local.minute() as i64).clamp(0, 1440)
}

fn minute_range(interval: &Interval, day: NaiveDate, tz: Tz) -> MinuteRange {
    MinuteRange {
        start: minute_of_day(interval.start, day, tz),
        end: minute_of_day(interval.end, day, tz),
    }
}
