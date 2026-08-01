import { Component, NgZone, OnDestroy, OnInit, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import { MinuteRange, PontoMaisService, TaskWorkedInfo, WorkDayTimeline } from "../../services/pontomais.service";
import { WorklogDraft } from "../../models/worklog";
import { IssueChange } from "../../core/models";
import { StatusHistoryModalComponent } from "../../components/status-history-modal/status-history-modal.component";
import { DayWorklogPreviewModalComponent } from "../../components/day-worklog-preview-modal/day-worklog-preview-modal.component";
import { TaskService } from "../../services/task.service";
import { ToastService } from "../../core/feedback/toast.service";
import { ToastComponent } from "../../shared/ui/toast/toast.component";
import { DateRangePickerComponent } from "../../shared/ui/date-range-picker/date-range-picker.component";
import { DateRange, localTimeZone, todayIn, weekday } from "../../shared/date/local-date";
import { fromRangePreference, toRangePreference, WORKLOG_GENERATE_PRESETS } from "../../shared/date/range-presets";
import { connectionCommands, preferencesCommands, worklogsCommands } from "../../core/tauri";
import { rangeSyncKey, SyncStateService } from "../../services/sync-state.service";
import { PontoMaisStateService } from "../../services/pontomais-state.service";

const MIN_AXIS_START = 7 * 60;
const MIN_AXIS_END = 19 * 60;
const WEEKDAYS = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];
const NO_DRAFTS: WorklogDraft[] = [];
const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

