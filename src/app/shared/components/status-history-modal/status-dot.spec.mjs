import { test } from "node:test";
import assert from "node:assert/strict";
import { isStatusChange, statusBadge, statusDotColor } from "./status-dot.ts";

const statuses = [
  { id: "1", name: "A fazer", categoryKey: "new" },
  { id: "3", name: "Desenvolvendo", categoryKey: "indeterminate" },
  { id: "10", name: "Feito", categoryKey: "done" },
  { id: "20", name: "Estranho", categoryKey: "undefined" },
];

function entry(items) {
  return { changelogId: "c", authorAccountId: null, authorDisplayName: null, jiraCreatedAt: "", items };
}

function status(toId, toString, fieldId = "status") {
  return { fieldId, field: "status", fieldType: "jira", fromId: null, fromString: null, toId, toString };
}

test("usa a cor da categoria do status de destino", () => {
  assert.equal(statusDotColor(entry([status("10", "Feito")]), statuses), "var(--cat-done)");
  assert.equal(statusDotColor(entry([status("3", "Desenvolvendo")]), statuses), "var(--cat-ind)");
  assert.equal(statusDotColor(entry([status("1", "A fazer")]), statuses), "var(--cat-new)");
});

test("procura pelo nome quando o id não está no catálogo", () => {
  assert.equal(statusDotColor(entry([status("999", " feito ")]), statuses), "var(--cat-done)");
  assert.equal(statusDotColor(entry([status(null, "Desenvolvendo")]), statuses), "var(--cat-ind)");
});

test("devolve null quando não há como saber a cor", () => {
  assert.equal(statusDotColor(entry([status("999", "Sumiu")]), statuses), null);
  assert.equal(statusDotColor(entry([status("20", "Estranho")]), statuses), null);
  assert.equal(statusDotColor(entry([status("10", "Feito")]), []), null);
  assert.equal(statusDotColor(entry([status(null, null)]), statuses), null);
});

test("ignora mudanças que não são de status", () => {
  const assignee = { ...status("10", "Feito", "assignee"), field: "assignee" };
  assert.equal(statusDotColor(entry([assignee]), statuses), null);
  assert.equal(statusDotColor(entry([assignee, status("1", "A fazer")]), statuses), "var(--cat-new)");
});

test("selo do status usa a variante da categoria e fica neutro quando não acha", () => {
  assert.equal(statusBadge("10", "Feito", statuses), "ok");
  assert.equal(statusBadge("3", "Desenvolvendo", statuses), "info");
  assert.equal(statusBadge("1", "A fazer", statuses), "");
  assert.equal(statusBadge(null, " desenvolvendo ", statuses), "info");
  assert.equal(statusBadge("999", "Sumiu", statuses), "");
  assert.equal(statusBadge("20", "Estranho", statuses), "");
  assert.equal(statusBadge(null, null, statuses), "");
});

test("identifica mudanças de status pelo campo", () => {
  assert.equal(isStatusChange(status("1", "A fazer")), true);
  assert.equal(isStatusChange({ ...status("1", "A fazer", "assignee"), field: "assignee" }), false);
});
