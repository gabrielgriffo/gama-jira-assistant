export type WorklogDraftState = "pending" | "sending" | "sent" | "failed";

export interface WorklogDraft {
  id: number;
  date: string;
  issueId: string;
  issueKey: string;
  issueSummary: string;
  start: number;
  startText?: string;
  duration: number;
  durationRaw: string;
  comment: string;
  state: WorklogDraftState;
  error: string | null;
}

export interface WorklogSubmission {
  date: string;
  issueKey: string;
  issueSummary: string;
  status: string;
  hours: number;
  startTimestamp: string;
  endTimestamp: string;
  comment?: string;
}
