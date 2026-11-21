import { daysInMonth, firstDayOf, LocalDate, monthTitle, shiftMonth, weekday, YearMonth } from "../../date/local-date";

export interface DraftRange {
  start: LocalDate | null;
  end: LocalDate | null;
}

export function pickDay(draft: DraftRange, day: LocalDate): DraftRange {
  if (!draft.start || draft.end) return { start: day, end: null };
  return day < draft.start ? { start: day, end: draft.start } : { start: draft.start, end: day };
}

export function visibleBounds(draft: DraftRange, hover: LocalDate | null): { lo: LocalDate; hi: LocalDate } | null {
  if (!draft.start) return null;
  if (draft.end) return { lo: draft.start, hi: draft.end };
  if (hover) return hover < draft.start ? { lo: hover, hi: draft.start } : { lo: draft.start, hi: hover };
  return { lo: draft.start, hi: draft.start };
}

export interface DayCell {
  date: LocalDate;
  day: number;
  isToday: boolean;
  disabled: boolean;
  isFirstOfMonth: boolean;
  isLastOfMonth: boolean;
}

export interface MonthView {
  key: YearMonth;
  title: string;
  leading: number[];
  days: DayCell[];
}

export function buildMonth(key: YearMonth, today: LocalDate): MonthView {
  const total = daysInMonth(key);
  const days: DayCell[] = Array.from({ length: total }, (_, i) => {
    const date = `${key}-${String(i + 1).padStart(2, "0")}`;
    return {
      date,
      day: i + 1,
      isToday: date === today,
      disabled: date > today,
      isFirstOfMonth: i === 0,
      isLastOfMonth: i === total - 1,
    };
  });
  const leading = Array.from({ length: weekday(firstDayOf(key)) }, (_, i) => i);
  return { key, title: monthTitle(key), leading, days };
}

export function buildMonths(from: YearMonth, to: YearMonth, today: LocalDate): MonthView[] {
  const out: MonthView[] = [];
  for (let m = from; m <= to; m = shiftMonth(m, 1)) out.push(buildMonth(m, today));
  return out;
}
