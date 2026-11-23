import { computed, inject, Injectable, signal } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { statusCommands, worklogsCommands } from "../../core/tauri";
import { StatusCatalogStore } from "../../core/issues/status-catalog.store";
import { PontoMaisService } from "../../core/pontomais/pontomais.service";
import { ToastService } from "../../core/feedback/toast.service";
import { JiraWorklogService } from "../worklog/data-access/jira-worklog.service";
import type { DateRange } from "../../shared/date/local-date";
import { importPlan, ImportStep, ImportStepId, runImportSteps } from "./onboarding-plan";

export type FirstImportPhase = "idle" | "running" | "finished";

@Injectable({
  providedIn: "root",
})
export class FirstImportService {
  private readonly statusCatalog = inject(StatusCatalogStore);
  private readonly pontoMais = inject(PontoMaisService);
  private readonly jiraWorklogs = inject(JiraWorklogService);
  private readonly toast = inject(ToastService);

  private readonly _steps = signal<ImportStep[]>([]);
  private readonly _phase = signal<FirstImportPhase>("idle");
  private readonly _range = signal<DateRange | null>(null);
  private inBackground = false;

  readonly steps = this._steps.asReadonly();
  readonly phase = this._phase.asReadonly();
  readonly range = this._range.asReadonly();
  readonly hasFailures = computed(() => this._steps().some((step) => step.status === "failed"));

  async start(range: DateRange, jira: boolean, pontoMais: boolean): Promise<void> {
    if (this._phase() === "running") return;
    this._range.set(range);
    this._steps.set(importPlan(jira, pontoMais));
    await this.run();
  }

  async retry(): Promise<void> {
    if (this._phase() !== "finished" || !this.hasFailures()) return;
    await this.run();
  }

  continueInBackground(): void {
    this.inBackground = true;
  }

  private async run(): Promise<void> {
    const range = this._range();
    if (!range) return;
    this._phase.set("running");
    await runImportSteps(
      this._steps(),
      (id) => this.execute(id, range),
      (steps) => this._steps.set(steps),
    );
    for (const step of this._steps()) {
      if (step.status === "failed") error(`first import step ${step.id} failed: ${step.error}`);
    }
    this._phase.set("finished");
    if (this.inBackground) this.notifyFinished();
  }

  private async execute(id: ImportStepId, { from, to }: DateRange): Promise<number | null> {
    switch (id) {
      case "statuses":
        await statusCommands.syncCatalog();
        await this.statusCatalog.load();
        return null;
      case "ponto":
        return this.pontoMais.syncPontoMaisEntries(from, to);
      case "issues":
        return worklogsCommands.syncPeriodIssues(from, to);
      case "worklogs":
        return this.jiraWorklogs.syncWorklogs(from, to, true);
    }
  }

  private notifyFinished(): void {
    if (this.hasFailures()) {
      this.toast.error(
        "A primeira importação terminou com erros. Os dados que faltaram serão buscados quando você abrir cada tela.",
        6000,
      );
    } else {
      this.toast.success("Primeira importação concluída.");
    }
  }
}
