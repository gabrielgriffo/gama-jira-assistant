import { test } from "node:test";
import assert from "node:assert/strict";
import { modalityOf, trackInputModality } from "./input-modality.ts";

const fakeDocument = () => {
  const listeners = [];
  return {
    listeners,
    documentElement: { dataset: {} },
    addEventListener: (type, handler, capture) => listeners.push({ type, handler, capture }),
    dispatch(event) {
      for (const l of listeners) if (l.type === event.type) l.handler(event);
    },
  };
};

test("pointerdown indica uso do mouse", () => {
  assert.equal(modalityOf({ type: "pointerdown" }), "pointer");
});

test("teclas de ação indicam uso do teclado", () => {
  for (const key of ["Enter", "Escape", "ArrowDown", " ", "Tab"]) {
    assert.equal(modalityOf({ type: "keydown", key }), "keyboard");
  }
});

test("teclas modificadoras sozinhas não mudam a modalidade", () => {
  for (const key of ["Shift", "Control", "Alt", "Meta"]) {
    assert.equal(modalityOf({ type: "keydown", key }), null);
  }
});

test("outros eventos não mudam a modalidade", () => {
  assert.equal(modalityOf({ type: "click" }), null);
  assert.equal(modalityOf({ type: "keyup", key: "Enter" }), null);
});

test("registra os dois ouvintes na fase de captura", () => {
  const doc = fakeDocument();
  trackInputModality(doc);
  assert.deepEqual(
    doc.listeners.map((l) => [l.type, l.capture]),
    [
      ["keydown", true],
      ["pointerdown", true],
    ],
  );
});

test("marca a última modalidade usada no documento", () => {
  const doc = fakeDocument();
  trackInputModality(doc);
  doc.dispatch({ type: "keydown", key: "Escape" });
  assert.equal(doc.documentElement.dataset.input, "keyboard");
  doc.dispatch({ type: "pointerdown" });
  assert.equal(doc.documentElement.dataset.input, "pointer");
  doc.dispatch({ type: "keydown", key: "Shift" });
  assert.equal(doc.documentElement.dataset.input, "pointer");
});
