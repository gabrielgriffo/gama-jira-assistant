import type { WorklogDraft } from "../../data-access/worklog";

export const MINUTES_PER_DAY = 1440;
export const MAX_COMMENT_CHARS = 5000;

export type DraftField = "start" | "duration" | "comment";

export interface DraftError {
  field: DraftField;
  text: string;
}

type ValidatedDraft = Pick<WorklogDraft, "id" | "issueKey" | "start" | "duration" | "comment" | "state"> &
  Partial<Pick<WorklogDraft, "date">>;

export function validateDrafts(drafts: ValidatedDraft[], timeZone = ""): Record<number, DraftError> {
  const errors: Record<number, DraftError> = {};
  const live = drafts.filter((d) => d.state !== "sent");
  for (const d of live) {
    const error = ownError(d) ?? skippedByDaylightSaving(d, timeZone);
    if (error) errors[d.id] = error;
  }
  for (const a of live) {
    if (errors[a.id]) continue;
    for (const b of drafts) {
      if (a === b || !Number.isFinite(b.start) || !Number.isFinite(b.duration)) continue;
      if (a.start < b.start + b.duration && b.start < a.start + a.duration) {
        errors[a.id] = {
          field: "start",
          text: `Sobreposição com ${b.issueKey} (${clock(b.start)}–${clock(b.start + b.duration)}).`,
        };
        break;
      }
    }
  }
  return errors;
}

function ownError(d: ValidatedDraft): DraftError | null {
  if (!Number.isFinite(d.start)) return { field: "start", text: "Informe o horário de início." };
  if (!Number.isFinite(d.duration)) {
    return { field: "duration", text: "Informe a duração no formato h:mm, por exemplo, 2:30." };
  }
  if (d.duration < 1) return { field: "duration", text: "O Jira não aceita worklogs com menos de 1 minuto." };
  if (d.duration > MINUTES_PER_DAY) return { field: "duration", text: "A duração não pode passar de 24 horas." };
  if (d.start < 0 || d.start + d.duration > MINUTES_PER_DAY) {
    return { field: "start", text: "O worklog precisa começar e terminar no mesmo dia." };
  }
  if (d.comment.trim().length > MAX_COMMENT_CHARS) {
    return { field: "comment", text: `O comentário não pode passar de ${MAX_COMMENT_CHARS} caracteres.` };
  }
  return null;
}

function skippedByDaylightSaving(d: ValidatedDraft, timeZone: string): DraftError | null {
  if (!timeZone || !d.date || existsInZone(d.date, d.start, timeZone)) return null;
  return { field: "start", text: "Este horário não existe no fuso do Jira por causa do horário de verão." };
}

export function existsInZone(date: string, minutes: number, timeZone: string): boolean {
  try {
    const [y, mo, d] = date.split("-").map(Number);
    const wallClock = Date.UTC(y, mo - 1, d, Math.floor(minutes / 60), minutes % 60);
    const offset = offsetMinutesAt(wallClock - offsetMinutesAt(wallClock, timeZone) * 60000, timeZone);
    const parts = new Intl.DateTimeFormat("en-GB", {
      timeZone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      hourCycle: "h23",
    }).formatToParts(new Date(wallClock - offset * 60000));
    const get = (type: string) => parts.find((p) => p.type === type)?.value;
    const local = `${get("year")}-${get("month")}-${get("day")} ${get("hour")}:${get("minute")}`;
    return local === `${date} ${clock(minutes)}`;
  } catch {
    return true;
  }
}

export function clock(minutes: number): string {
  const m = Math.max(0, Math.min(MINUTES_PER_DAY, Math.round(minutes)));
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

export function jiraOffset(date: string, minutes: number, timeZone: string): string {
  try {
    const [y, mo, d] = date.split("-").map(Number);
    const wallClock = Date.UTC(y, mo - 1, d, Math.floor(minutes / 60), minutes % 60);
    const firstGuess = offsetMinutesAt(wallClock, timeZone);
    const offset = offsetMinutesAt(wallClock - firstGuess * 60000, timeZone);
    const sign = offset < 0 ? "-" : "+";
    const abs = Math.abs(offset);
    return `${sign}${String(Math.floor(abs / 60)).padStart(2, "0")}${String(abs % 60).padStart(2, "0")}`;
  } catch {
    return "+0000";
  }
}

function offsetMinutesAt(instant: number, timeZone: string): number {
  const part = new Intl.DateTimeFormat("en-US", { timeZone: timeZone || undefined, timeZoneName: "longOffset" })
    .formatToParts(new Date(instant))
    .find((p) => p.type === "timeZoneName")?.value;
  const match = part?.match(/GMT([+-])(\d{2}):(\d{2})/);
  if (!match) return 0;
  const value = Number(match[2]) * 60 + Number(match[3]);
  return match[1] === "-" ? -value : value;
}
