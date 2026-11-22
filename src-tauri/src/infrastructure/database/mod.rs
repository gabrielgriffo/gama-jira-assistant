mod changelog_repository;
mod issue_repository;
mod jira_connection_repository;
pub mod migrations;
mod pontomais_entry_repository;
mod status_repository;
mod worklog_repository;

pub use changelog_repository::{AssigneeTransition, ChangelogRepository, StatusTransition};
pub use issue_repository::{IssueRepository, WorkCandidate};
pub use jira_connection_repository::{CachePurge, JiraConnectionRepository};
pub use pontomais_entry_repository::PontoMaisEntryRepository;
pub use status_repository::StatusRepository;
pub use worklog_repository::{SqliteWorklogRepository, StoredWorklog, SyncedCleanup, SyncedWorklog};

use sqlx::SqlitePool;

#[derive(Clone)]
pub struct Repositories {
    pub connection: JiraConnectionRepository,
    pub issue: IssueRepository,
    pub status: StatusRepository,
    pub changelog: ChangelogRepository,
    pub pontomais_entry: PontoMaisEntryRepository,
    pub worklog: SqliteWorklogRepository,
}

impl Repositories {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            connection: JiraConnectionRepository::new(pool.clone()),
            issue: IssueRepository::new(pool.clone()),
            status: StatusRepository::new(pool.clone()),
            changelog: ChangelogRepository::new(pool.clone()),
            pontomais_entry: PontoMaisEntryRepository::new(pool.clone()),
            worklog: SqliteWorklogRepository::new(pool),
        }
    }
}

#[cfg(test)]
pub(crate) use jira_connection_repository::in_memory_pool;
