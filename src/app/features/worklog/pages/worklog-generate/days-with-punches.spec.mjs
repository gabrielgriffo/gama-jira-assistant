import { test } from "node:test";
import assert from "node:assert/strict";
import { daysWithPunches } from "./days-with-punches.ts";

const entry = (time, disabled = false) => ({ time, entry_type: "Entrada", disabled });

test("mantém só os dias com marcação de ponto, na mesma ordem", () => {
  const days = [
    { date: "2026-11-17", entries: [] },
    { date: "2026-11-16", entries: [entry("08:00"), entry("12:00")] },
    { date: "2026-11-13", entries: [] },
    { date: "2026-11-12", entries: [entry("09:00")] },
  ];
  assert.deepEqual(
    daysWithPunches(days).map((d) => d.date),
    ["2026-11-16", "2026-11-12"],
  );
});

test("mantém o dia mesmo quando a marcação está desconsiderada", () => {
  const days = [{ date: "2026-11-16", entries: [entry("08:00", true)] }];
  assert.equal(daysWithPunches(days).length, 1);
});

test("período sem nenhuma marcação fica vazio", () => {
  assert.deepEqual(daysWithPunches([{ date: "2026-11-17", entries: [] }]), []);
  assert.deepEqual(daysWithPunches([]), []);
});
