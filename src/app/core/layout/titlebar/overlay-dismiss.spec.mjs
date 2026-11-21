import { test } from "node:test";
import assert from "node:assert/strict";
import { leftSlop, topOverlay } from "./overlay-dismiss.ts";

const overlay = (name, leaving = false) => ({
  name,
  closest: (sel) => (sel === ".modal-leave" && leaving ? {} : null),
});

test("escolhe a sobreposição mais acima, a última do documento", () => {
  const list = [overlay("configurações"), overlay("login")];
  assert.equal(topOverlay(list).name, "login");
});

test("ignora sobreposições que já estão saindo", () => {
  const list = [overlay("configurações"), overlay("login", true)];
  assert.equal(topOverlay(list).name, "configurações");
  assert.equal(topOverlay([overlay("histórico", true)]), null);
});

test("sem sobreposição não há o que fechar", () => {
  assert.equal(topOverlay([]), null);
});

test("só considera que o mouse saiu do lugar depois de mais de 4px", () => {
  const start = { x: 100, y: 20 };
  assert.equal(leftSlop(start, { x: 104, y: 16 }), false);
  assert.equal(leftSlop(start, { x: 105, y: 20 }), true);
  assert.equal(leftSlop(start, { x: 100, y: 25 }), true);
});
