export interface JiraWorklog {
  id: string;
  issueKey: string;
  issueSummary: string;
  issueBrowseUrl: string;
  origin: string;
  authorAccountId: string;
  authorDisplayName: string | null;
  started: string;
  timeSpentSeconds: number;
  comment: string | null;
  created: string;
  updated: string;
}

export interface JiraWorklogDay {
  date: string;
  worklogs: JiraWorklog[];
  totalSeconds: number;
}

export interface JiraWorklogHistoryResponse {
  days: JiraWorklogDay[];
  totalSeconds: number;
  totalWorklogs: number;
  uniqueIssues: number;
  lastSyncedAt: string | null;
}
