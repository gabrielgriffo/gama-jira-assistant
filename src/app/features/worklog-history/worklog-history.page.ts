import { Component, OnDestroy, OnInit, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import { openUrl } from "@tauri-apps/plugin-opener";
import { JiraWorklogService } from "../../services/jira-worklog.service";
import { PontoMaisService } from "../../services/pontomais.service";
import { JiraWorklog, JiraWorklogHistoryResponse } from "../../models/jira-worklog";
import { ToastComponent } from "../../shared/ui/toast/toast.component";
import { ToastService } from "../../core/feedback/toast.service";
import { DateRangePickerComponent } from "../../shared/ui/date-range-picker/date-range-picker.component";
import {
  addDays,
  DateRange,
  localTimeZone,
  relativeDateTime,
  todayIn,
  weekday,
  zonedDateTime,
} from "../../shared/date/local-date";
import { fromRangePreference, toRangePreference, WORKLOG_HISTORY_PRESETS } from "../../shared/date/range-presets";
import { connectionCommands, preferencesCommands } from "../../core/tauri";
import { rangeSyncKey, SyncStateService } from "../../services/sync-state.service";

const WEEKDAYS = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];
const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
const PENDING_TOLERANCE_MINUTES = 10;

interface HistoryDay {
  date: string;
  isToday: boolean;
  worklogs: JiraWorklog[];
  registeredMinutes: number;
  pontoMinutes: number;
  zeroWorklogs: number;
}

interface GridRow {
  issueKey: string;
  issueSummary: string;
  issueBrowseUrl: string;
  totalMinutes: number;
  cells: number[];
  zeroCells: number[];
}

@Component({
  selector: "app-worklog-history",
  imports: [CommonModule, FormsModule, RouterModule, ToastComponent, DateRangePickerComponent],
  templateUrl: "./worklog-history.page.html",
  styleUrl: "./worklog-history.page.css",
})
export class WorklogHistoryPage implements OnInit, OnDestroy {
  readonly presets = WORKLOG_HISTORY_PRESETS;

  timeZone = localTimeZone();
  range: DateRange = WORKLOG_HISTORY_PRESETS[0].resolve(todayIn(this.timeZone));
  loading = false;
  response: JiraWorklogHistoryResponse | null = null;
  viewMode: "grid" | "day" = "grid";

  days: HistoryDay[] = [];
  gridDays: string[] = [];
  gridRows: GridRow[] = [];
  registeredMinutes = 0;
  pontoMinutes = 0;
  pontoDays = 0;
  pendingDays = 0;
  syncedLabel = "";

  private pontoByDay = new Map<string, number>();
  private unlistenSyncFinished?: () => void;
  private readonly syncState = inject(SyncStateService);

  constructor(
    private jiraWorklogService: JiraWorklogService,
    private pontoMaisService: PontoMaisService,
    private toastService: ToastService,
  ) {}

  get syncing(): boolean {
    return this.syncState.isRangeSyncing("worklogs", this.range);
  }

  async ngOnInit(): Promise<void> {
    this.unlistenSyncFinished = this.syncState.onFinished((key) => {
      if (key === rangeSyncKey("worklogs", this.range)) {
        this.loadWorklogs();
      }
    });
    await this.loadSettings();
    await this.loadWorklogs();
    await this.syncWorklogs(false);
  }

  ngOnDestroy(): void {
    this.unlistenSyncFinished?.();
  }

  private async loadSettings(): Promise<void> {
    try {
      this.timeZone = (await connectionCommands.getJiraTimeZone()) || localTimeZone();
    } catch (err) {
      error(`failed to load Jira time zone: ${err}`);
    }

    try {
      const preferences = await preferencesCommands.getPreferences();
      this.range = fromRangePreference(preferences.worklogHistoryRange, todayIn(this.timeZone), "month", this.presets);
    } catch (err) {
      error(`failed to load worklog history range: ${err}`);
    }
  }

  async applyRange(range: DateRange): Promise<void> {
    this.range = range;
    preferencesCommands
      .updateRangePreference("worklogHistory", toRangePreference(range, todayIn(this.timeZone), this.presets))
      .catch((err) => error(`failed to save worklog history range: ${err}`));
    await this.loadWorklogs();
    await this.syncWorklogs(false);
  }

  async syncWorklogs(notify = true): Promise<void> {
    if (this.syncing) return;
    try {
      const synced = await this.jiraWorklogService.syncWorklogs(this.range.from, this.range.to, notify);
      if (synced !== null && notify) {
        this.toastService.success(`${synced} worklog(s) sincronizado(s).`);
      }
    } catch (err) {
      error(`failed to sync worklogs: ${err}`);
      if (notify || !String(err).includes("nenhuma credencial")) {
        this.toastService.error(`Não foi possível sincronizar os worklogs: ${err}`);
      }
    }
  }

  async loadWorklogs(): Promise<void> {
    this.loading = !this.response;
    try {
      const [response, timeline] = await Promise.all([
        this.jiraWorklogService.getWorklogsForPeriod(this.range.from, this.range.to),
        this.pontoMaisService.getPontoMaisTimeline(this.range.from, this.range.to).catch((err) => {
          error(`failed to load ponto for history: ${err}`);
          return [];
        }),
      ]);
      this.response = response;
      this.pontoByDay = new Map(timeline.map((day) => [day.date, day.total_minutes]));
      this.buildView(response);
    } catch (err) {
      error(`failed to load worklogs: ${err}`);
      this.toastService.error(`Não foi possível carregar os worklogs: ${err}`);
    } finally {
      this.loading = false;
    }
  }

