import { test } from "node:test";
import assert from "node:assert/strict";
import { downloadPercent, updateStatusText } from "./update-status.ts";

test("o percentual só aparece quando o tamanho total é conhecido", () => {
  assert.equal(downloadPercent(512, 2048), 25);
  assert.equal(downloadPercent(512, undefined), null);
  assert.equal(downloadPercent(512, 0), null);
  assert.equal(downloadPercent(4096, 2048), 100);
});

test("sem verificação, mostra só a versão instalada", () => {
  assert.equal(updateStatusText({ phase: "idle" }, "0.1.2"), "Versão 0.1.2");
});

test("o download mostra a versão nova e o progresso", () => {
  assert.equal(
    updateStatusText({ phase: "downloading", version: "0.1.3", downloaded: 300, total: 1000 }, "0.1.2"),
    "Baixando a versão 0.1.3… 30%",
  );
  assert.equal(updateStatusText({ phase: "downloading", version: "0.1.3" }, "0.1.2"), "Baixando a versão 0.1.3…");
});

test("cada fase tem um texto próprio", () => {
  assert.equal(updateStatusText({ phase: "checking" }, "0.1.2"), "Verificando atualizações…");
  assert.equal(
    updateStatusText({ phase: "ready", version: "0.1.3" }, "0.1.2"),
    "A versão 0.1.3 foi baixada e está pronta para instalar.",
  );
  assert.equal(updateStatusText({ phase: "installing", version: "0.1.3" }, "0.1.2"), "Instalando a versão 0.1.3…");
  assert.equal(updateStatusText({ phase: "up-to-date" }, "0.1.2"), "Versão 0.1.2. Esta é a versão mais recente.");
  assert.equal(
    updateStatusText({ phase: "error", error: "sem conexão" }, "0.1.2"),
    "Não foi possível atualizar: sem conexão",
  );
  assert.equal(
    updateStatusText({ phase: "unavailable" }, "0.1.2"),
    "Versão 0.1.2. A atualização automática não funciona no modo de desenvolvimento.",
  );
});
