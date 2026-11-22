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
import { JiraWorklogService } from "../../data-access/jira-worklog.service";
import { PontoMaisService } from "../../../../core/pontomais/pontomais.service";
import { JiraWorklog, JiraWorklogHistoryResponse } from "../../data-access/jira-worklog";
import { ToastService } from "../../../../core/feedback/toast.service";
import { DateRangePickerComponent } from "../../../../shared/ui/date-range-picker/date-range-picker.component";
import {
  addDays,
  relativeDateTime,
  sameDates,
  todayIn,
  weekday,
  zonedDateTime,
} from "../../../../shared/date/local-date";
import { RangeSelection, WORKLOG_HISTORY_PRESETS } from "../../../../shared/date/range-presets";
import { AppSettingsService } from "../../../../core/preferences/app-settings.service";
import { rangeSyncKey, SyncStateService } from "../../../../core/sync/sync-state.service";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { monthGroups, type MonthGroup } from "./month-groups";
import { sortByLastChange } from "../../../../shared/sort/last-change";
import { isMissingCredentials, linkCommands } from "../../../../core/tauri";
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
  imports: [IconComponent, DateRangePickerComponent],
  templateUrl: "./worklog-history.page.html",
  styleUrl: "./worklog-history.page.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WorklogHistoryPage implements OnInit, OnDestroy {
  private jiraWorklogService = inject(JiraWorklogService);
  private pontoMaisService = inject(PontoMaisService);
  private toastService = inject(ToastService);

  readonly presets = WORKLOG_HISTORY_PRESETS;

  private readonly settings = inject(AppSettingsService);
  private readonly syncState = inject(SyncStateService);

  readonly timeZone = computed(() => this.settings.timeZone);
  readonly today = this.settings.today;
  readonly range = signal<RangeSelection>(this.settings.range("worklogHistory", "month", this.presets));
  readonly loading = signal(false);
  readonly response = signal<JiraWorklogHistoryResponse | null>(null);
  readonly viewMode = signal<"grid" | "day">("grid");

  readonly days = signal<HistoryDay[]>([]);
  readonly gridMonths = signal<MonthGroup[]>([]);
  readonly gridRows = signal<GridRow[]>([]);
  readonly registeredMinutes = signal(0);
  readonly worklogCount = signal(0);
  readonly pontoMinutes = signal(0);
  readonly pontoDays = signal(0);
  readonly pendingDays = signal(0);
  readonly difference = signal(0);
  readonly gridColumns = signal<GridColumn[]>([]);
  readonly syncedLabel = signal("");
  readonly loadError = signal("");
  readonly pontoError = signal(false);
  readonly differenceLabel = signal("Diferença");
  readonly syncing = computed(() => this.syncState.isRangeSyncing("worklogs", this.range()));

  private pontoByDay = new Map<string, number>();
  private readonly loads = new LatestOnly();
  private loadedRange: RangeSelection | null = null;
  private unlistenSyncFinished?: () => void;

  constructor() {
    const localData = inject(LocalDataService);
    const sessionChanged = changeTracker<string>();
    effect(() => {
      if (!sessionChanged(localData.sessionKey())) return;
      untracked(() => this.resetAndReload());
    });
  }

  private resetAndReload(): void {
    this.loads.begin();
    this.response.set(null);
    this.days.set([]);
    this.gridRows.set([]);
    this.gridColumns.set([]);
    this.loadedRange = null;
    this.loadError.set("");
    this.loadWorklogs();
    this.syncWorklogs(false);
  }

  async ngOnInit(): Promise<void> {
    this.unlistenSyncFinished = this.syncState.onFinished((key) => {
      if (key === rangeSyncKey("worklogs", this.range())) {
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
    this.range.set(range);
    this.settings.saveRange("worklogHistory", range, this.presets);
    await this.loadWorklogs();
    await this.syncWorklogs(false);
  }

  async syncWorklogs(notify = true): Promise<void> {
    if (this.syncing()) return;
    try {
      const synced = await this.jiraWorklogService.syncWorklogs(this.range().from, this.range().to, notify);
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
    const range = this.range();
    const isCurrent = this.loads.begin();
    const showLoading = this.response() ? undefined : setTimeout(() => this.loading.set(true), LOADING_DELAY_MS);
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
      if (pontoFailed && !this.pontoError()) {
        this.toastService.error("Não foi possível carregar as marcações de ponto do período.");
      }
      this.pontoError.set(pontoFailed);
      this.response.set(response);
      this.loadError.set("");
      this.loadedRange = range;
      this.pontoByDay = new Map(timeline.map((day) => [day.date, day.totalMinutes]));
      this.buildView(response, range);
    } catch (err) {
      if (!isCurrent()) return;
      error(`failed to load worklogs: ${err}`);
      this.toastService.error(`Não foi possível carregar os worklogs: ${err}`);
      if (!sameDates(this.loadedRange, range)) {
        this.response.set(null);
        this.days.set([]);
        this.gridRows.set([]);
        this.gridColumns.set([]);
        this.loadError.set("Não foi possível carregar os worklogs do período.");
      }
    } finally {
      clearTimeout(showLoading);
      if (isCurrent()) this.loading.set(false);
    }
  }

  private buildView(response: JiraWorklogHistoryResponse, range: RangeSelection): void {
    const today = todayIn(this.timeZone());
    const lastDay = range.to < today ? range.to : today;
    const worklogsByDay = new Map(response.days.map((day) => [day.date, day.worklogs]));

    const allDays: string[] = [];
    for (let d = lastDay; d >= range.from; d = addDays(d, -1)) allDays.push(d);

    this.days.set(
      allDays
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
        .filter((day) => day.worklogs.length > 0 || day.pontoMinutes > 0),
    );

    this.registeredMinutes.set(this.days().reduce((sum, day) => sum + day.registeredMinutes, 0));
    this.worklogCount.set(this.days().reduce((sum, day) => sum + day.worklogs.length, 0));
    this.pontoMinutes.set([...this.pontoByDay.values()].reduce((sum, m) => sum + m, 0));
    this.pontoDays.set([...this.pontoByDay.values()].filter((m) => m > 0).length);
    this.pendingDays.set(this.days().filter((day) => day.pending).length);
    this.difference.set(differenceBeforeToday(this.days(), today));
    this.differenceLabel.set(range.to >= today ? "Diferença até ontem" : "Diferença");

    this.gridMonths.set(monthGroups(allDays));
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
    this.gridRows.set(sortByLastChange([...rows.values()].map((row) => ({ ...row, key: row.issueKey }))));
    this.gridColumns.set(gridColumns(allDays, this.gridRows(), this.pontoByDay, today));

    this.syncedLabel.set(
      response.lastSyncedAt ? `Atualizado ${relativeDateTime(response.lastSyncedAt, this.timeZone())}` : "",
    );
  }

  openIssue(url: string): void {
    if (!url) return;
    linkCommands.openJiraUrl(url).catch((err) => error(`failed to open issue: ${err}`));
  }

  isZero(worklog: JiraWorklog): boolean {
    return worklog.timeSpentSeconds <= 0;
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
    const start = zonedDateTime(worklog.started, this.timeZone());
    const end = zonedDateTime(
      new Date(new Date(worklog.started).getTime() + worklog.timeSpentSeconds * 1000).toISOString(),
      this.timeZone(),
    );
    if (!start || !end) return "";
    return `${start.time}–${end.time}`;
  }
}
