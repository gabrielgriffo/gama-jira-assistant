import { Component, NgZone, OnDestroy, OnInit, effect, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import {
  MinuteRange,
  PontoMaisService,
  TaskWorkedInfo,
  WorkDayTimeline,
} from "../../../../core/pontomais/pontomais.service";
import { WorklogDraft } from "../../data-access/worklog";
import { daysWithPunches } from "./days-with-punches";
import { draftSnapshot } from "./draft-snapshot";
import { IssueChange } from "../../../../core/models";
import { StatusHistoryModalComponent } from "../../../../shared/components/status-history-modal/status-history-modal.component";
import { DayWorklogPreviewModalComponent } from "../../dialogs/day-worklog-preview-modal/day-worklog-preview-modal.component";
import { TaskService } from "../../../../core/issues/task.service";
import { ToastService } from "../../../../core/feedback/toast.service";
import { DateRangePickerComponent } from "../../../../shared/ui/date-range-picker/date-range-picker.component";
import { weekday } from "../../../../shared/date/local-date";
import { RangeSelection, WORKLOG_GENERATE_PRESETS } from "../../../../shared/date/range-presets";
import { isMissingCredentials, worklogsCommands } from "../../../../core/tauri";
import { LatestOnly } from "../../../../shared/async/latest-only";
import { ListenerBag } from "../../../../shared/async/listeners";
import { Adjustments } from "./adjustments";
import { AppSettingsService } from "../../../../core/preferences/app-settings.service";
import { rangeSyncKey, SyncStateService } from "../../../../core/sync/sync-state.service";
import { PontoMaisStateService } from "../../../../core/pontomais/pontomais-state.service";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { StatusCatalogStore } from "../../../../core/issues/status-catalog.store";
import { LocalDataService } from "../../../../core/sync/local-data.service";
import { changeTracker } from "../../../../shared/async/change-tracker";
import { WorklogDraftStore } from "../../data-access/worklog-draft.store";

const MIN_AXIS_START = 7 * 60;
const MIN_AXIS_END = 19 * 60;
const WEEKDAYS = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];
const NO_DRAFTS: WorklogDraft[] = [];
const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

@Component({
  selector: "app-worklog-generate",
  imports: [
    IconComponent,
    CommonModule,
    FormsModule,
    RouterModule,
    StatusHistoryModalComponent,
    DayWorklogPreviewModalComponent,
    DateRangePickerComponent,
  ],
  templateUrl: "./worklog-generate.page.html",
  styleUrl: "./worklog-generate.page.css",
})
export class WorklogGeneratePage implements OnInit, OnDestroy {
  readonly presets = WORKLOG_GENERATE_PRESETS;
  axisStart = MIN_AXIS_START;
  axisEnd = MIN_AXIS_END;
  axisTicks: { label: string; left: string }[] = [];
  axisGuides: string[] = [];

  private readonly settings = inject(AppSettingsService);
  get timeZone(): string {
    return this.settings.timeZone;
  }
  range: RangeSelection = this.settings.range("worklogGenerate", "week", this.presets);

  importingPonto = false;
  timelineLoaded = false;
  loadError = "";
  timeline: WorkDayTimeline[] = [];

  isModalOpen = false;
  selectedTaskKey = "";
  statusHistory: IssueChange[] = [];
  earliestDate: string | null = null;
  latestDate: string | null = null;
  fetchingStatusHistory = false;
  historyError = "";

  showDayWorklogPreview = false;
  dayWorklogPreviewDate = "";

  private readonly adjustments = new Adjustments();
  private readonly draftStore = inject(WorklogDraftStore);
  private readonly drafts = this.draftStore.drafts;
  private readonly generatedDrafts = this.draftStore.generated;
  private readonly listeners = new ListenerBag();
  private readonly timelineLoads = new LatestOnly();
  private loadedRange: RangeSelection | null = null;
  private readonly syncState = inject(SyncStateService);
  private readonly pontoMaisState = inject(PontoMaisStateService);

