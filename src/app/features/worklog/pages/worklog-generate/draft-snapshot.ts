import type { WorklogDraft } from "../../data-access/worklog";

export function draftSnapshot(drafts: WorklogDraft[]): string {
  return JSON.stringify(drafts.map((d) => [d.issueKey, d.start, d.durationRaw, d.comment, d.state]));
}
