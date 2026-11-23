import { addDays, type LocalDate } from "../../shared/date/local-date";
import {
  DEFAULT_RANGE_PRESETS,
  exceedsMaxRange,
  MAX_RANGE_DAYS,
  type RangeSelection,
} from "../../shared/date/range-presets";

export type SkippableStep = "jira" | "pontomais" | "date";
export type ImportStepId = "statuses" | "ponto" | "issues" | "worklogs";
export type ImportStepStatus = "pending" | "running" | "done" | "failed";

export interface ImportStep {
  id: ImportStepId;
  status: ImportStepStatus;
  count: number | null;
  error: string | null;
}

export const SKIP_IMPACT: Record<SkippableStep, SkipImpact> = {
  jira: {
    title: "Pular a conta do Jira?",
    intro: "Sem a conta do Jira, estas funcionalidades não vão funcionar até você conectá-la em Configurações › Conta:",
    features: ["Tarefas e histórico de status", "Gerar worklogs e enviá-los ao Jira", "Histórico de worklogs"],
  },
  pontomais: {
    title: "Pular a conta do PontoMais?",
    intro:
      "Sem a conta do PontoMais, estas funcionalidades não vão funcionar até você conectá-la em Configurações › Conta:",
    features: [
      "Importar as marcações de ponto",
      "Gerar worklogs a partir dos horários do ponto",
      "Comparar o ponto com as horas registradas no Histórico de worklogs",
    ],
  },
  date: {
    title: "Pular a primeira importação?",
    intro:
      "Sem a primeira importação, cada tela busca os dados do período que você abrir, e a primeira consulta de cada período pode demorar mais.",
    features: [],
  },
};

export interface SkipImpact {
  title: string;
  intro: string;
  features: string[];
}

export function skipAllImpact(jira: boolean, pontoMais: boolean): SkipImpact {
  const features = [...(jira ? [] : SKIP_IMPACT.jira.features), ...(pontoMais ? [] : SKIP_IMPACT.pontomais.features)];
  const intro = features.length
    ? "A primeira importação não será feita, e estas funcionalidades não vão funcionar até você conectar as contas em Configurações › Conta:"
    : SKIP_IMPACT.date.intro;
  return { title: "Pular a configuração inicial?", intro, features };
}

export type OnboardingDecision = "show" | "hide" | "existing-user";
export type ResumeStep = "theme" | "jira" | "pontomais" | "date";

export function onboardingDecision(
  progress: { completed: boolean } | undefined,
  jira: boolean,
  pontoMais: boolean,
): OnboardingDecision {
  if (progress?.completed) return "hide";
  if (progress) return "show";
  return jira || pontoMais ? "existing-user" : "show";
}

export function resumeStep(themeSeen: boolean, jira: boolean, pontoMais: boolean): ResumeStep {
  if (!themeSeen) return "theme";
  if (!jira) return "jira";
  if (!pontoMais) return "pontomais";
  return "date";
}

const DEFAULT_IMPORT_PRESET = "7d";

export function earliestImportStart(today: LocalDate): LocalDate {
  return addDays(today, -(MAX_RANGE_DAYS - 1));
}

export function defaultImportRange(today: LocalDate): RangeSelection {
  const preset = DEFAULT_RANGE_PRESETS.find((p) => p.id === DEFAULT_IMPORT_PRESET)!;
  return { ...preset.resolve(today), presetId: preset.id };
}

export function isValidImportRange(range: RangeSelection, today: LocalDate): boolean {
  const date = /^\d{4}-\d{2}-\d{2}$/;
  return (
    date.test(range.from) &&
    date.test(range.to) &&
    range.from <= range.to &&
    range.to <= today &&
    range.from >= earliestImportStart(today) &&
    !exceedsMaxRange(range.from, range.to)
  );
}

export function importPlan(jira: boolean, pontoMais: boolean): ImportStep[] {
  const ids: ImportStepId[] = [];
  if (jira) ids.push("statuses");
  if (pontoMais) ids.push("ponto");
  if (jira) ids.push("issues", "worklogs");
  return ids.map((id) => ({ id, status: "pending", count: null, error: null }));
}

const counted = (count: number | null, one: string, many: string, fallback: string) =>
  count === null ? fallback : `${count} ${count === 1 ? one : many}`;

export function importStepText(step: ImportStep): string {
  const done = step.status === "done";
  if (step.status === "failed") return FAILED_TEXT[step.id];
  switch (step.id) {
    case "statuses":
      return done ? "Status do Jira carregados" : "Carregando os status do Jira";
    case "ponto":
      return done
        ? counted(
            step.count,
            "marcação de ponto importada",
            "marcações de ponto importadas",
            "Marcações de ponto importadas",
          )
        : "Importando as marcações de ponto";
    case "issues":
      return done
        ? `${counted(step.count, "tarefa encontrada", "tarefas encontradas", "Tarefas encontradas")}, com o histórico de status`
        : "Buscando as tarefas que passaram por você e o histórico de status";
    case "worklogs":
      return done
        ? `${counted(step.count, "worklog encontrado", "worklogs encontrados", "Worklogs encontrados")} no Jira`
        : "Buscando os seus worklogs no Jira";
  }
}

const FAILED_TEXT: Record<ImportStepId, string> = {
  statuses: "Não foi possível carregar os status do Jira",
  ponto: "Não foi possível importar as marcações de ponto",
  issues: "Não foi possível buscar as tarefas e o histórico de status",
  worklogs: "Não foi possível buscar os seus worklogs no Jira",
};

export async function runImportSteps(
  steps: ImportStep[],
  run: (id: ImportStepId) => Promise<number | null>,
  onChange: (steps: ImportStep[]) => void,
): Promise<ImportStep[]> {
  let current = steps.map((step) =>
    step.status === "done" ? step : { ...step, status: "pending" as const, error: null },
  );
  onChange(current);
  for (const step of current) {
    if (step.status === "done") continue;
    const update = (patch: Partial<ImportStep>) => {
      current = current.map((s) => (s.id === step.id ? { ...s, ...patch } : s));
      onChange(current);
    };
    update({ status: "running" });
    try {
      update({ status: "done", count: await run(step.id) });
    } catch (err) {
      update({ status: "failed", error: err instanceof Error ? err.message : String(err) });
    }
  }
  return current;
}