  constructor(
    private pontoMaisService: PontoMaisService,
    private taskService: TaskService,
    private ngZone: NgZone,
    private toastService: ToastService,
  ) {
    this.updateAxis();

    const localData = inject(LocalDataService);
    const sessionChanged = changeTracker<string>();
    effect(() => {
      if (!sessionChanged(localData.sessionKey())) return;
      this.resetForNewSession();
    });

    const statusCatalog = inject(StatusCatalogStore);
    const catalogChanged = changeTracker<unknown>();
    effect(() => {
      if (catalogChanged(statusCatalog.statuses())) this.loadTimeline();
    });

    const pontoChanged = changeTracker<string>();
    effect(() => {
      const ponto = this.pontoMaisState.state();
      if (!pontoChanged(`${ponto.connected}|${ponto.account ?? ""}`)) return;
      this.loadTimeline();
      if (ponto.connected) this.syncPeriod(false);
    });
  }

  get hasPontoMais(): boolean {
    return this.pontoMaisState.state().connected;
  }

  private resetForNewSession(): void {
    this.timelineLoads.begin();
    this.timeline = [];
    this.loadedRange = null;
    this.loadError = "";
    this.adjustments.clear();
    this.draftStore.clear();
    this.showDayWorklogPreview = false;
    this.dayWorklogPreviewDate = "";
    this.closeModal();
    this.updateAxis();
    this.loadTimeline();
    this.syncPeriod(false);
  }

  get syncing(): boolean {
    return this.syncState.isRangeSyncing("period", this.range);
  }

  get loadingStatusHistory(): boolean {
    return this.fetchingStatusHistory;
  }

  get syncingStatusHistory(): boolean {
    return this.isModalOpen && this.syncState.isChangelogSyncing(this.selectedTaskKey);
  }

  async ngOnInit(): Promise<void> {
    await this.setupEventListeners();
    if (this.listeners.isDisposed) return;
    await this.refreshPontoMaisConnection();
    await this.loadTimeline();
    await this.syncPeriod(false);
  }

  ngOnDestroy(): void {
    this.listeners.dispose();
  }

  private async setupEventListeners(): Promise<void> {
    await this.listeners.add(
      Promise.resolve(
        this.syncState.onFinished((key) => {
          if (key === rangeSyncKey("period", this.range)) {
            this.loadTimeline();
          }
        }),
      ),
    );

    await this.listeners.add(
      this.taskService.onStatusHistorySynced((issueKey: string) => {
        this.ngZone.run(async () => {
          if (this.isModalOpen && this.selectedTaskKey === issueKey) {
            await this.reloadStatusHistory();
          }
        });
      }),
    );

    await this.listeners.add(
      this.taskService.onStatusHistorySyncFailed((payload: { issueKey: string; reason: string }) => {
        this.ngZone.run(() => {
          if (this.isModalOpen && this.selectedTaskKey === payload.issueKey) {
            if (!this.statusHistory.length) {
              this.historyError = "Não foi possível carregar o histórico de status.";
            }
          }
        });
      }),
    );
  }

  async applyRange(range: RangeSelection): Promise<void> {
    this.range = range;
    this.adjustments.clear();
    this.settings.saveRange("worklogGenerate", range, this.presets);
    await this.loadTimeline();
    await this.syncPeriod(false);
  }

  get emptyHint(): string {
    return this.hasPontoMais
      ? "Utilize Importar ponto para obter as marcações."
      : "Conecte a conta do PontoMais em Configurações para importar as marcações.";
  }

