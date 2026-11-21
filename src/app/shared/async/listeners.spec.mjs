import { test } from "node:test";
import assert from "node:assert/strict";
import { ListenerBag } from "./listeners.ts";

test("remove os listeners registrados ao descartar", async () => {
  const bag = new ListenerBag();
  let removed = 0;
  await bag.add(Promise.resolve(() => removed++));
  bag.dispose();
  assert.equal(removed, 1);
});

test("remove na hora um listener que termina de registrar depois do descarte", async () => {
  const bag = new ListenerBag();
  let removed = 0;
  let resolve;
  const pending = bag.add(new Promise((r) => (resolve = r)));
  bag.dispose();
  resolve(() => removed++);
  await pending;
  assert.equal(removed, 1);
});
