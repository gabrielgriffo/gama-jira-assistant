import { test } from "node:test";
import assert from "node:assert/strict";
import { categoryOf, filterTasks, sortTasks, tabCounts } from "./task-filters.ts";

const task = (key, summary, statusCategoryKey, statusIsDevelopment = false) => ({
  key,
  summary,
  statusCategoryKey,
  statusIsDevelopment,
});

const tasks = [
  task("GAMA-1", "Corrigir login", "new"),
  task("GAMA-2", "Integração com o Jira", "indeterminate", true),
  task("OUTRO-3", "Relatório de vendas", "done"),
  task("GAMA-4", "Ajustar relatório", "indeterminate"),
  task("GAMA-5", "Categoria desconhecida", "undefined", true),
];

test("categoria segue a do status e trata desconhecidas como em andamento", () => {
  assert.equal(categoryOf(tasks[0]), "new");
  assert.equal(categoryOf(tasks[2]), "done");
  assert.equal(categoryOf(tasks[4]), "indeterminate");
});

test("contagem por aba considera todas as tarefas", () => {
  assert.deepEqual(
    tabCounts(tasks).map((t) => [t.id, t.label, t.count]),
    [
      ["all", "Todas", 5],
      ["development", "Em desenvolvimento", 2],
      ["new", "A fazer", 1],
      ["done", "Concluído", 1],
    ],
  );
});

test("abas A fazer e Concluído filtram pela categoria", () => {
  assert.deepEqual(
    filterTasks(tasks, "new", "").map((t) => t.key),
    ["GAMA-1"],
  );
  assert.deepEqual(
    filterTasks(tasks, "done", "").map((t) => t.key),
    ["OUTRO-3"],
  );
  assert.equal(filterTasks(tasks, "all", "").length, 5);
});

test("aba Em desenvolvimento mostra só os status marcados nas Configurações", () => {
  assert.deepEqual(
    filterTasks(tasks, "development", "").map((t) => t.key),
    ["GAMA-2", "GAMA-5"],
  );
  assert.deepEqual(
    filterTasks(tasks, "development", "jira").map((t) => t.key),
    ["GAMA-2"],
  );
});

test("busca por chave ou título ignora maiúsculas, acentos e espaços nas pontas", () => {
  assert.deepEqual(
    filterTasks(tasks, "all", "  RELATORIO ").map((t) => t.key),
    ["OUTRO-3", "GAMA-4"],
  );
  assert.deepEqual(
    filterTasks(tasks, "all", "outro-3").map((t) => t.key),
    ["OUTRO-3"],
  );
  assert.deepEqual(
    filterTasks(tasks, "all", "integracao").map((t) => t.key),
    ["GAMA-2"],
  );
});

test("busca combina com a aba selecionada", () => {
  assert.deepEqual(
    filterTasks(tasks, "done", "relatório").map((t) => t.key),
    ["OUTRO-3"],
  );
  assert.deepEqual(filterTasks(tasks, "new", "relatório"), []);
});

const sortable = (key, lastChangeAt, statusIsDevelopment = false) => ({ key, lastChangeAt, statusIsDevelopment });
const keys = (list) => list.map((t) => t.key);

test("ordena pela última mudança de status ou responsável, da mais recente para a mais antiga", () => {
  const list = [
    sortable("A-1", "2026-10-02T10:00:00.000Z"),
    sortable("A-2", "2026-10-05T10:00:00.000Z"),
    sortable("A-3", "2026-10-03T10:00:00.000Z"),
  ];
  assert.deepEqual(keys(sortTasks(list, false)), ["A-2", "A-3", "A-1"]);
});

test("tarefas sem mudança registrada ficam no fim", () => {
  const list = [sortable("A-1", null), sortable("A-2", "2026-10-01T10:00:00.000Z"), sortable("A-3", null)];
  assert.deepEqual(keys(sortTasks(list, false)), ["A-2", "A-1", "A-3"]);
});

test("com o interruptor ligado, tarefas em status de desenvolvimento vêm primeiro", () => {
  const list = [
    sortable("A-1", "2026-10-09T10:00:00.000Z"),
    sortable("A-2", "2026-10-01T10:00:00.000Z", true),
    sortable("A-3", null, true),
    sortable("A-4", "2026-10-05T10:00:00.000Z", true),
  ];
  assert.deepEqual(keys(sortTasks(list, true)), ["A-4", "A-2", "A-3", "A-1"]);
  assert.deepEqual(keys(sortTasks(list, false)), ["A-1", "A-4", "A-2", "A-3"]);
});

test("empate na data usa a chave em ordem numérica e não altera a lista original", () => {
  const list = [sortable("A-10", "2026-10-01T10:00:00.000Z"), sortable("A-9", "2026-10-01T10:00:00.000Z")];
  assert.deepEqual(keys(sortTasks(list, true)), ["A-9", "A-10"]);
  assert.deepEqual(keys(list), ["A-10", "A-9"]);
});