  async loadTimeline(): Promise<void> {
    const range = this.range;
    const isCurrent = this.timelineLoads.begin();
    try {
      const timeline = daysWithPunches(await this.pontoMaisService.getPontoMaisTimeline(range.from, range.to));
      if (!isCurrent()) return;
      this.timeline = timeline;
      this.loadedRange = range;
      this.loadError = "";
      this.updateAxis();
    } catch (err) {
      if (!isCurrent()) return;
      error(`failed to load worklog timeline: ${err}`);
      this.toastService.error("Não foi possível carregar a linha do tempo do período.");
      if (this.loadedRange !== range) {
        this.timeline = [];
        this.loadError = "Não foi possível carregar a linha do tempo do período selecionado.";
        this.updateAxis();
      }
    } finally {
      if (isCurrent()) this.timelineLoaded = true;
    }
  }

  async syncPeriod(notify = true): Promise<void> {
    if (this.syncing) return;
    try {
      const summary = await worklogsCommands.syncWorklogPeriod(this.range.from, this.range.to, notify);
      if (summary && notify) {
        const ponto =
          summary.pontoEntries === null ? "PontoMais não conectado" : `${summary.pontoEntries} marcações de ponto`;
        this.toastService.success(
          `Sincronização concluída: ${ponto}, ${summary.issues} issues e ${summary.worklogs} worklogs.`,
        );
      }
    } catch (err) {
      error(`failed to sync worklog period: ${err}`);
      await this.refreshPontoMaisConnection();
      if (notify || !isMissingCredentials(err)) {
        this.toastService.error(`Não foi possível sincronizar o período: ${err}`);
      }
    }
  }

  async importPonto(): Promise<void> {
    this.importingPonto = true;
    try {
      const total = await this.pontoMaisService.syncPontoMaisEntries(this.range.from, this.range.to);
      await this.loadTimeline();
      this.toastService.success(`${total} marcações de ponto importadas.`);
    } catch (err) {
      error(`failed to import PontoMais entries: ${err}`);
      await this.refreshPontoMaisConnection();
      this.toastService.error(`Não foi possível importar o ponto: ${err}`);
    } finally {
      this.importingPonto = false;
    }
  }

  private async refreshPontoMaisConnection(): Promise<void> {
    await this.pontoMaisState.refresh();
  }

  scrollToTop(element: HTMLElement): void {
    element.scrollTo({ top: 0, behavior: "smooth" });
  }

  scrollToBottom(element: HTMLElement): void {
    element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
  }

  trackDay(_: number, day: WorkDayTimeline): string {
    return day.date;
  }

  trackTask(_: number, task: TaskWorkedInfo): string {
    return task.issueId;
  }

  dayTitle(date: string): string {
    const [, m, d] = date.split("-").map(Number);
    return `${WEEKDAYS[weekday(date)]}, ${d} ${MONTHS[m - 1]}`;
  }

  hfmt(minutes: number): string {
    const sign = minutes < 0 ? "-" : "";
    const m = Math.abs(Math.round(minutes));
    return `${sign}${Math.floor(m / 60)}h${String(m % 60).padStart(2, "0")}`;
  }

  clock(minutes: number): string {
    const m = Math.max(0, Math.min(1440, Math.round(minutes)));
    return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
  }

  pct(minutes: number): string {
    const clamped = Math.min(Math.max(minutes, this.axisStart), this.axisEnd);
    return `${(((clamped - this.axisStart) / (this.axisEnd - this.axisStart)) * 100).toFixed(3)}%`;
  }

  private updateAxis(): void {
    const ranges = this.timeline.flatMap((day) => [
      ...day.punchIntervals,
      ...day.tasksWorked.flatMap((task) => task.countedSegments),
    ]);
    const earliest = Math.min(MIN_AXIS_START, ...ranges.map((r) => r.start));
    const latest = Math.max(MIN_AXIS_END, ...ranges.map((r) => r.end));
    this.axisStart = Math.max(0, Math.floor(earliest / 60) * 60);
    this.axisEnd = Math.min(1440, Math.ceil(latest / 60) * 60);

    const first = this.axisStart / 60;
    const last = this.axisEnd / 60;
    const hours: number[] = [];
    for (let h = first; h <= last; h++) {
      if (h === first || h === last || h % 2 === 1) hours.push(h);
    }
    this.axisTicks = hours.map((h) => ({ label: `${String(h).padStart(2, "0")}h`, left: this.pct(h * 60) }));
    this.axisGuides = hours.slice(1, -1).map((h) => this.pct(h * 60));
  }

