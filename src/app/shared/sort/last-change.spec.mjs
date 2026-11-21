import { test } from "node:test";
import assert from "node:assert/strict";
import { sortByLastChange } from "./last-change.ts";

const item = (key, lastChangeAt) => ({ key, lastChangeAt });

test("ordena da mudança mais recente para a mais antiga e deixa sem mudança no fim", () => {
  const sorted = sortByLastChange([
    item("GAMA-1", null),
    item("GAMA-2", "2026-11-10T12:00:00.000Z"),
    item("GAMA-3", "2026-11-17T09:00:00.000Z"),
  ]);
  assert.deepEqual(
    sorted.map((i) => i.key),
    ["GAMA-3", "GAMA-2", "GAMA-1"],
  );
});

test("desempata pela chave com comparação numérica", () => {
  const sorted = sortByLastChange([
    item("GAMA-100", null),
    item("GAMA-9", null),
    item("GAMA-20", "2026-11-10T12:00:00.000Z"),
    item("GAMA-3", "2026-11-10T12:00:00.000Z"),
  ]);
  assert.deepEqual(
    sorted.map((i) => i.key),
    ["GAMA-3", "GAMA-20", "GAMA-9", "GAMA-100"],
  );
});

test("não altera a lista original", () => {
  const items = [item("GAMA-2", null), item("GAMA-1", null)];
  sortByLastChange(items);
  assert.deepEqual(
    items.map((i) => i.key),
    ["GAMA-2", "GAMA-1"],
  );
});
