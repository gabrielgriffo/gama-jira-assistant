import { test } from "node:test";
import assert from "node:assert/strict";
import { differenceBeforeToday, gridColumns } from "./grid-columns.ts";

const today = "2026-11-18";

test("soma cada coluna e marca os dias passados abaixo do ponto", () => {
  const days = ["2026-11-18", "2026-11-17", "2026-11-16", "2026-11-15"];
  const rows = [{ cells: [0, 240, 480, 0] }, { cells: [0, 180, 0, 0] }];
  const ponto = new Map([
    ["2026-11-18", 300],
    ["2026-11-17", 480],
    ["2026-11-16", 480],
  ]);

  const columns = gridColumns(days, rows, ponto, today);

  assert.deepEqual(
    columns.map((c) => [c.registered, c.ponto, c.diff, c.missing, c.className]),
    [
      [0, 300, -300, false, "today-col"],
      [420, 480, -60, true, ""],
      [480, 480, 0, false, ""],
      [0, 0, 0, false, "wkend"],
    ],
  );
});

test("a diferença do período ignora o ponto parcial de hoje", () => {
  const days = [
    { date: "2026-11-18", registeredMinutes: 0, pontoMinutes: 120 },
    { date: "2026-11-17", registeredMinutes: 480, pontoMinutes: 482 },
  ];
  assert.equal(differenceBeforeToday(days, today), -2);
});
