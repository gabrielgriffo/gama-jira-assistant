import { addDays, DateRange, firstDayOf, LocalDate, monthOf, weekday } from "./local-date";

export interface RangePreset {
  id: string;
  label: string;
  resolve: (today: LocalDate) => DateRange;
}

const startOfWeek = (t: LocalDate): LocalDate => addDays(t, -((weekday(t) + 6) % 7));

const PRESET_TODAY: RangePreset = { id: "today", label: "Hoje", resolve: (t) => ({ from: t, to: t }) };
const PRESET_THIS_WEEK: RangePreset = {
  id: "week",
  label: "Esta semana",
  resolve: (t) => ({ from: startOfWeek(t), to: t }),
};
const PRESET_PREV_WEEK: RangePreset = {
  id: "prev-week",
  label: "Semana passada",
  resolve: (t) => {
    const end = addDays(startOfWeek(t), -1);
    return { from: addDays(end, -6), to: end };
  },
};
const PRESET_7D: RangePreset = { id: "7d", label: "Últimos 7 dias", resolve: (t) => ({ from: addDays(t, -6), to: t }) };
const PRESET_30D: RangePreset = {
  id: "30d",
  label: "Últimos 30 dias",
  resolve: (t) => ({ from: addDays(t, -29), to: t }),
};
const PRESET_90D: RangePreset = {
  id: "90d",
  label: "Últimos 90 dias",
  resolve: (t) => ({ from: addDays(t, -89), to: t }),
};
const PRESET_THIS_MONTH: RangePreset = {
  id: "month",
  label: "Este mês",
  resolve: (t) => ({ from: firstDayOf(monthOf(t)), to: t }),
};
const PRESET_PREV_MONTH: RangePreset = {
  id: "prev-month",
  label: "Mês passado",
  resolve: (t) => {
    const end = addDays(firstDayOf(monthOf(t)), -1);
    return { from: firstDayOf(monthOf(end)), to: end };
  },
};

export const WORKLOG_GENERATE_PRESETS: RangePreset[] = [
  PRESET_TODAY,
  PRESET_THIS_WEEK,
  PRESET_PREV_WEEK,
  PRESET_7D,
  PRESET_THIS_MONTH,
  PRESET_PREV_MONTH,
];

export const WORKLOG_HISTORY_PRESETS: RangePreset[] = [
  PRESET_THIS_MONTH,
  PRESET_PREV_MONTH,
  PRESET_THIS_WEEK,
  PRESET_PREV_WEEK,
  PRESET_30D,
  PRESET_90D,
];

export const DEFAULT_RANGE_PRESETS: RangePreset[] = [
  { id: "today", label: "Hoje", resolve: (t) => ({ from: t, to: t }) },
  { id: "7d", label: "Últimos 7 dias", resolve: (t) => ({ from: addDays(t, -6), to: t }) },
  { id: "30d", label: "Últimos 30 dias", resolve: (t) => ({ from: addDays(t, -29), to: t }) },
  { id: "90d", label: "Últimos 90 dias", resolve: (t) => ({ from: addDays(t, -89), to: t }) },
  { id: "month", label: "Este mês", resolve: (t) => ({ from: firstDayOf(monthOf(t)), to: t }) },
  {
    id: "prev-month",
    label: "Mês passado",
    resolve: (t) => {
      const end = addDays(firstDayOf(monthOf(t)), -1);
      return { from: firstDayOf(monthOf(end)), to: end };
    },
  },
];

export function matchPreset(
  range: DateRange,
  today: LocalDate,
  presets: RangePreset[] = DEFAULT_RANGE_PRESETS,
): RangePreset | undefined {
  return presets.find((p) => {
    const r = p.resolve(today);
    return r.from === range.from && r.to === range.to;
  });
}

export function resolvePreset(id: string, today: LocalDate, presets: RangePreset[] = DEFAULT_RANGE_PRESETS): DateRange {
  return (presets.find((p) => p.id === id) ?? presets[0]).resolve(today);
}

export function toRangePreference(
  range: DateRange,
  today: LocalDate,
  presets: RangePreset[] = DEFAULT_RANGE_PRESETS,
): { preset: string } | DateRange {
  const preset = matchPreset(range, today, presets);
  return preset ? { preset: preset.id } : { from: range.from, to: range.to };
}

export function fromRangePreference(
  saved: { preset: string } | DateRange | undefined | null,
  today: LocalDate,
  fallbackId = "30d",
  presets: RangePreset[] = DEFAULT_RANGE_PRESETS,
): DateRange {
  if (saved && "preset" in saved) {
    const preset = presets.find((p) => p.id === saved.preset);
    if (preset) return preset.resolve(today);
  }
  if (
    saved &&
    "from" in saved &&
    isLocalDate(saved.from) &&
    isLocalDate(saved.to) &&
    saved.from <= saved.to &&
    saved.from <= today
  ) {
    return { from: saved.from, to: saved.to > today ? today : saved.to };
  }
  return resolvePreset(fallbackId, today, presets);
}

function isLocalDate(value: unknown): value is LocalDate {
  return typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);
}
