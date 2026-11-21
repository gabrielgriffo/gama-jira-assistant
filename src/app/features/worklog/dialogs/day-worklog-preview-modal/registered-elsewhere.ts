import type { WorklogDraft } from "../../data-access/worklog";

export function minutesRegisteredElsewhere(
  day: { registeredMinutes: number; unconfirmedMinutes: number },
  drafts: Pick<WorklogDraft, "state" | "duration">[],
): number {
  const sentHere = drafts.filter((d) => d.state === "sent").reduce((sum, d) => sum + d.duration, 0);
  return Math.max(0, day.registeredMinutes + day.unconfirmedMinutes - sentHere);
}
