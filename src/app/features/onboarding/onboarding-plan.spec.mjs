import { test } from "node:test";
import assert from "node:assert/strict";
import {
  defaultImportRange,
  earliestImportStart,
  importPlan,
  importStepText,
  isValidImportRange,
  runImportSteps,
  onboardingDecision,
  resumeStep,
  SKIP_IMPACT,
  skipAllImpact,
} from "./onboarding-plan.ts";
import { exceedsMaxRange } from "../../shared/date/range-presets.ts";

const today = "2026-11-22";

test("a configuração inicial aparece para quem começa do zero e some depois de concluída", () => {
  assert.equal(onboardingDecision(undefined, false, false), "show");
  assert.equal(onboardingDecision({ completed: true }, false, false), "hide");
  assert.equal(onboardingDecision({ completed: true }, true, true), "hide");
});

test("quem já usava o app com uma conta conectada não vê a configuração inicial", () => {
  assert.equal(onboardingDecision(undefined, true, false), "existing-user");
  assert.equal(onboardingDecision(undefined, false, true), "existing-user");
  assert.equal(onboardingDecision(undefined, true, true), "existing-user");
});

test("a configuração começada e não concluída volta mesmo com contas conectadas", () => {
  assert.equal(onboardingDecision({ completed: false }, true, false), "show");
  assert.equal(onboardingDecision({ completed: false }, true, true), "show");
  assert.equal(onboardingDecision({ completed: false }, false, false), "show");
});

test("a configuração retomada começa na primeira etapa pendente", () => {
  assert.equal(resumeStep(false, true, true), "theme");
  assert.equal(resumeStep(true, false, false), "jira");
  assert.equal(resumeStep(true, false, true), "jira");
  assert.equal(resumeStep(true, true, false), "pontomais");
  assert.equal(resumeStep(true, true, true), "date");
});

test("o período sugerido são os últimos 7 dias, terminando hoje", () => {
  assert.deepEqual(defaultImportRange(today), { from: "2026-11-16", to: today, presetId: "7d" });
  assert.deepEqual(defaultImportRange("2026-01-03"), { from: "2025-12-28", to: "2026-01-03", presetId: "7d" });
  assert.equal(isValidImportRange(defaultImportRange(today), today), true);
});

test("a data mais antiga cabe exatamente no limite de período do backend", () => {
  const earliest = earliestImportStart(today);
  assert.equal(earliest, "2025-11-22");
  assert.equal(exceedsMaxRange(earliest, today), false);
  assert.equal(exceedsMaxRange("2025-11-21", today), true);
});

test("o período da importação precisa estar entre o limite e hoje, com início antes do fim", () => {
  assert.equal(isValidImportRange({ from: "2025-11-22", to: today }, today), true);
  assert.equal(isValidImportRange({ from: today, to: today }, today), true);
  assert.equal(isValidImportRange({ from: "2026-10-01", to: "2026-10-31" }, today), true);
  assert.equal(isValidImportRange({ from: "2025-11-21", to: "2026-01-10" }, today), false);
  assert.equal(isValidImportRange({ from: "2026-11-20", to: "2026-11-23" }, today), false);
  assert.equal(isValidImportRange({ from: "2026-11-20", to: "2026-11-10" }, today), false);
  assert.equal(isValidImportRange({ from: "", to: today }, today), false);
  assert.equal(isValidImportRange({ from: "22/11/2026", to: today }, today), false);
});

test("as etapas da importação dependem das contas conectadas", () => {
  const ids = (jira, ponto) => importPlan(jira, ponto).map((s) => s.id);
  assert.deepEqual(ids(true, true), ["statuses", "ponto", "issues", "worklogs"]);
  assert.deepEqual(ids(true, false), ["statuses", "issues", "worklogs"]);
  assert.deepEqual(ids(false, true), ["ponto"]);
  assert.deepEqual(ids(false, false), []);
  assert.ok(importPlan(true, true).every((s) => s.status === "pending" && s.count === null && s.error === null));
});

test("o resumo de cada etapa usa singular, plural e um texto sem número quando não há contagem", () => {
  const done = (id, count) => importStepText({ id, status: "done", count, error: null });
  assert.equal(done("ponto", 1), "1 marcação de ponto importada");
  assert.equal(done("ponto", 42), "42 marcações de ponto importadas");
  assert.equal(done("issues", 0), "0 tarefas encontradas, com o histórico de status");
  assert.equal(done("worklogs", 1), "1 worklog encontrado no Jira");
  assert.equal(done("worklogs", null), "Worklogs encontrados no Jira");
  const failed = (id) => importStepText({ id, status: "failed", count: null, error: "x" });
  assert.equal(failed("statuses"), "Não foi possível carregar os status do Jira");
  assert.equal(failed("ponto"), "Não foi possível importar as marcações de ponto");
  assert.equal(failed("issues"), "Não foi possível buscar as tarefas e o histórico de status");
  assert.equal(failed("worklogs"), "Não foi possível buscar os seus worklogs no Jira");
  assert.equal(done("ponto", null), "Marcações de ponto importadas");
  assert.equal(
    importStepText({ id: "worklogs", status: "running", count: null, error: null }),
    "Buscando os seus worklogs no Jira",
  );
});

test("as etapas rodam em ordem e uma falha não interrompe as seguintes", async () => {
  const order = [];
  const snapshots = [];
  const result = await runImportSteps(
    importPlan(true, true),
    async (id) => {
      order.push(id);
      if (id === "ponto") throw new Error("sessão expirada");
      return id === "statuses" ? null : 7;
    },
    (steps) => snapshots.push(steps.map((s) => s.status).join(",")),
  );

  assert.deepEqual(order, ["statuses", "ponto", "issues", "worklogs"]);
  assert.deepEqual(
    result.map((s) => [s.id, s.status, s.count]),
    [
      ["statuses", "done", null],
      ["ponto", "failed", null],
      ["issues", "done", 7],
      ["worklogs", "done", 7],
    ],
  );
  assert.equal(result[1].error, "sessão expirada");
  assert.ok(snapshots.includes("running,pending,pending,pending"));
  assert.ok(snapshots.includes("done,failed,running,pending"));
});

test("tentar de novo roda só as etapas que falharam e mantém as concluídas", async () => {
  const first = await runImportSteps(
    importPlan(true, true),
    async (id) => {
      if (id === "ponto" || id === "worklogs") throw new Error("falhou");
      return 3;
    },
    () => {},
  );
  const rerun = [];
  const second = await runImportSteps(
    first,
    async (id) => {
      rerun.push(id);
      return 5;
    },
    () => {},
  );

  assert.deepEqual(rerun, ["ponto", "worklogs"]);
  assert.deepEqual(
    second.map((s) => [s.id, s.status, s.count, s.error]),
    [
      ["statuses", "done", 3, null],
      ["ponto", "done", 5, null],
      ["issues", "done", 3, null],
      ["worklogs", "done", 5, null],
    ],
  );
});

test("pular tudo avisa só o que depende das contas que ainda não foram conectadas", () => {
  const none = skipAllImpact(false, false);
  assert.deepEqual(none.features, [...SKIP_IMPACT.jira.features, ...SKIP_IMPACT.pontomais.features]);
  assert.match(none.intro, /Configurações › Conta/);

  assert.deepEqual(skipAllImpact(true, false).features, SKIP_IMPACT.pontomais.features);
  assert.deepEqual(skipAllImpact(false, true).features, SKIP_IMPACT.jira.features);

  const both = skipAllImpact(true, true);
  assert.deepEqual(both.features, []);
  assert.equal(both.intro, SKIP_IMPACT.date.intro);
});
