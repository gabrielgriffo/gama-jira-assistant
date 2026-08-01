import { invoke } from "./tauri-client";
import { onEvent } from "./events";
import type { Issue, IssueChangelogResponse } from "../models";

export interface PeriodSyncSummary {
  pontoEntries: number | null;
  issues: number;
  worklogs: number;
}

export const connectionCommands = {
  getJiraTimeZone: () => invoke<string | null>("get_jira_time_zone"),
};

export const issuesCommands = {
  listMyTasks: (from: string, to: string, force = false) => invoke<Issue[]>("list_my_tasks", { from, to, force }),

  getCachedTasks: (from: string, to: string) => invoke<Issue[]>("get_cached_tasks", { from, to }),

  onTasksUpdated: (callback: () => void) => onEvent<void>("tasks-updated", callback),

  onTasksSynced: (callback: () => void) => onEvent<void>("tasks-synced", callback),

  onTasksSyncFailed: (callback: (reason: string) => void) => onEvent<string>("tasks-sync-failed", callback),
};

export const changelogCommands = {
  getStatusHistory: (issueKey: string) => invoke<IssueChangelogResponse>("get_status_history", { issueKey }),

  getCachedStatusHistory: (issueKey: string) =>
    invoke<IssueChangelogResponse>("get_cached_status_history", { issueKey }),

  onStatusHistorySynced: (callback: (issueKey: string) => void) => onEvent<string>("status-history-synced", callback),

  onStatusHistorySyncFailed: (callback: (payload: { issue_key: string; reason: string }) => void) =>
    onEvent<{ issue_key: string; reason: string }>("status-history-sync-failed", callback),
};

export type StatusCategoryKey = "new" | "indeterminate" | "done";

export interface JiraStatus {
  id: string;
  name: string;
  untranslatedName?: string | null;
  categoryKey: StatusCategoryKey | string;
  categoryName: string;
  categoryColor: string;
  iconUrl?: string | null;
  isDevelopment: boolean;
}

export interface StatusDevelopmentUpdate {
  statusId: string;
  isDevelopment: boolean;
}

export const statusCommands = {
  getCatalog: () => invoke<JiraStatus[]>("get_jira_status_catalog"),

  getCatalogSyncedAt: () => invoke<string | null>("get_jira_status_catalog_synced_at"),

  syncCatalog: () => invoke<void>("sync_jira_status_catalog"),

  updateIsDevelopment: (updates: StatusDevelopmentUpdate[]) =>
    invoke<void>("update_jira_statuses_is_development", { updates }),
};

export const syncCommands = {
  getActiveSyncs: () => invoke<string[]>("get_active_syncs"),

  onSyncStarted: (callback: (key: string) => void) => onEvent<string>("sync-started", callback),

  onSyncFinished: (callback: (key: string) => void) => onEvent<string>("sync-finished", callback),
};

export const worklogsCommands = {
  syncWorklogPeriod: (fromDate: string, toDate: string, force = false) =>
    invoke<PeriodSyncSummary | null>("sync_worklog_period", { fromDate, toDate, force }),
};

export type TasksRangePreference = { preset: string } | { from: string; to: string };

export type RangeScreen = "tasks" | "worklogGenerate" | "worklogHistory";

export interface Preferences {
  version: number;
  theme: "light" | "dark" | "system";
  minSyncIntervalMinutes: number;
  tasksRange?: TasksRangePreference;
  worklogGenerateRange?: TasksRangePreference;
  worklogHistoryRange?: TasksRangePreference;
}

export const preferencesCommands = {
  getPreferences: () => invoke<Preferences>("get_preferences"),

  updateRangePreference: (screen: RangeScreen, range: TasksRangePreference) =>
    invoke<void>("update_range_preference", { screen, range }),

  updateMinSyncInterval: (minutes: number) => invoke<void>("update_min_sync_interval", { minutes }),

  clearLocalData: () => invoke<void>("clear_local_data"),
};
