import { test } from "node:test";
import assert from "node:assert/strict";
import { monthGroups } from "./month-groups.ts";

test("agrupa dias consecutivos do mesmo mês com a sigla em três letras", () => {
  assert.deepEqual(monthGroups(["2026-11-29", "2026-11-30", "2026-12-01"]), [
    { key: "2026-11", label: "NOV", span: 2 },
    { key: "2026-12", label: "DEZ", span: 1 },
  ]);
});

test("separa o mesmo mês de anos diferentes", () => {
  assert.deepEqual(
    monthGroups(["2025-12-31", "2026-01-01", "2026-12-01"]).map((g) => g.key),
    ["2025-12", "2026-01", "2026-12"],
  );
});

test("devolve lista vazia sem dias", () => {
  assert.deepEqual(monthGroups([]), []);
});
