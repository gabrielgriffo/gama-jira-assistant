import { Component, OnDestroy, OnInit, effect, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import { openUrl } from "@tauri-apps/plugin-opener";
import { JiraWorklogService } from "../../data-access/jira-worklog.service";
import { PontoMaisService } from "../../../../core/pontomais/pontomais.service";
import { JiraWorklog, JiraWorklogHistoryResponse } from "../../data-access/jira-worklog";
import { ToastService } from "../../../../core/feedback/toast.service";
import { DateRangePickerComponent } from "../../../../shared/ui/date-range-picker/date-range-picker.component";
import { addDays, relativeDateTime, todayIn, weekday, zonedDateTime } from "../../../../shared/date/local-date";
import { RangeSelection, WORKLOG_HISTORY_PRESETS } from "../../../../shared/date/range-presets";
import { AppSettingsService } from "../../../../core/preferences/app-settings.service";
import { rangeSyncKey, SyncStateService } from "../../../../core/sync/sync-state.service";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { monthGroups, type MonthGroup } from "./month-groups";
import { sortByLastChange } from "../../../../shared/sort/last-change";
import { isMissingCredentials } from "../../../../core/tauri";
import { LatestOnly } from "../../../../shared/async/latest-only";
import { changeTracker } from "../../../../shared/async/change-tracker";
import { LocalDataService } from "../../../../core/sync/local-data.service";
import {
  differenceBeforeToday,
  GridColumn,
  gridColumns,
  isWeekendDate,
  PENDING_TOLERANCE_MINUTES,
} from "./grid-columns";

const WEEKDAYS = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];
const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

interface HistoryDay {
  date: string;
  isToday: boolean;
  worklogs: JiraWorklog[];
  registeredMinutes: number;
  pontoMinutes: number;
  zeroWorklogs: number;
  pending: boolean;
}

const LOADING_DELAY_MS = 300;

interface GridRow {
  issueKey: string;
  issueSummary: string;
  issueBrowseUrl: string;
  lastChangeAt: string | null;
  totalMinutes: number;
  cells: number[];
  zeroCells: number[];
}

@Component({
  selector: "app-worklog-history",
  imports: [IconComponent, CommonModule, FormsModule, RouterModule, DateRangePickerComponent],
  templateUrl: "./worklog-history.page.html",
  styleUrl: "./worklog-history.page.css",
})
export class WorklogHistoryPage implements OnInit, OnDestroy {
  readonly presets = WORKLOG_HISTORY_PRESETS;

  private readonly settings = inject(AppSettingsService);
  get timeZone(): string {
    return this.settings.timeZone;
  }
  range: RangeSelection = this.settings.range("worklogHistory", "month", this.presets);
  loading = false;
  response: JiraWorklogHistoryResponse | null = null;
  viewMode: "grid" | "day" = "grid";

  days: HistoryDay[] = [];
  gridDays: string[] = [];
  gridMonths: MonthGroup[] = [];
  gridRows: GridRow[] = [];
  registeredMinutes = 0;
  pontoMinutes = 0;
  pontoDays = 0;
  pendingDays = 0;
  difference = 0;
  gridColumns: GridColumn[] = [];
  syncedLabel = "";
  loadError = "";
  pontoError = false;
  differenceLabel = "Diferença";

  private pontoByDay = new Map<string, number>();
  private readonly loads = new LatestOnly();
  private loadedRange: RangeSelection | null = null;
  private unlistenSyncFinished?: () => void;
  private readonly syncState = inject(SyncStateService);

  constructor(
    private jiraWorklogService: JiraWorklogService,
    private pontoMaisService: PontoMaisService,
    private toastService: ToastService,
  ) {
    const localData = inject(LocalDataService);
    const sessionChanged = changeTracker<string>();
    effect(() => {
      if (!sessionChanged(localData.sessionKey())) return;
      this.loads.begin();
      this.response = null;
      this.days = [];
      this.gridRows = [];
      this.gridColumns = [];
      this.loadedRange = null;
      this.loadError = "";
      this.loadWorklogs();
      this.syncWorklogs(false);
    });
  }

  get syncing(): boolean {
    return this.syncState.isRangeSyncing("worklogs", this.range);
  }

  async ngOnInit(): Promise<void> {
    this.unlistenSyncFinished = this.syncState.onFinished((key) => {
      if (key === rangeSyncKey("worklogs", this.range)) {
        this.loadWorklogs();
      }
    });
    await this.loadWorklogs();
    await this.syncWorklogs(false);
  }

