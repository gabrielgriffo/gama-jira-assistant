import { Component, NgZone, OnDestroy, OnInit, effect, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Issue, IssueChange, IssueChangelogResponse } from "../../../../core/models";
import { issuesCommands, changelogCommands, isMissingCredentials } from "../../../../core/tauri";
import { AppSettingsService } from "../../../../core/preferences/app-settings.service";
import { StatusCatalogStore } from "../../../../core/issues/status-catalog.store";
import { StatusHistoryModalComponent } from "../../../../shared/components/status-history-modal/status-history-modal.component";
import { DateRangePickerComponent } from "../../../../shared/ui/date-range-picker/date-range-picker.component";
import { brDate, relativeDateTime } from "../../../../shared/date/local-date";
import { RangeSelection } from "../../../../shared/date/range-presets";
import { SyncStateService } from "../../../../core/sync/sync-state.service";
import { filterTasks, sortTasks, tabCounts, TaskTab, TaskTabCount } from "./task-filters";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { JiraStateService } from "../../../../core/auth/jira-state.service";
import { LatestOnly } from "../../../../shared/async/latest-only";
import { ListenerBag } from "../../../../shared/async/listeners";
import { changeTracker } from "../../../../shared/async/change-tracker";
import { LocalDataService } from "../../../../core/sync/local-data.service";

@Component({
  selector: "app-tasks",
  imports: [
    IconComponent,
    CommonModule,
    FormsModule,
    RouterModule,
    StatusHistoryModalComponent,
    DateRangePickerComponent,
  ],
  templateUrl: "./tasks.page.html",
  styleUrl: "./tasks.page.css",
})
export class TasksPage implements OnInit, OnDestroy {
  tasks: Issue[] = [];
  activeTab: TaskTab = "all";
  query = "";
  developmentFirst = true;
  loading = false;
  errorMessage = "";
  historyError = "";
  isModalOpen = false;
  selectedTaskKey = "";
  statusHistory: IssueChange[] = [];
  earliestDate: string | null = null;
  latestDate: string | null = null;
  fetchingStatusHistory = false;

  private readonly settings = inject(AppSettingsService);
  range: RangeSelection = this.settings.range("tasks", "30d");

  private readonly listeners = new ListenerBag();
  private readonly taskLoads = new LatestOnly();
  private readonly cacheLoads = new LatestOnly();
  private reloadingCache = false;
  private reloadQueued = false;
  private visibleCache?: { key: unknown[]; value: Issue[] };

  private readonly syncState = inject(SyncStateService);

  constructor(private readonly ngZone: NgZone) {
    const statusCatalog = inject(StatusCatalogStore);
    let first = true;
    effect(() => {
      statusCatalog.statuses();
      if (first) {
        first = false;
        return;
      }
      this.reloadCachedTasks();
    });

    const jiraState = inject(JiraStateService);
    let wasConnected: boolean | null = null;
    effect(() => {
      const resolved = jiraState.resolved();
      const connected = jiraState.state().connected;
      if (!resolved) return;
      const previous = wasConnected;
      wasConnected = connected;
      if (previous === null || previous === connected) return;
      if (connected) {
        this.range = this.settings.range("tasks", "30d");
        this.loadTasks();
      } else {
        this.taskLoads.begin();
        this.cacheLoads.begin();
        this.loading = false;
        this.tasks = [];
        this.errorMessage = "";
        this.closeModal();
      }
    });

    const localData = inject(LocalDataService);
    const cleared = changeTracker<number>();
    effect(() => {
      if (!cleared(localData.version())) return;
      this.tasks = [];
      this.closeModal();
      if (jiraState.state().connected) this.loadTasks();
    });
  }

  get timeZone(): string {
    return this.settings.timeZone;
  }

  scrollToTop(element: HTMLElement): void {
    element.scrollTo({ top: 0, behavior: "smooth" });
  }

  scrollToBottom(element: HTMLElement): void {
    element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
  }

  get visibleTasks(): Issue[] {
    const key = [this.tasks, this.activeTab, this.query, this.developmentFirst];
    const cached = this.visibleCache;
    if (!cached || key.some((part, i) => part !== cached.key[i])) {
      this.visibleCache = {
        key,
        value: sortTasks(filterTasks(this.tasks, this.activeTab, this.query), this.developmentFirst),
      };
    }
    return this.visibleCache!.value;
  }

  get tabs(): TaskTabCount[] {
    return tabCounts(this.tasks).filter((tab) => tab.id === "all" || tab.count > 0 || tab.id === this.activeTab);
  }

  trackTab(_: number, tab: TaskTabCount): TaskTab {
    return tab.id;
  }

  get syncing(): boolean {
    return this.syncState.isRangeSyncing("issues", this.range);
  }

  get loadingStatusHistory(): boolean {
    return this.fetchingStatusHistory;
  }

  get syncingStatusHistory(): boolean {
    return this.isModalOpen && this.syncState.isChangelogSyncing(this.selectedTaskKey);
  }

  applyRange(range: RangeSelection): void {
    this.range = range;
    this.settings.saveRange("tasks", range);
    this.loadTasks();
  }

