export type LocalDate = string;
export type YearMonth = string;

export interface DateRange {
  from: LocalDate;
  to: LocalDate;
}

const DAY_MS = 86_400_000;
const MONTHS = [
  "janeiro",
  "fevereiro",
  "março",
  "abril",
  "maio",
  "junho",
  "julho",
  "agosto",
  "setembro",
  "outubro",
  "novembro",
  "dezembro",
];
const MONTHS_SHORT = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

const pad = (n: number) => String(n).padStart(2, "0");

function toUtcMs(d: LocalDate): number {
  const [y, m, day] = d.split("-").map(Number);
  return Date.UTC(y, m - 1, day);
}

function fromUtcMs(ms: number): LocalDate {
  const x = new Date(ms);
  return `${x.getUTCFullYear()}-${pad(x.getUTCMonth() + 1)}-${pad(x.getUTCDate())}`;
}

export const addDays = (d: LocalDate, n: number): LocalDate => fromUtcMs(toUtcMs(d) + n * DAY_MS);

export const daysBetween = (from: LocalDate, to: LocalDate): number =>
  Math.round((toUtcMs(to) - toUtcMs(from)) / DAY_MS) + 1;

export const weekday = (d: LocalDate): number => new Date(toUtcMs(d)).getUTCDay();

export const monthOf = (d: LocalDate): YearMonth => d.slice(0, 7);
export const firstDayOf = (m: YearMonth): LocalDate => `${m}-01`;

export function daysInMonth(m: YearMonth): number {
  const [y, mo] = m.split("-").map(Number);
  return new Date(Date.UTC(y, mo, 0)).getUTCDate();
}

export function shiftMonth(m: YearMonth, n: number): YearMonth {
  const [y, mo] = m.split("-").map(Number);
  const x = new Date(Date.UTC(y, mo - 1 + n, 1));
  return `${x.getUTCFullYear()}-${pad(x.getUTCMonth() + 1)}`;
}

export function localTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

export function todayIn(timeZone: string): LocalDate {
  return new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).format(
    new Date(),
  );
}

export function zonedDateTime(iso: string, timeZone: string): { date: LocalDate; time: string } | null {
  const instant = new Date(iso);
  if (Number.isNaN(instant.getTime())) return null;
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(instant);
  const get = (type: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === type)?.value ?? "";
  return { date: `${get("year")}-${get("month")}-${get("day")}`, time: `${get("hour")}:${get("minute")}` };
}

export function relativeDateTime(iso: string, timeZone: string): string {
  const zoned = zonedDateTime(iso, timeZone);
  if (!zoned) return iso;
  const today = todayIn(timeZone);
  if (zoned.date === today) return `hoje, ${zoned.time}`;
  if (zoned.date === addDays(today, -1)) return `ontem, ${zoned.time}`;
  const [, m, d] = zoned.date.split("-");
  return `${d}/${m}`;
}

export function monthTitle(m: YearMonth): string {
  const [y, mo] = m.split("-").map(Number);
  const name = MONTHS[mo - 1];
  return `${name.charAt(0).toUpperCase()}${name.slice(1)} de ${y}`;
}

export function shortDate(d: LocalDate): string {
  const [, m, day] = d.split("-").map(Number);
  return `${day} ${MONTHS_SHORT[m - 1]}`;
}

export function rangeLabel({ from, to }: DateRange): string {
  const y1 = from.slice(0, 4);
  const y2 = to.slice(0, 4);
  if (from === to) return `${shortDate(from)} ${y1}`;
  return y1 === y2
    ? `${shortDate(from)} – ${shortDate(to)} ${y2}`
    : `${shortDate(from)} ${y1} – ${shortDate(to)} ${y2}`;
}