  spanStyle(segment: MinuteRange): Record<string, string> {
    return { left: this.pct(segment.start), width: `calc(${this.pct(segment.end)} - ${this.pct(segment.start)})` };
  }

  punchText(day: WorkDayTimeline): string {
    const active = day.entries.filter((e) => !e.disabled).map((e) => e.time);
    const parts: string[] = [];
    for (let i = 0; i < active.length; i += 2) {
      const end = active[i + 1] ?? (day.open ? "agora" : "sem saída");
      parts.push(`${active[i]}–${end}`);
    }
    return parts.join(", ") || "—";
  }

  lastPunch(day: WorkDayTimeline): string {
    const active = day.entries.filter((e) => !e.disabled);
    return active[active.length - 1]?.time ?? "";
  }

  devRange(task: TaskWorkedInfo): string {
    if (!task.devSegments.length) return "";
    return `${this.clock(task.devSegments[0].start)}–${this.clock(task.devSegments[task.devSegments.length - 1].end)}`;
  }

  adjustment(day: WorkDayTimeline, task: TaskWorkedInfo): number {
    return this.adjustments.get(day, task);
  }

  finalMinutes(day: WorkDayTimeline, task: TaskWorkedInfo): number {
    return Math.max(0, task.workedMinutes + this.adjustment(day, task));
  }

  calculated(day: WorkDayTimeline): number {
    return day.tasksWorked.reduce((sum, task) => sum + this.finalMinutes(day, task), 0);
  }

  difference(day: WorkDayTimeline): number {
    return day.totalMinutes - this.calculated(day);
  }

  canFix(day: WorkDayTimeline): boolean {
    return this.difference(day) !== 0 && !day.open && day.gamaSyncedCount === 0 && day.totalMinutes > 0;
  }

  fix(day: WorkDayTimeline, task: TaskWorkedInfo): void {
    const current = this.adjustments.get(day, task);
    this.adjustments.set(day, task, Math.max(1 - task.workedMinutes, current + this.difference(day)));
  }

  unfix(day: WorkDayTimeline, task: TaskWorkedInfo): void {
    this.adjustments.delete(day, task);
  }

  async onTaskClick(taskKey: string): Promise<void> {
    this.selectedTaskKey = taskKey;
    this.isModalOpen = true;
    this.statusHistory = [];
    this.earliestDate = null;
    this.latestDate = null;
    this.historyError = "";
    this.fetchingStatusHistory = true;

    try {
      const response = await this.taskService.getStatusHistory(taskKey);
      if (!this.isShowingHistory(taskKey)) return;
      this.statusHistory = response.entries;
      this.earliestDate = response.earliestDate;
      this.latestDate = response.latestDate;
    } catch (err) {
      error(`failed to get status history ${taskKey}: ${err}`);
      if (!this.isShowingHistory(taskKey)) return;
      this.historyError = "Não foi possível carregar o histórico de status.";
    } finally {
      if (this.isShowingHistory(taskKey)) this.fetchingStatusHistory = false;
    }
  }