  private buildView(response: JiraWorklogHistoryResponse): void {
    const today = todayIn(this.timeZone);
    const lastDay = this.range.to < today ? this.range.to : today;
    const worklogsByDay = new Map(response.days.map((day) => [day.date, day.worklogs]));

    const allDays: string[] = [];
    for (let d = lastDay; d >= this.range.from; d = addDays(d, -1)) allDays.push(d);

    this.days = allDays
      .map((date) => {
        const worklogs = worklogsByDay.get(date) ?? [];
        return {
          date,
          isToday: date === today,
          worklogs,
          registeredMinutes: Math.round(worklogs.reduce((sum, w) => sum + w.timeSpentSeconds, 0) / 60),
          pontoMinutes: this.pontoByDay.get(date) ?? 0,
          zeroWorklogs: worklogs.filter((w) => this.isZero(w)).length,
        };
      })
      .filter((day) => day.worklogs.length > 0 || day.pontoMinutes > 0);

    this.registeredMinutes = Math.round(response.totalSeconds / 60);
    this.pontoMinutes = [...this.pontoByDay.values()].reduce((sum, m) => sum + m, 0);
    this.pontoDays = [...this.pontoByDay.values()].filter((m) => m > 0).length;
    this.pendingDays = this.days.filter(
      (day) => day.date < today && day.pontoMinutes - day.registeredMinutes > PENDING_TOLERANCE_MINUTES,
    ).length;

    this.gridDays = allDays;
    const rows = new Map<string, GridRow>();
    for (const day of response.days) {
      const column = allDays.indexOf(day.date);
      if (column < 0) continue;
      for (const worklog of day.worklogs) {
        let row = rows.get(worklog.issueKey);
        if (!row) {
          row = {
            issueKey: worklog.issueKey,
            issueSummary: worklog.issueSummary,
            issueBrowseUrl: worklog.issueBrowseUrl,
            totalMinutes: 0,
            cells: allDays.map(() => 0),
            zeroCells: allDays.map(() => 0),
          };
          rows.set(worklog.issueKey, row);
        }
        if (this.isZero(worklog)) row.zeroCells[column] += 1;
        const minutes = worklog.timeSpentSeconds / 60;
        row.cells[column] += minutes;
        row.totalMinutes += minutes;
      }
    }
    this.gridRows = [...rows.values()].sort((a, b) => a.issueKey.localeCompare(b.issueKey, "pt-BR", { numeric: true }));

    this.syncedLabel = response.lastSyncedAt
      ? `Atualizado ${relativeDateTime(response.lastSyncedAt, this.timeZone)}`
      : "";
  }

  get difference(): number {
    return this.registeredMinutes - this.pontoMinutes;
  }

  openIssue(url: string): void {
    if (!url) return;
    openUrl(url).catch((err) => error(`failed to open issue: ${err}`));
  }

  isZero(worklog: JiraWorklog): boolean {
    return worklog.timeSpentSeconds <= 0;
  }

  trackDay(_: number, day: HistoryDay): string {
    return day.date;
  }

  trackWorklog(_: number, worklog: JiraWorklog): string {
    return worklog.id;
  }

  dayTitle(date: string): string {
    const [, m, d] = date.split("-").map(Number);
    return `${WEEKDAYS[weekday(date)]}, ${d} ${MONTHS[m - 1]}`;
  }

  dayNumber(date: string): string {
    return date.slice(8, 10);
  }

  weekdayShort(date: string): string {
    return WEEKDAYS[weekday(date)].slice(0, 3).toLowerCase();
  }

  isWeekend(date: string): boolean {
    const wd = weekday(date);
    return wd === 0 || wd === 6;
  }

  columnClass(date: string): string {
    if (this.isWeekend(date)) return "wkend";
    return date === todayIn(this.timeZone) ? "today-col" : "";
  }

  isPending(day: HistoryDay): boolean {
    return day.date < todayIn(this.timeZone) && day.pontoMinutes - day.registeredMinutes > PENDING_TOLERANCE_MINUTES;
  }

  columnRegistered(index: number): number {
    return this.gridRows.reduce((sum, row) => sum + row.cells[index], 0);
  }

  columnPonto(date: string): number {
    return this.pontoByDay.get(date) ?? 0;
  }

  columnDiff(index: number, date: string): number {
    return this.columnRegistered(index) - this.columnPonto(date);
  }

  isMissing(index: number, date: string): boolean {
    return (
      date < todayIn(this.timeZone) &&
      this.columnPonto(date) > 0 &&
      this.columnDiff(index, date) < -PENDING_TOLERANCE_MINUTES
    );
  }

  cellAlpha(minutes: number): string {
    return `${Math.min(22, 6 + Math.round((minutes / 480) * 16))}%`;
  }

  hfmt(minutes: number): string {
    const sign = minutes < 0 ? "-" : "";
    const m = Math.abs(Math.round(minutes));
    return `${sign}${Math.floor(m / 60)}h${String(m % 60).padStart(2, "0")}`;
  }

  gfmt(minutes: number): string {
    const m = Math.abs(Math.round(minutes));
    return `${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}`;
  }

  signedGfmt(minutes: number): string {
    const rounded = Math.round(minutes);
    if (rounded === 0) return "0:00";
    return `${rounded > 0 ? "+" : "-"}${this.gfmt(rounded)}`;
  }

  timeRange(worklog: JiraWorklog): string {
    const start = zonedDateTime(worklog.started, this.timeZone);
    const end = zonedDateTime(
      new Date(new Date(worklog.started).getTime() + worklog.timeSpentSeconds * 1000).toISOString(),
      this.timeZone,
    );
    if (!start || !end) return "";
    return `${start.time}–${end.time}`;
  }
}
