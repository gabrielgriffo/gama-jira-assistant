import { test } from "node:test";
import assert from "node:assert/strict";
import { chronologicalOrder } from "./task-order.ts";

const task = (key, counted, dev = counted) => ({
  key,
  countedSegments: counted.map((start) => ({ start, end: start + 60 })),
  devSegments: dev.map((start) => ({ start, end: start + 60 })),
});

test("ordena pelo início do primeiro trecho contado, não pelo tempo trabalhado", () => {
  const longAfternoon = task("A-1", [600]);
  const shortMorning = task("A-2", [480]);
  assert.deepEqual(
    chronologicalOrder([longAfternoon, shortMorning]).map((t) => t.key),
    ["A-2", "A-1"],
  );
});

test("sem trecho contado usa o primeiro trecho de desenvolvimento e deixa sem trechos por último", () => {
  const noSegments = task("A-3", []);
  const devOnly = task("A-4", [], [540]);
  const counted = task("A-5", [500]);
  assert.deepEqual(
    chronologicalOrder([noSegments, devOnly, counted]).map((t) => t.key),
    ["A-5", "A-4", "A-3"],
  );
});

test("empate no início desempata pela chave e não altera a lista original", () => {
  const original = [task("B-2", [480]), task("B-1", [480])];
  assert.deepEqual(
    chronologicalOrder(original).map((t) => t.key),
    ["B-1", "B-2"],
  );
  assert.deepEqual(
    original.map((t) => t.key),
    ["B-2", "B-1"],
  );
});