  metaLabel(task: Issue): string {
    const parts: string[] = [];
    if (task.issueTypeName) {
      parts.push(task.issueTypeName, task.priorityName ? `Prioridade ${task.priorityName}` : "Sem prioridade");
    }
    if (task.dueDate) parts.push(`Vencimento em ${brDate(task.dueDate)}`);
    const label = parts.join(" · ");
    return label.charAt(0).toUpperCase() + label.slice(1);
  }

  updatedLabel(task: Issue): string {
    return task.jiraUpdatedAt ? relativeDateTime(task.jiraUpdatedAt, this.timeZone) : "—";
  }

  trackTask(_: number, task: Issue): string {
    return task.id;
  }

  statusBadge(task: Issue): string {
    switch (task.statusCategoryKey) {
      case "new":
        return "";
      case "done":
        return "ok";
      default:
        return "info";
    }
  }

  async ngOnInit(): Promise<void> {
    await this.listeners.add(
      issuesCommands.onTasksUpdated(() => {
        this.ngZone.run(() => this.reloadCachedTasks());
      }),
    );

    await this.listeners.add(
      issuesCommands.onTasksSynced(() => {
        this.ngZone.run(() => this.reloadCachedTasks());
      }),
    );

    await this.listeners.add(
      issuesCommands.onTasksSyncFailed((reason) => {
        if (isMissingCredentials(reason)) return;
        error(`background task sync failed: ${reason}`);
        this.ngZone.run(() => {
          if (!this.tasks.length) {
            this.errorMessage = "Não foi possível atualizar suas tarefas do Jira.";
          }
        });
      }),
    );

    await this.listeners.add(
      changelogCommands.onStatusHistorySynced((issueKey) => {
        this.ngZone.run(() => {
          this.reloadCachedTasks();
          if (this.isShowingHistory(issueKey)) {
            this.reloadStatusHistory();
          }
        });
      }),
    );

    await this.listeners.add(
      changelogCommands.onStatusHistorySyncFailed(({ issueKey, reason }) => {
        if (!isMissingCredentials(reason)) {
          error(`background status history sync failed for ${issueKey}: ${reason}`);
        }
        this.ngZone.run(() => {
          if (this.isShowingHistory(issueKey) && !this.statusHistory.length) {
            this.historyError = "Não foi possível carregar o histórico de status.";
          }
        });
      }),
    );

    if (this.listeners.isDisposed) return;
    this.loadTasks();
  }

  private async reloadCachedTasks(): Promise<void> {
    if (this.reloadingCache) {
      this.reloadQueued = true;
      return;
    }

    this.reloadingCache = true;
    try {
      do {
        this.reloadQueued = false;
        const range = this.range;
        const isCurrent = this.cacheLoads.begin();
        const tasks = await issuesCommands.getCachedTasks(range.from, range.to);
        this.ngZone.run(() => {
          if (!isCurrent() || range !== this.range) return;
          this.tasks = tasks;
          if (tasks.length) {
            this.errorMessage = "";
          }
        });
      } while (this.reloadQueued);
    } catch (err) {
      error(`failed to reload cached tasks: ${err}`);
    } finally {
      this.reloadingCache = false;
    }
  }

  ngOnDestroy(): void {
    this.listeners.dispose();
  }

  openTask(task: Issue, event: Event): void {
    event.preventDefault();
    if (task.browseUrl) {
      openUrl(task.browseUrl).catch((err) => {
        error(`failed to open task ${task.key}: ${err}`);
      });
    }
  }

  async viewStatusHistory(task: Issue, event: Event): Promise<void> {
    event.preventDefault();
    event.stopPropagation();

    const key = task.key;
    this.selectedTaskKey = key;
    this.isModalOpen = true;
    this.statusHistory = [];
    this.earliestDate = null;
    this.latestDate = null;
    this.historyError = "";
    this.fetchingStatusHistory = true;

    try {
      const response = await changelogCommands.getStatusHistory(key);
      if (!this.isShowingHistory(key)) return;
      this.statusHistory = response.entries;
      this.earliestDate = response.earliestDate;
      this.latestDate = response.latestDate;
    } catch (err) {
      error(`failed to get status history ${key}: ${err}`);
      if (this.isShowingHistory(key)) this.historyError = "Não foi possível carregar o histórico de status.";
    } finally {
      if (this.isShowingHistory(key)) this.fetchingStatusHistory = false;
    }
  }

  async syncStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey;
    if (!key) return;
    this.historyError = "";
    try {
      await changelogCommands.getStatusHistory(key, true);
    } catch (err) {
      error(`failed to sync status history ${key}: ${err}`);
      if (this.isShowingHistory(key)) this.historyError = "Não foi possível sincronizar o histórico de status.";
    }
  }

  private async reloadStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey;
    if (!key) return;

    try {
      const response = await changelogCommands.getCachedStatusHistory(key);
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

  loadTasks(force = false): void {
    const range = this.range;
    const isCurrent = this.taskLoads.begin();
    this.cacheLoads.begin();
    this.loading = true;
    issuesCommands
      .listMyTasks(range.from, range.to, force)
      .then((tasks) => {
        if (!isCurrent()) return;
        this.tasks = tasks;
        this.errorMessage = "";
      })
      .catch((err) => {
        if (!isCurrent() || isMissingCredentials(err)) return;
        this.errorMessage = "Não foi possível carregar suas tarefas do Jira.";
        error(`failed to list my tasks: ${err}`);
      })
      .finally(() => {
        if (isCurrent()) this.loading = false;
      });
  }
}
