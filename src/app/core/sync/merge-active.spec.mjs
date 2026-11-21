import { test } from "node:test";
import assert from "node:assert/strict";
import { mergeActiveSyncs } from "./merge-active.ts";

test("um fim de sincronização recebido durante a consulta prevalece sobre a foto antiga", () => {
  const merged = mergeActiveSyncs(["period:a", "issues:b"], new Map([["period:a", false]]));
  assert.deepEqual([...merged], ["issues:b"]);
});

test("um início recebido durante a consulta é mantido", () => {
  const merged = mergeActiveSyncs([], new Map([["worklogs:c", true]]));
  assert.deepEqual([...merged], ["worklogs:c"]);
});
