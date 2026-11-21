import { test } from "node:test";
import assert from "node:assert/strict";
import { draftSnapshot } from "./draft-snapshot.ts";

const draft = (overrides = {}) => ({
  id: 1,
  date: "2026-11-17",
  issueId: "10",
  issueKey: "GAMA-1",
  issueSummary: "Tarefa",
  start: 480,
  duration: 120,
  durationRaw: "2:00",
  comment: "",
  state: "pending",
  error: null,
  ...overrides,
});

test("rascunhos gerados de novo para o mesmo dia têm a mesma foto, mesmo com outro id", () => {
  assert.equal(
    draftSnapshot([draft({ id: 1 }), draft({ id: 2, issueKey: "GAMA-2" })]),
    draftSnapshot([draft({ id: 7 }), draft({ id: 8, issueKey: "GAMA-2" })]),
  );
});

test("mudar início, duração ou comentário muda a foto", () => {
  const base = draftSnapshot([draft()]);
  assert.notEqual(draftSnapshot([draft({ start: 540 })]), base);
  assert.notEqual(draftSnapshot([draft({ durationRaw: "1:30", duration: 90 })]), base);
  assert.notEqual(draftSnapshot([draft({ comment: "revisão" })]), base);
});

test("remover um rascunho muda a foto", () => {
  const base = draftSnapshot([draft(), draft({ id: 2, issueKey: "GAMA-2" })]);
  assert.notEqual(draftSnapshot([draft()]), base);
});

test("um envio com falha muda a foto", () => {
  assert.notEqual(draftSnapshot([draft({ state: "failed" })]), draftSnapshot([draft()]));
});
