import { test } from "node:test";
import assert from "node:assert/strict";
import { existsInZone, jiraOffset, validateDrafts } from "./draft-validation.ts";

const draft = (overrides = {}) => ({
  id: 1,
  issueKey: "GAMA-1",
  start: 480,
  duration: 60,
  comment: "",
  state: "pending",
  ...overrides,
});

test("rascunho válido não tem erro", () => {
  assert.deepEqual(validateDrafts([draft()]), {});
});

test("duração zero aponta o campo de duração", () => {
  assert.equal(validateDrafts([draft({ duration: 0 })])[1].field, "duration");
});

test("início apagado vira erro no campo de início", () => {
  const error = validateDrafts([draft({ start: NaN })])[1];
  assert.equal(error.field, "start");
  assert.match(error.text, /horário de início/);
});

test("duração acima de 24 horas é recusada antes do envio", () => {
  assert.equal(validateDrafts([draft({ duration: 25 * 60 })])[1].field, "duration");
});

test("worklog que passa da meia-noite é recusado", () => {
  const error = validateDrafts([draft({ start: 23 * 60 + 30, duration: 60 })])[1];
  assert.equal(error.field, "start");
  assert.match(error.text, /mesmo dia/);
});

test("comentário acima do limite do backend é recusado", () => {
  assert.equal(validateDrafts([draft({ comment: "x".repeat(5001) })])[1].field, "comment");
});

test("sobreposição aponta o início e ignora rascunhos com início inválido", () => {
  const errors = validateDrafts([
    draft({ id: 1, start: 480, duration: 120 }),
    draft({ id: 2, issueKey: "GAMA-2", start: 540, duration: 60 }),
    draft({ id: 3, issueKey: "GAMA-3", start: NaN }),
  ]);
  assert.equal(errors[1].field, "start");
  assert.match(errors[1].text, /GAMA-2/);
  assert.match(errors[3].text, /horário de início/);
});

test("rascunho já enviado não é validado de novo", () => {
  assert.deepEqual(validateDrafts([draft({ duration: 0, state: "sent" })]), {});
});

test("offset usa o horário do worklog no dia de troca de horário de verão", () => {
  assert.equal(jiraOffset("2026-03-08", 60, "America/New_York"), "-0500");
  assert.equal(jiraOffset("2026-03-08", 9 * 60, "America/New_York"), "-0400");
  assert.equal(jiraOffset("2026-11-17", 9 * 60, "America/Sao_Paulo"), "-0300");
});

test("início dentro do salto do horário de verão é recusado antes do envio", () => {
  assert.equal(existsInZone("2026-03-08", 2 * 60 + 30, "America/New_York"), false);
  assert.equal(existsInZone("2026-03-08", 3 * 60 + 30, "America/New_York"), true);
  assert.equal(existsInZone("2026-11-17", 9 * 60, "America/Sao_Paulo"), true);

  const errors = validateDrafts([draft({ date: "2026-03-08", start: 150 })], "America/New_York");
  assert.equal(errors[1].field, "start");
  assert.match(errors[1].text, /horário de verão/);
  assert.deepEqual(validateDrafts([draft({ date: "2026-03-08", start: 150 })]), {});
});
