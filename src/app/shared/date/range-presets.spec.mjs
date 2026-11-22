import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_RANGE_PRESETS,
  exceedsMaxRange,
  fromRangePreference,
  selectedPreset,
  toRangePreference,
  WORKLOG_GENERATE_PRESETS,
  WORKLOG_HISTORY_PRESETS,
} from "./range-presets.ts";

const monday = "2026-11-16";
const presets = WORKLOG_GENERATE_PRESETS;

test("as três telas oferecem os mesmos atalhos de período", () => {
  const ids = (list) => list.map((p) => p.id);
  const expected = ["today", "week", "prev-week", "7d", "30d", "90d", "month", "prev-month"];
  assert.deepEqual(ids(DEFAULT_RANGE_PRESETS), expected);
  assert.deepEqual(ids(WORKLOG_GENERATE_PRESETS), expected);
  assert.deepEqual(ids(WORKLOG_HISTORY_PRESETS), expected);
});

test("atalhos de semana passada, 30 dias, 90 dias e mês passado resolvem os períodos esperados", () => {
  const range = (id) => presets.find((p) => p.id === id).resolve("2026-11-18");
  assert.deepEqual(range("prev-week"), { from: "2026-11-09", to: "2026-11-15" });
  assert.deepEqual(range("30d"), { from: "2026-10-20", to: "2026-11-18" });
  assert.deepEqual(range("90d"), { from: "2026-08-21", to: "2026-11-18" });
  assert.deepEqual(range("prev-month"), { from: "2026-10-01", to: "2026-10-31" });
});

test("atalho escolhido prevalece sobre outro que resolve para as mesmas datas", () => {
  const week = { from: monday, to: monday, presetId: "week" };
  assert.equal(selectedPreset(week, monday, presets)?.id, "week");
  assert.deepEqual(toRangePreference(week, monday, presets), { preset: "week" });
});

test("período escolhido à mão continua reconhecendo o primeiro atalho equivalente", () => {
  const manual = { from: monday, to: monday };
  assert.equal(selectedPreset(manual, monday, presets)?.id, "today");
  assert.deepEqual(toRangePreference(manual, monday, presets), { preset: "today" });
});

test("atalho guardado que não corresponde mais às datas é ignorado", () => {
  const stale = { from: "2026-11-10", to: "2026-11-12", presetId: "week" };
  assert.equal(selectedPreset(stale, monday, presets), undefined);
  assert.deepEqual(toRangePreference(stale, monday, presets), { from: "2026-11-10", to: "2026-11-12" });
});

test("preferência salva com atalho volta com o atalho identificado", () => {
  assert.deepEqual(fromRangePreference({ preset: "week" }, monday, "today", presets), {
    from: monday,
    to: monday,
    presetId: "week",
  });
  assert.deepEqual(fromRangePreference({ preset: "week" }, "2026-11-18", "today", presets), {
    from: monday,
    to: "2026-11-18",
    presetId: "week",
  });
});

test("sem preferência salva, o atalho padrão volta identificado", () => {
  assert.deepEqual(fromRangePreference(null, monday, "week", presets), { from: monday, to: monday, presetId: "week" });
});

test("período salvo acima do limite do backend volta para o padrão", () => {
  const saved = { from: "2025-01-01", to: "2026-11-18" };
  const restored = fromRangePreference(saved, "2026-11-18", "30d");
  assert.equal(restored.presetId, "30d");
  assert.equal(exceedsMaxRange("2025-11-17", "2026-11-18"), true);
  assert.equal(exceedsMaxRange("2025-11-18", "2026-11-18"), false);
});
