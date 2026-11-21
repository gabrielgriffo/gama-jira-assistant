import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveTheme } from "./theme.ts";

test("o tema em cache da sessão vale primeiro, para não piscar", () => {
  assert.equal(resolveTheme("dark", "light"), "dark");
});

test("sem cache, o tema salvo nas preferências é restaurado", () => {
  assert.equal(resolveTheme(null, "system"), "system");
});

test("valores desconhecidos caem no tema claro", () => {
  assert.equal(resolveTheme("neon", undefined), "light");
});