  async syncStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey;
    if (!key) return;
    try {
      await this.taskService.getStatusHistory(key, true);
    } catch (err) {
      error(`failed to sync status history ${key}: ${err}`);
      if (this.isShowingHistory(key)) this.historyError = "Não foi possível sincronizar o histórico de status.";
    }
  }

  private async reloadStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey;
    if (!key) return;

    try {
      const response = await this.taskService.getCachedStatusHistory(key);
      if (!this.isShowingHistory(key)) return;
      this.statusHistory = response.entries;
      this.earliestDate = response.earliestDate;
      this.latestDate = response.latestDate;
      this.historyError = "";
    } catch (err) {
      error(`failed to reload status history ${key}: ${err}`);
    }
  }

  private isShowingHistory(key: string): boolean {
    return this.isModalOpen && this.selectedTaskKey === key;
  }

  closeModal(): void {
    this.isModalOpen = false;
    this.selectedTaskKey = "";
    this.statusHistory = [];
    this.earliestDate = null;
    this.latestDate = null;
    this.historyError = "";
    this.fetchingStatusHistory = false;
  }

  get dayWorklogPreviewDay(): WorkDayTimeline | null {
    return this.timeline.find((d) => d.date === this.dayWorklogPreviewDate) ?? null;
  }

  get dayWorklogPreviewDrafts(): WorklogDraft[] {
    return this.drafts.get(this.dayWorklogPreviewDate) ?? NO_DRAFTS;
  }

  draftCount(day: WorkDayTimeline, state: WorklogDraft["state"]): number {
    return (this.drafts.get(day.date) ?? []).filter((d) => d.state === state).length;
  }

  hasOpenDrafts(day: WorkDayTimeline): boolean {
    return (this.drafts.get(day.date) ?? []).some((d) => d.state !== "sent");
  }

  showDayWorklog(day: WorkDayTimeline): void {
    if (!this.hasOpenDrafts(day)) {
      const tasks = day.tasksWorked.filter((task) => this.finalMinutes(day, task) > 0);
      if (!tasks.length) {
        this.toastService.show("Nenhuma issue com tempo contabilizado neste dia.", "info");
        return;
      }

      const durations = tasks.map((task) => this.finalMinutes(day, task));
      const starts = this.sequentialStarts(day.punchIntervals, durations);
      const drafts: WorklogDraft[] = tasks.map((task, i) => ({
        id: this.draftStore.nextId(),
        date: day.date,
        issueId: task.issueId,
        issueKey: task.key,
        issueSummary: task.summary,
        start: starts[i],
        duration: durations[i],
        durationRaw: `${Math.floor(durations[i] / 60)}:${String(durations[i] % 60).padStart(2, "0")}`,
        comment: "",
        state: "pending",
        error: null,
      }));
      this.drafts.set(day.date, drafts);
      this.generatedDrafts.set(day.date, draftSnapshot(drafts));
    }

    this.dayWorklogPreviewDate = day.date;
    this.showDayWorklogPreview = true;
  }

  closeDayWorklogPreview(): void {
    const date = this.dayWorklogPreviewDate;
    const drafts = this.drafts.get(date) ?? [];
    if (drafts.every((d) => d.state === "sent") || draftSnapshot(drafts) === this.generatedDrafts.get(date)) {
      this.drafts.delete(date);
      this.generatedDrafts.delete(date);
    }
    this.showDayWorklogPreview = false;
    this.dayWorklogPreviewDate = "";
  }

  async onDayWorklogSent(): Promise<void> {
    const date = this.dayWorklogPreviewDate;
    if ((this.drafts.get(date) ?? []).every((d) => d.state === "sent")) {
      this.adjustments.clearDay(date);
    }
    await this.loadTimeline();
  }

  private sequentialStarts(intervals: MinuteRange[], durations: number[]): number[] {
    let i = 0;
    let cur = intervals.length ? intervals[0].start : 480;
    const starts: number[] = [];
    for (const duration of durations) {
      while (i < intervals.length && cur >= intervals[i].end) {
        i++;
        if (i < intervals.length) cur = Math.max(cur, intervals[i].start);
      }
      starts.push(cur);
      let rem = duration;
      while (rem > 0) {
        if (i >= intervals.length) {
          cur += rem;
          break;
        }
        const avail = intervals[i].end - cur;
        if (rem <= avail) {
          cur += rem;
          rem = 0;
        } else {
          rem -= avail;
          i++;
          cur = i < intervals.length ? intervals[i].start : intervals[i - 1].end;
        }
      }
    }
    return starts;
  }
}
