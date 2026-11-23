import { test } from "node:test";
import assert from "node:assert/strict";
import { OPEN_DELAY_MS, canTruncate, isTruncated, openDelay, placeTooltip, tooltipText } from "./truncation.ts";

const metrics = (over = {}) => ({
  textOverflow: "clip",
  lineClamp: "none",
  scrollWidth: 100,
  clientWidth: 100,
  scrollHeight: 20,
  clientHeight: 20,
  ...over,
});

test("texto com reticências só conta como cortado quando passa da largura", () => {
  assert.equal(isTruncated(metrics({ textOverflow: "ellipsis", scrollWidth: 140 })), true);
  assert.equal(isTruncated(metrics({ textOverflow: "ellipsis" })), false);
});

test("texto sem reticências não conta como cortado mesmo transbordando", () => {
  assert.equal(isTruncated(metrics({ scrollWidth: 140 })), false);
});

test("line-clamp conta como cortado quando passa da altura", () => {
  assert.equal(isTruncated(metrics({ lineClamp: "2", scrollHeight: 48, clientHeight: 32 })), true);
  assert.equal(isTruncated(metrics({ lineClamp: "2", scrollHeight: 32, clientHeight: 32 })), false);
  assert.equal(isTruncated(metrics({ lineClamp: "", scrollHeight: 48, clientHeight: 32 })), false);
});

test("só elementos com reticências ou line-clamp podem cortar texto", () => {
  assert.equal(canTruncate({ textOverflow: "ellipsis", lineClamp: "none" }), true);
  assert.equal(canTruncate({ textOverflow: "clip", lineClamp: "2" }), true);
  assert.equal(canTruncate({ textOverflow: "clip", lineClamp: "none" }), false);
});

test("o primeiro tooltip espera o atraso; os seguintes abrem na hora", () => {
  assert.equal(openDelay(null, 1000, false), OPEN_DELAY_MS);
  assert.equal(openDelay(null, 1000, true), 0);
  assert.equal(openDelay(1000, 1300, false), 0);
  assert.equal(openDelay(1000, 1301, false), OPEN_DELAY_MS);
});

test("o texto do tooltip junta espaços e quebras de linha", () => {
  assert.equal(tooltipText("  Analisar\n   campos   ausentes "), "Analisar campos ausentes");
});

const viewport = { width: 1100, height: 600 };

test("fica acima do texto, alinhado ao início dele", () => {
  const place = placeTooltip({ left: 300, top: 200, width: 150, height: 20 }, { width: 240, height: 28 }, viewport);
  assert.deepEqual(place, { left: 292, top: 166, side: "top", originX: 16 });
});

test("vai para baixo quando não cabe acima", () => {
  const place = placeTooltip({ left: 300, top: 20, width: 150, height: 20 }, { width: 240, height: 28 }, viewport);
  assert.equal(place.side, "bottom");
  assert.equal(place.top, 46);
});

test("não sai da janela pela direita nem pela esquerda", () => {
  const right = placeTooltip({ left: 1000, top: 200, width: 90, height: 20 }, { width: 240, height: 28 }, viewport);
  assert.equal(right.left, 852);
  assert.equal(right.originX, 156);
  const left = placeTooltip({ left: 2, top: 200, width: 90, height: 20 }, { width: 240, height: 28 }, viewport);
  assert.equal(left.left, 8);
  assert.equal(left.originX, 2);
});