  ngOnDestroy(): void {
    this.unlistenSyncFinished?.();
  }

  async applyRange(range: RangeSelection): Promise<void> {
    this.range = range;
    this.settings.saveRange("worklogHistory", range, this.presets);
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
      if (notify || !isMissingCredentials(err)) {
        this.toastService.error(`Não foi possível sincronizar os worklogs: ${err}`);
      }
    }
  }

  async loadWorklogs(): Promise<void> {
    const range = this.range;
    const isCurrent = this.loads.begin();
    const showLoading = this.response ? undefined : setTimeout(() => (this.loading = true), LOADING_DELAY_MS);
    let pontoFailed = false;
    try {
      const [response, timeline] = await Promise.all([
        this.jiraWorklogService.getWorklogsForPeriod(range.from, range.to),
        this.pontoMaisService.getPontoMaisTimeline(range.from, range.to).catch((err) => {
          error(`failed to load ponto for history: ${err}`);
          pontoFailed = true;
          return [];
        }),
      ]);
      if (!isCurrent()) return;
      if (pontoFailed && !this.pontoError) {
        this.toastService.error("Não foi possível carregar as marcações de ponto do período.");
      }
      this.pontoError = pontoFailed;
      this.response = response;
      this.loadError = "";
      this.loadedRange = range;
      this.pontoByDay = new Map(timeline.map((day) => [day.date, day.totalMinutes]));
      this.buildView(response, range);
    } catch (err) {
      if (!isCurrent()) return;
      error(`failed to load worklogs: ${err}`);
      this.toastService.error(`Não foi possível carregar os worklogs: ${err}`);
      if (this.loadedRange !== range) {
        this.response = null;
        this.days = [];
        this.gridRows = [];
        this.gridColumns = [];
        this.loadError = "Não foi possível carregar os worklogs do período.";
      }
    } finally {
      clearTimeout(showLoading);
      if (isCurrent()) this.loading = false;
    }
  }

  private buildView(response: JiraWorklogHistoryResponse, range: RangeSelection): void {
    const today = todayIn(this.timeZone);
    const lastDay = range.to < today ? range.to : today;
    const worklogsByDay = new Map(response.days.map((day) => [day.date, day.worklogs]));

    const allDays: string[] = [];
    for (let d = lastDay; d >= range.from; d = addDays(d, -1)) allDays.push(d);

    this.days = allDays
      .map((date) => {
        const worklogs = worklogsByDay.get(date) ?? [];
        const registeredMinutes = Math.round(worklogs.reduce((sum, w) => sum + w.timeSpentSeconds, 0) / 60);
        const pontoMinutes = this.pontoByDay.get(date) ?? 0;
        return {
          date,
          isToday: date === today,
          worklogs,
          registeredMinutes,
          pontoMinutes,
          zeroWorklogs: worklogs.filter((w) => this.isZero(w)).length,
          pending: date < today && pontoMinutes - registeredMinutes > PENDING_TOLERANCE_MINUTES,
        };
      })
      .filter((day) => day.worklogs.length > 0 || day.pontoMinutes > 0);

    this.registeredMinutes = Math.round(response.totalSeconds / 60);
    this.pontoMinutes = [...this.pontoByDay.values()].reduce((sum, m) => sum + m, 0);
    this.pontoDays = [...this.pontoByDay.values()].filter((m) => m > 0).length;
    this.pendingDays = this.days.filter((day) => day.pending).length;
    this.difference = differenceBeforeToday(this.days, today);
    this.differenceLabel = range.to >= today ? "Diferença até ontem" : "Diferença";

    this.gridDays = allDays;
    this.gridMonths = monthGroups(allDays);
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
            lastChangeAt: worklog.issueLastChangeAt,
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
    this.gridRows = sortByLastChange([...rows.values()].map((row) => ({ ...row, key: row.issueKey })));
    this.gridColumns = gridColumns(allDays, this.gridRows, this.pontoByDay, today);

    this.syncedLabel = response.lastSyncedAt
      ? `Atualizado ${relativeDateTime(response.lastSyncedAt, this.timeZone)}`
      : "";
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

  trackRow(_: number, row: GridRow): string {
    return row.issueKey;
  }

  trackMonth(_index: number, month: MonthGroup): string {
    return month.key;
  }

  trackIndex(index: number): number {
    return index;
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
    return isWeekendDate(date);
  }

  trackColumn(_: number, column: GridColumn): string {
    return column.date;
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