@Component({
  selector: "app-worklog-generate",
  imports: [
    CommonModule,
    FormsModule,
    RouterModule,
    StatusHistoryModalComponent,
    DayWorklogPreviewModalComponent,
    ToastComponent,
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

  timeZone = localTimeZone();
  range: DateRange = WORKLOG_GENERATE_PRESETS[1].resolve(todayIn(this.timeZone));

  importingPonto = false;
  loadingTimeline = false;
  hasPontoMais = false;
  timeline: WorkDayTimeline[] = [];

  isModalOpen = false;
  selectedTaskKey = "";
  statusHistory: IssueChange[] = [];
  earliestDate: string | null = null;
  latestDate: string | null = null;
  fetchingStatusHistory = false;

  showDayWorklogPreview = false;
  dayWorklogPreviewDate = "";

  private adjustments = new Map<string, number>();
  private drafts = new Map<string, WorklogDraft[]>();
  private draftSeq = 0;
  private unlistenStatusHistorySynced?: () => void;
  private unlistenStatusHistorySyncFailed?: () => void;
  private unlistenSyncFinished?: () => void;
  private readonly syncState = inject(SyncStateService);
  private readonly pontoMaisState = inject(PontoMaisStateService);

  constructor(
    private pontoMaisService: PontoMaisService,
    private taskService: TaskService,
    private ngZone: NgZone,
    private toastService: ToastService,
  ) {
    this.updateAxis();
  }

  get syncing(): boolean {
    return this.syncState.isRangeSyncing("period", this.range);
  }

  get loadingStatusHistory(): boolean {
    return !this.statusHistory.length && (this.fetchingStatusHistory || this.statusHistorySyncing);
  }

  get syncingStatusHistoryInBackground(): boolean {
    return this.statusHistory.length > 0 && this.statusHistorySyncing;
  }

  private get statusHistorySyncing(): boolean {
    return this.isModalOpen && this.syncState.isChangelogSyncing(this.selectedTaskKey);
  }

  async ngOnInit(): Promise<void> {
    await this.setupEventListeners();
    await this.loadSettings();
    await this.loadTimeline();
    await this.syncPeriod(false);
  }

  ngOnDestroy(): void {
    this.unlistenStatusHistorySynced?.();
    this.unlistenStatusHistorySyncFailed?.();
    this.unlistenSyncFinished?.();
  }

  private async setupEventListeners(): Promise<void> {
    this.unlistenSyncFinished = this.syncState.onFinished((key) => {
      if (key === rangeSyncKey("period", this.range)) {
        this.loadTimeline();
      }
    });

    this.unlistenStatusHistorySynced = await this.taskService.onStatusHistorySynced((issueKey: string) => {
      this.ngZone.run(async () => {
        if (this.isModalOpen && this.selectedTaskKey === issueKey) {
          await this.reloadStatusHistory();
        }
      });
    });

    this.unlistenStatusHistorySyncFailed = await this.taskService.onStatusHistorySyncFailed(
      (payload: { issue_key: string; reason: string }) => {
        this.ngZone.run(() => {
          if (this.isModalOpen && this.selectedTaskKey === payload.issue_key) {
            if (!this.statusHistory.length) {
              this.toastService.error("Não foi possível carregar o histórico de status.");
            }
          }
        });
      },
    );
  }

  private async loadSettings(): Promise<void> {
    try {
      this.timeZone = (await connectionCommands.getJiraTimeZone()) || localTimeZone();
    } catch (err) {
      error(`failed to load Jira time zone: ${err}`);
    }

    await this.refreshPontoMaisConnection();

    try {
      const preferences = await preferencesCommands.getPreferences();
      this.range = fromRangePreference(preferences.worklogGenerateRange, todayIn(this.timeZone), "week", this.presets);
    } catch (err) {
      error(`failed to load worklog generate range: ${err}`);
    }
  }

  async applyRange(range: DateRange): Promise<void> {
    this.range = range;
    this.adjustments.clear();
    preferencesCommands
      .updateRangePreference("worklogGenerate", toRangePreference(range, todayIn(this.timeZone), this.presets))
      .catch((err) => error(`failed to save worklog generate range: ${err}`));
    await this.loadTimeline();
    await this.syncPeriod(false);
  }

  async loadTimeline(): Promise<void> {
    this.loadingTimeline = !this.timeline.length;
    try {
      this.timeline = await this.pontoMaisService.getPontoMaisTimeline(this.range.from, this.range.to);
      this.updateAxis();
    } catch (err) {
      error(`failed to load worklog timeline: ${err}`);
      this.toastService.error("Não foi possível montar a linha do tempo do período.");
    } finally {
      this.loadingTimeline = false;
    }
  }

  async syncPeriod(notify = true): Promise<void> {
    if (this.syncing) return;
    try {
      const summary = await worklogsCommands.syncWorklogPeriod(this.range.from, this.range.to, notify);
      if (summary && notify) {
        const ponto =
          summary.pontoEntries === null ? "ponto não conectado" : `${summary.pontoEntries} marcações de ponto`;
        this.toastService.success(`Sincronizado: ${ponto}, ${summary.issues} issues e ${summary.worklogs} worklogs.`);
      }
    } catch (err) {
      error(`failed to sync worklog period: ${err}`);
      await this.refreshPontoMaisConnection();
      if (notify || !String(err).includes("nenhuma credencial")) {
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
    this.hasPontoMais = this.pontoMaisState.state().connected;
  }

  trackDay(_: number, day: WorkDayTimeline): string {
    return day.date;
  }

  trackTask(_: number, task: TaskWorkedInfo): string {
    return task.issue_id;
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
      ...day.punch_intervals,
      ...day.tasks_worked.flatMap((task) => task.counted_segments),
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
    if (!task.dev_segments.length) return "";
    return `${this.clock(task.dev_segments[0].start)}–${this.clock(task.dev_segments[task.dev_segments.length - 1].end)}`;
  }

  adjustment(day: WorkDayTimeline, task: TaskWorkedInfo): number {
    return this.adjustments.get(`${day.date}|${task.issue_id}`) ?? 0;
  }

  finalMinutes(day: WorkDayTimeline, task: TaskWorkedInfo): number {
    return Math.max(0, task.worked_minutes + this.adjustment(day, task));
  }

  calculated(day: WorkDayTimeline): number {
    return day.tasks_worked.reduce((sum, task) => sum + this.finalMinutes(day, task), 0);
  }

  difference(day: WorkDayTimeline): number {
    return day.total_minutes - this.calculated(day);
  }

  canFix(day: WorkDayTimeline): boolean {
    return this.difference(day) !== 0 && !day.open && day.gama_synced_count === 0 && day.total_minutes > 0;
  }

  fix(day: WorkDayTimeline, task: TaskWorkedInfo): void {
    const key = `${day.date}|${task.issue_id}`;
    const diff = this.difference(day);
    const current = this.adjustments.get(key) ?? 0;
    this.adjustments.set(key, Math.max(1 - task.worked_minutes, current + diff));
  }

  unfix(day: WorkDayTimeline, task: TaskWorkedInfo): void {
    this.adjustments.delete(`${day.date}|${task.issue_id}`);
  }

  async onTaskClick(taskKey: string): Promise<void> {
    this.selectedTaskKey = taskKey;
    this.isModalOpen = true;
    this.statusHistory = [];
    this.earliestDate = null;
    this.latestDate = null;
    this.fetchingStatusHistory = true;

    try {
      const response = await this.taskService.getStatusHistory(taskKey);
      this.statusHistory = response.entries;
      this.earliestDate = response.earliestDate;
      this.latestDate = response.latestDate;
    } catch (err) {
      error(`failed to get status history ${taskKey}: ${err}`);
      this.toastService.error("Não foi possível carregar o histórico de status.");
    } finally {
      this.fetchingStatusHistory = false;
    }
  }

  private async reloadStatusHistory(): Promise<void> {
    if (!this.selectedTaskKey) return;

    try {
      const response = await this.taskService.getCachedStatusHistory(this.selectedTaskKey);
      this.statusHistory = response.entries;
      this.earliestDate = response.earliestDate;
      this.latestDate = response.latestDate;
    } catch (err) {
      error(`failed to reload status history ${this.selectedTaskKey}: ${err}`);
    }
  }

  closeModal(): void {
    this.isModalOpen = false;
    this.selectedTaskKey = "";
    this.statusHistory = [];
    this.earliestDate = null;
    this.latestDate = null;
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
      const tasks = day.tasks_worked.filter((task) => this.finalMinutes(day, task) > 0);
      if (!tasks.length) {
        this.toastService.show("Nenhuma issue com tempo contado neste dia.", "info");
        return;
      }

      const durations = tasks.map((task) => this.finalMinutes(day, task));
      const starts = this.sequentialStarts(day.punch_intervals, durations);
      this.drafts.set(
        day.date,
        tasks.map((task, i) => ({
          id: ++this.draftSeq,
          date: day.date,
          issueId: task.issue_id,
          issueKey: task.key,
          issueSummary: task.summary,
          start: starts[i],
          duration: durations[i],
          durationRaw: `${Math.floor(durations[i] / 60)}:${String(durations[i] % 60).padStart(2, "0")}`,
          comment: "",
          state: "pending",
          error: null,
        })),
      );
    }

    this.dayWorklogPreviewDate = day.date;
    this.showDayWorklogPreview = true;
  }

  closeDayWorklogPreview(): void {
    const date = this.dayWorklogPreviewDate;
    const drafts = this.drafts.get(date) ?? [];
    if (drafts.every((d) => d.state === "sent")) {
      this.drafts.delete(date);
    }
    this.showDayWorklogPreview = false;
    this.dayWorklogPreviewDate = "";
  }

  async onDayWorklogSent(): Promise<void> {
    const date = this.dayWorklogPreviewDate;
    if ((this.drafts.get(date) ?? []).every((d) => d.state === "sent")) {
      [...this.adjustments.keys()].filter((k) => k.startsWith(`${date}|`)).forEach((k) => this.adjustments.delete(k));
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
