import {
  ChangeDetectionStrategy,
  Component,
  OnDestroy,
  OnInit,
  computed,
  effect,
  inject,
  signal,
  untracked,
} from "@angular/core";
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
import { chronologicalOrder } from "./task-order";
import { IssueChange } from "../../../../core/models";
import { StatusHistoryModalComponent } from "../../../../shared/components/status-history-modal/status-history-modal.component";
import { DayWorklogPreviewModalComponent } from "../../dialogs/day-worklog-preview-modal/day-worklog-preview-modal.component";
import { ToastService } from "../../../../core/feedback/toast.service";
import { DateRangePickerComponent } from "../../../../shared/ui/date-range-picker/date-range-picker.component";
import { sameDates, weekday } from "../../../../shared/date/local-date";
import { RangeSelection, WORKLOG_GENERATE_PRESETS } from "../../../../shared/date/range-presets";
import { changelogCommands, isMissingCredentials, worklogsCommands } from "../../../../core/tauri";
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
  imports: [IconComponent, StatusHistoryModalComponent, DayWorklogPreviewModalComponent, DateRangePickerComponent],
  templateUrl: "./worklog-generate.page.html",
  styleUrl: "./worklog-generate.page.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WorklogGeneratePage implements OnInit, OnDestroy {
  private pontoMaisService = inject(PontoMaisService);
  private toastService = inject(ToastService);
  private readonly settings = inject(AppSettingsService);
  private readonly syncState = inject(SyncStateService);
  private readonly pontoMaisState = inject(PontoMaisStateService);
  private readonly draftStore = inject(WorklogDraftStore);
  private readonly drafts = this.draftStore.drafts;
  private readonly generatedDrafts = this.draftStore.generated;

  readonly presets = WORKLOG_GENERATE_PRESETS;
  readonly timeZone = computed(() => this.settings.timeZone);
  readonly today = this.settings.today;
  readonly range = signal<RangeSelection>(this.settings.range("worklogGenerate", "week", this.presets));

  readonly importingPonto = signal(false);
  readonly timelineLoaded = signal(false);
  readonly loadError = signal("");
  readonly timeline = signal<WorkDayTimeline[]>([]);
  readonly axis = computed(() => buildAxis(this.timeline()));

  readonly isModalOpen = signal(false);
  readonly selectedTaskKey = signal("");
  readonly statusHistory = signal<IssueChange[]>([]);
  readonly earliestDate = signal<string | null>(null);
  readonly latestDate = signal<string | null>(null);
  readonly loadingStatusHistory = signal(false);
  readonly historyError = signal("");

  readonly showDayWorklogPreview = signal(false);
  readonly dayWorklogPreviewDate = signal("");

  readonly hasPontoMais = computed(() => this.pontoMaisState.state().connected);
  readonly syncing = computed(() => this.syncState.isRangeSyncing("period", this.range()));
  readonly syncingStatusHistory = computed(
    () => this.isModalOpen() && this.syncState.isChangelogSyncing(this.selectedTaskKey()),
  );
  readonly emptyHint = computed(() =>
    this.hasPontoMais()
      ? "Utilize Importar ponto para obter as marcações."
      : "Conecte a conta do PontoMais em Configurações para importar as marcações.",
  );
  readonly dayWorklogPreviewDay = computed(
    () => this.timeline().find((d) => d.date === this.dayWorklogPreviewDate()) ?? null,
  );
  readonly dayWorklogPreviewDrafts = computed(() => this.drafts.get(this.dayWorklogPreviewDate()) ?? NO_DRAFTS);

  private readonly adjustments = new Adjustments();
  private readonly listeners = new ListenerBag();
  private readonly timelineLoads = new LatestOnly();
  private loadedRange: RangeSelection | null = null;
  private readonly adjustmentsRevision = signal(0);

  constructor() {
    const localData = inject(LocalDataService);
    const sessionChanged = changeTracker<string>();
    effect(() => {
      if (!sessionChanged(localData.sessionKey())) return;
      untracked(() => this.resetForNewSession());
    });

    const statusCatalog = inject(StatusCatalogStore);
    const catalogChanged = changeTracker<unknown>();
    effect(() => {
      if (catalogChanged(statusCatalog.statuses())) untracked(() => this.loadTimeline());
    });

    const pontoChanged = changeTracker<string>();
    effect(() => {
      const ponto = this.pontoMaisState.state();
      if (!pontoChanged(`${ponto.connected}|${ponto.account ?? ""}`)) return;
      untracked(() => {
        this.loadTimeline();
        if (ponto.connected) this.syncPeriod(false);
      });
    });
  }

  private resetForNewSession(): void {
    this.timelineLoads.begin();
    this.timeline.set([]);
    this.loadedRange = null;
    this.loadError.set("");
    this.adjustments.clear();
    this.bumpAdjustments();
    this.draftStore.clear();
    this.showDayWorklogPreview.set(false);
    this.dayWorklogPreviewDate.set("");
    this.closeModal();
    this.loadTimeline();
    this.syncPeriod(false);
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
          if (key === rangeSyncKey("period", this.range())) {
            this.loadTimeline();
          }
        }),
      ),
    );

    await this.listeners.add(
      changelogCommands.onStatusHistorySynced((issueKey: string) => {
        if (this.isShowingHistory(issueKey)) this.reloadStatusHistory();
      }),
    );

    await this.listeners.add(
      changelogCommands.onStatusHistorySyncFailed((payload: { issueKey: string; reason: string }) => {
        if (this.isShowingHistory(payload.issueKey) && !this.statusHistory().length) {
          this.historyError.set("Não foi possível carregar o histórico de status.");
        }
      }),
    );
  }

  async applyRange(range: RangeSelection): Promise<void> {
    this.range.set(range);
    this.adjustments.clear();
    this.bumpAdjustments();
    this.settings.saveRange("worklogGenerate", range, this.presets);
    await this.loadTimeline();
    await this.syncPeriod(false);
  }

  async loadTimeline(): Promise<void> {
    const range = this.range();
    const isCurrent = this.timelineLoads.begin();
    try {
      const timeline = daysWithPunches(await this.pontoMaisService.getPontoMaisTimeline(range.from, range.to));
      if (!isCurrent()) return;
      this.timeline.set(timeline);
      this.loadedRange = range;
      this.loadError.set("");
    } catch (err) {
      if (!isCurrent()) return;
      error(`failed to load worklog timeline: ${err}`);
      this.toastService.error("Não foi possível carregar a linha do tempo do período.");
      if (!sameDates(this.loadedRange, range)) {
        this.timeline.set([]);
        this.loadError.set("Não foi possível carregar a linha do tempo do período selecionado.");
      }
    } finally {
      if (isCurrent()) this.timelineLoaded.set(true);
    }
  }

  async syncPeriod(notify = true): Promise<void> {
    if (this.syncing()) return;
    try {
      const summary = await worklogsCommands.syncWorklogPeriod(this.range().from, this.range().to, notify);
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
    this.importingPonto.set(true);
    try {
      const total = await this.pontoMaisService.syncPontoMaisEntries(this.range().from, this.range().to);
      await this.loadTimeline();
      this.toastService.success(`${total} marcações de ponto importadas.`);
    } catch (err) {
      error(`failed to import PontoMais entries: ${err}`);
      await this.refreshPontoMaisConnection();
      this.toastService.error(`Não foi possível importar o ponto: ${err}`);
    } finally {
      this.importingPonto.set(false);
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
    return axisPct(this.axis(), minutes);
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
    this.adjustmentsRevision();
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
    return (
      this.difference(day) !== 0 &&
      !day.open &&
      day.gamaSyncedCount === 0 &&
      day.totalMinutes > 0 &&
      !this.hasOpenDrafts(day)
    );
  }

  fix(day: WorkDayTimeline, task: TaskWorkedInfo): void {
    const current = this.adjustments.get(day, task);
    this.adjustments.set(day, task, Math.max(1 - task.workedMinutes, current + this.difference(day)));
    this.bumpAdjustments();
  }

  unfix(day: WorkDayTimeline, task: TaskWorkedInfo): void {
    this.adjustments.delete(day, task);
    this.bumpAdjustments();
  }

  private bumpAdjustments(): void {
    this.adjustmentsRevision.update((n) => n + 1);
  }

  async onTaskClick(taskKey: string): Promise<void> {
    this.selectedTaskKey.set(taskKey);
    this.isModalOpen.set(true);
    this.statusHistory.set([]);
    this.earliestDate.set(null);
    this.latestDate.set(null);
    this.historyError.set("");
    this.loadingStatusHistory.set(true);

    try {
      const response = await changelogCommands.getStatusHistory(taskKey);
      if (!this.isShowingHistory(taskKey)) return;
      this.statusHistory.set(response.entries);
      this.earliestDate.set(response.earliestDate);
      this.latestDate.set(response.latestDate);
    } catch (err) {
      error(`failed to get status history ${taskKey}: ${err}`);
      if (!this.isShowingHistory(taskKey)) return;
      this.historyError.set("Não foi possível carregar o histórico de status.");
    } finally {
      if (this.isShowingHistory(taskKey)) this.loadingStatusHistory.set(false);
    }
  }

  async syncStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey();
    if (!key) return;
    try {
      await changelogCommands.getStatusHistory(key, true);
    } catch (err) {
      error(`failed to sync status history ${key}: ${err}`);
      if (this.isShowingHistory(key)) this.historyError.set("Não foi possível sincronizar o histórico de status.");
    }
  }

  private async reloadStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey();
    if (!key) return;

    try {
      const response = await changelogCommands.getCachedStatusHistory(key);
      if (!this.isShowingHistory(key)) return;
      this.statusHistory.set(response.entries);
      this.earliestDate.set(response.earliestDate);
      this.latestDate.set(response.latestDate);
      this.historyError.set("");
    } catch (err) {
      error(`failed to reload status history ${key}: ${err}`);
    }
  }

  private isShowingHistory(key: string): boolean {
    return this.isModalOpen() && this.selectedTaskKey() === key;
  }

  closeModal(): void {
    this.isModalOpen.set(false);
    this.selectedTaskKey.set("");
    this.statusHistory.set([]);
    this.earliestDate.set(null);
    this.latestDate.set(null);
    this.historyError.set("");
    this.loadingStatusHistory.set(false);
  }

  draftCount(day: WorkDayTimeline, state: WorklogDraft["state"]): number {
    return (this.drafts.get(day.date) ?? []).filter((d) => d.state === state).length;
  }

  hasOpenDrafts(day: WorkDayTimeline): boolean {
    return (this.drafts.get(day.date) ?? []).some((d) => d.state !== "sent");
  }

  showDayWorklog(day: WorkDayTimeline): void {
    if (!this.hasOpenDrafts(day)) {
      const tasks = chronologicalOrder(day.tasksWorked.filter((task) => this.finalMinutes(day, task) > 0));
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

    this.dayWorklogPreviewDate.set(day.date);
    this.showDayWorklogPreview.set(true);
  }

  closeDayWorklogPreview(): void {
    const date = this.dayWorklogPreviewDate();
    const drafts = this.drafts.get(date) ?? [];
    if (drafts.every((d) => d.state === "sent") || draftSnapshot(drafts) === this.generatedDrafts.get(date)) {
      this.drafts.delete(date);
      this.generatedDrafts.delete(date);
    }
    this.showDayWorklogPreview.set(false);
    this.dayWorklogPreviewDate.set("");
  }

  async onDayWorklogSent(): Promise<void> {
    const date = this.dayWorklogPreviewDate();
    if ((this.drafts.get(date) ?? []).every((d) => d.state === "sent")) {
      this.adjustments.clearDay(date);
      this.bumpAdjustments();
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

interface Axis {
  start: number;
  end: number;
  ticks: { label: string; left: string }[];
  guides: string[];
}

function axisPct(axis: Pick<Axis, "start" | "end">, minutes: number): string {
  const clamped = Math.min(Math.max(minutes, axis.start), axis.end);
  return `${(((clamped - axis.start) / (axis.end - axis.start)) * 100).toFixed(3)}%`;
}

function buildAxis(timeline: WorkDayTimeline[]): Axis {
  const ranges = timeline.flatMap((day) => [
    ...day.punchIntervals,
    ...day.tasksWorked.flatMap((task) => task.countedSegments),
  ]);
  const earliest = Math.min(MIN_AXIS_START, ...ranges.map((r) => r.start));
  const latest = Math.max(MIN_AXIS_END, ...ranges.map((r) => r.end));
  const bounds = {
    start: Math.max(0, Math.floor(earliest / 60) * 60),
    end: Math.min(1440, Math.ceil(latest / 60) * 60),
  };

  const first = bounds.start / 60;
  const last = bounds.end / 60;
  const hours: number[] = [];
  for (let h = first; h <= last; h++) {
    if (h === first || h === last || h % 2 === 1) hours.push(h);
  }
  return {
    ...bounds,
    ticks: hours.map((h) => ({ label: `${String(h).padStart(2, "0")}h`, left: axisPct(bounds, h * 60) })),
    guides: hours.slice(1, -1).map((h) => axisPct(bounds, h * 60)),
  };
}
