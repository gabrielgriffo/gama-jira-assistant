import { test } from "node:test";
import assert from "node:assert/strict";
import { Adjustments } from "./adjustments.ts";

const day = (worked, total = 480) => ({
  date: "2026-11-17",
  totalMinutes: total,
  tasksWorked: [{ issueId: "10", workedMinutes: worked }],
});
const task = { issueId: "10" };

test("mantém o ajuste enquanto o dia não muda", () => {
  const adjustments = new Adjustments();
  adjustments.set(day(450), task, 30);
  assert.equal(adjustments.get(day(450), task), 30);
});

test("descarta o ajuste quando a recarga muda o tempo calculado", () => {
  const adjustments = new Adjustments();
  adjustments.set(day(450), task, 30);
  assert.equal(adjustments.get(day(480), task), 0);
});

test("descarta o ajuste quando o ponto do dia muda", () => {
  const adjustments = new Adjustments();
  adjustments.set(day(450), task, 30);
  assert.equal(adjustments.get(day(450, 500), task), 0);
});

test("limpa os ajustes de um dia", () => {
  const adjustments = new Adjustments();
  adjustments.set(day(450), task, 30);
  adjustments.clearDay("2026-11-17");
  assert.equal(adjustments.get(day(450), task), 0);
});
