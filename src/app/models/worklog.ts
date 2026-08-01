export type WorklogDraftState = "pending" | "sending" | "sent" | "failed";

export interface WorklogDraft {
  id: number;
  date: string;
  issueId: string;
  issueKey: string;
  issueSummary: string;
  start: number;
  duration: number;
  durationRaw: string;
  comment: string;
  state: WorklogDraftState;
  error: string | null;
}

export interface WorklogSubmission {
  date: string;
  issue_key: string;
  issue_summary: string;
  status: string;
  hours: number;
  start_timestamp: string;
  end_timestamp: string;
  comment?: string;
}
