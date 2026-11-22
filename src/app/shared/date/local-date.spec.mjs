import { test } from "node:test";
import assert from "node:assert/strict";
import { brDate, sameDates } from "./local-date.ts";

test("formata data local no padrão brasileiro sem mudar o dia por fuso", () => {
  assert.equal(brDate("2026-11-12"), "12/11/2026");
  assert.equal(brDate("2026-01-01"), "01/01/2026");
});

test("mantém o texto original quando não é uma data no formato esperado", () => {
  assert.equal(brDate("12/11/2026"), "12/11/2026");
  assert.equal(brDate(""), "");
});

test("períodos com as mesmas datas são iguais mesmo sendo objetos diferentes", () => {
  assert.equal(sameDates({ from: "2026-11-01", to: "2026-11-07" }, { from: "2026-11-01", to: "2026-11-07" }), true);
  assert.equal(sameDates({ from: "2026-11-01", to: "2026-11-07" }, { from: "2026-11-01", to: "2026-11-08" }), false);
  assert.equal(sameDates(null, { from: "2026-11-01", to: "2026-11-07" }), false);
});
