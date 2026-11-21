import { test } from "node:test";
import assert from "node:assert/strict";
import { LatestOnly } from "./latest-only.ts";

test("só a carga mais recente continua válida", () => {
  const latest = new LatestOnly();
  const first = latest.begin();
  const second = latest.begin();
  assert.equal(first(), false);
  assert.equal(second(), true);
});
