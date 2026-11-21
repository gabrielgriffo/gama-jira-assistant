import { test } from "node:test";
import assert from "node:assert/strict";
import { CommandError, isMissingCredentials, toCommandError } from "./errors.ts";

test("reconhece a falta de credencial do Jira pela mensagem do backend", () => {
  assert.equal(isMissingCredentials("nenhuma credencial do Jira foi configurada"), true);
  assert.equal(isMissingCredentials(new Error("falha de comunicação com o Jira")), false);
});

test("reconhece a falta de credencial pelo código do erro do comando", () => {
  const err = toCommandError({ code: "credentials_not_found", message: "texto reescrito qualquer" });
  assert.equal(isMissingCredentials(err), true);
  assert.equal(isMissingCredentials(toCommandError({ code: "network", message: "nenhuma credencial" })), false);
});

test("o erro do comando vira texto só com a mensagem", () => {
  const err = toCommandError({ code: "network", message: "falha de comunicação com o Jira: timeout" });
  assert.ok(err instanceof CommandError);
  assert.equal(`${err}`, "falha de comunicação com o Jira: timeout");
  assert.equal(toCommandError("texto simples"), "texto simples");
});
