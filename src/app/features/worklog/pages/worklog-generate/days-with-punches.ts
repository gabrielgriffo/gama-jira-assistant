import type { WorkDayTimeline } from "../../../../core/pontomais/pontomais.service";

export function daysWithPunches<T extends Pick<WorkDayTimeline, "entries">>(days: readonly T[]): T[] {
  return days.filter((day) => day.entries.length > 0);
}
