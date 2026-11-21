import { test } from "node:test";
import assert from "node:assert/strict";
import { changeTracker } from "./change-tracker.ts";

test("o primeiro valor conhecido é a referência e não conta como mudança", () => {
  const changed = changeTracker();
  assert.equal(changed(null), false);
  assert.equal(changed("conta-a|0"), false);
  assert.equal(changed("conta-a|0"), false);
});

test("troca de conta ou limpeza dos dados locais conta como mudança", () => {
  const changed = changeTracker();
  changed("conta-a|0");
  assert.equal(changed("-|0"), true);
  assert.equal(changed("conta-b|0"), true);
  assert.equal(changed("conta-b|1"), true);
  assert.equal(changed(null), false);
  assert.equal(changed("conta-b|1"), false);
});
