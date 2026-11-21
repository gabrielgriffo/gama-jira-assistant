import { weekday } from "../../../../shared/date/local-date";

export const PENDING_TOLERANCE_MINUTES = 10;

export interface GridColumn {
  date: string;
  className: string;
  weekend: boolean;
  registered: number;
  ponto: number;
  diff: number;
  missing: boolean;
}

export function isWeekendDate(date: string): boolean {
  const wd = weekday(date);
  return wd === 0 || wd === 6;
}

export function gridColumns(
  days: string[],
  rows: { cells: number[] }[],
  pontoByDay: Map<string, number>,
  today: string,
): GridColumn[] {
  return days.map((date, index) => {
    const weekend = isWeekendDate(date);
    const registered = rows.reduce((sum, row) => sum + row.cells[index], 0);
    const ponto = pontoByDay.get(date) ?? 0;
    const diff = registered - ponto;
    return {
      date,
      className: weekend ? "wkend" : date === today ? "today-col" : "",
      weekend,
      registered,
      ponto,
      diff,
      missing: date < today && ponto > 0 && diff < -PENDING_TOLERANCE_MINUTES,
    };
  });
}

export function differenceBeforeToday(
  days: { date: string; registeredMinutes: number; pontoMinutes: number }[],
  today: string,
): number {
  return days.filter((day) => day.date < today).reduce((sum, day) => sum + day.registeredMinutes - day.pontoMinutes, 0);
}
