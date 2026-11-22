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
import { FormsModule } from "@angular/forms";
import { error } from "@tauri-apps/plugin-log";
import { Issue, IssueChange } from "../../../../core/models";
import { issuesCommands, changelogCommands, isMissingCredentials, linkCommands } from "../../../../core/tauri";
import { AppSettingsService } from "../../../../core/preferences/app-settings.service";
import { StatusCatalogStore } from "../../../../core/issues/status-catalog.store";
import { StatusHistoryModalComponent } from "../../../../shared/components/status-history-modal/status-history-modal.component";
import { DateRangePickerComponent } from "../../../../shared/ui/date-range-picker/date-range-picker.component";
import { brDate, relativeDateTime } from "../../../../shared/date/local-date";
import { RangeSelection } from "../../../../shared/date/range-presets";
import { SyncStateService } from "../../../../core/sync/sync-state.service";
import { filterTasks, sortTasks, tabCounts, TaskTab } from "./task-filters";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { JiraStateService } from "../../../../core/auth/jira-state.service";
import { LatestOnly } from "../../../../shared/async/latest-only";
import { ListenerBag } from "../../../../shared/async/listeners";
import { changeTracker } from "../../../../shared/async/change-tracker";
import { LocalDataService } from "../../../../core/sync/local-data.service";

@Component({
  selector: "app-tasks",
  imports: [IconComponent, FormsModule, StatusHistoryModalComponent, DateRangePickerComponent],
  templateUrl: "./tasks.page.html",
  styleUrl: "./tasks.page.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TasksPage implements OnInit, OnDestroy {
  private readonly settings = inject(AppSettingsService);
  private readonly syncState = inject(SyncStateService);

  readonly tasks = signal<Issue[]>([]);
  readonly activeTab = signal<TaskTab>("all");
  readonly query = signal("");
  readonly developmentFirst = signal(true);
  readonly loading = signal(false);
  readonly errorMessage = signal("");
  readonly historyError = signal("");
  readonly isModalOpen = signal(false);
  readonly selectedTaskKey = signal("");
  readonly statusHistory = signal<IssueChange[]>([]);
  readonly earliestDate = signal<string | null>(null);
  readonly latestDate = signal<string | null>(null);
  readonly loadingStatusHistory = signal(false);
  readonly range = signal<RangeSelection>(this.settings.range("tasks", "30d"));

  readonly timeZone = computed(() => this.settings.timeZone);
  readonly today = this.settings.today;
  readonly visibleTasks = computed(() =>
    sortTasks(filterTasks(this.tasks(), this.activeTab(), this.query()), this.developmentFirst()),
  );
  readonly tabs = computed(() =>
    tabCounts(this.tasks()).filter((tab) => tab.id === "all" || tab.count > 0 || tab.id === this.activeTab()),
  );
  readonly syncing = computed(() => this.syncState.isRangeSyncing("issues", this.range()));
  readonly syncingStatusHistory = computed(
    () => this.isModalOpen() && this.syncState.isChangelogSyncing(this.selectedTaskKey()),
  );

  private readonly listeners = new ListenerBag();
  private readonly taskLoads = new LatestOnly();
  private readonly cacheLoads = new LatestOnly();
  private readonly historyLoads = new LatestOnly();
  private reloadingCache = false;
  private reloadQueued = false;

  constructor() {
    const statusCatalog = inject(StatusCatalogStore);
    let first = true;
    effect(() => {
      statusCatalog.statuses();
      if (first) {
        first = false;
        return;
      }
      untracked(() => this.reloadCachedTasks());
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
      untracked(() => {
        if (connected) {
          this.range.set(this.settings.range("tasks", "30d"));
          this.loadTasks();
        } else {
          this.taskLoads.begin();
          this.cacheLoads.begin();
          this.loading.set(false);
          this.tasks.set([]);
          this.errorMessage.set("");
          this.closeModal();
        }
      });
    });

    const localData = inject(LocalDataService);
    const cleared = changeTracker<number>();
    effect(() => {
      if (!cleared(localData.version())) return;
      const connected = jiraState.state().connected;
      untracked(() => {
        this.tasks.set([]);
        this.closeModal();
        if (connected) this.loadTasks();
      });
    });
  }

  scrollToTop(element: HTMLElement): void {
    element.scrollTo({ top: 0, behavior: "smooth" });
  }

  scrollToBottom(element: HTMLElement): void {
    element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
  }

  applyRange(range: RangeSelection): void {
    this.range.set(range);
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
    return task.jiraUpdatedAt ? relativeDateTime(task.jiraUpdatedAt, this.timeZone()) : "—";
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
    await this.listeners.add(issuesCommands.onTasksUpdated(() => this.reloadCachedTasks()));
    await this.listeners.add(issuesCommands.onTasksSynced(() => this.reloadCachedTasks()));
    await this.listeners.add(
      issuesCommands.onTasksSyncFailed((reason) => {
        if (isMissingCredentials(reason)) return;
        error(`background task sync failed: ${reason}`);
        if (!this.tasks().length) {
          this.errorMessage.set("Não foi possível atualizar suas tarefas do Jira.");
        }
      }),
    );
    await this.listeners.add(
      changelogCommands.onStatusHistorySynced((issueKey) => {
        this.reloadCachedTasks();
        if (this.isShowingHistory(issueKey)) this.reloadStatusHistory();
      }),
    );
    await this.listeners.add(
      changelogCommands.onStatusHistorySyncFailed(({ issueKey, reason }) => {
        if (!isMissingCredentials(reason)) {
          error(`background status history sync failed for ${issueKey}: ${reason}`);
        }
        if (this.isShowingHistory(issueKey) && !this.statusHistory().length) {
          this.historyError.set("Não foi possível carregar o histórico de status.");
        }
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
        const range = this.range();
        const isCurrent = this.cacheLoads.begin();
        try {
          const tasks = await issuesCommands.getCachedTasks(range.from, range.to);
          if (!isCurrent() || range !== this.range()) continue;
          this.tasks.set(tasks);
          if (tasks.length) this.errorMessage.set("");
        } catch (err) {
          error(`failed to reload cached tasks: ${err}`);
        }
      } while (this.reloadQueued);
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
      linkCommands.openJiraUrl(task.browseUrl).catch((err) => {
        error(`failed to open task ${task.key}: ${err}`);
      });
    }
  }

  async viewStatusHistory(task: Issue, event: Event): Promise<void> {
    event.preventDefault();
    event.stopPropagation();

    const key = task.key;
    const isCurrent = this.historyLoads.begin();
    this.selectedTaskKey.set(key);
    this.isModalOpen.set(true);
    this.setHistory([], null, null);
    this.historyError.set("");
    this.loadingStatusHistory.set(true);

    try {
      const response = await changelogCommands.getStatusHistory(key);
      if (!isCurrent() || !this.isShowingHistory(key)) return;
      this.setHistory(response.entries, response.earliestDate, response.latestDate);
    } catch (err) {
      error(`failed to get status history ${key}: ${err}`);
      if (isCurrent() && this.isShowingHistory(key)) {
        this.historyError.set("Não foi possível carregar o histórico de status.");
      }
    } finally {
      if (isCurrent() && this.isShowingHistory(key)) this.loadingStatusHistory.set(false);
    }
  }

  async syncStatusHistory(): Promise<void> {
    const key = this.selectedTaskKey();
    if (!key) return;
    this.historyError.set("");
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
      this.setHistory(response.entries, response.earliestDate, response.latestDate);
      this.historyError.set("");
    } catch (err) {
      error(`failed to reload status history ${key}: ${err}`);
    }
  }

  private setHistory(entries: IssueChange[], earliest: string | null, latest: string | null): void {
    this.statusHistory.set(entries);
    this.earliestDate.set(earliest);
    this.latestDate.set(latest);
  }

  private isShowingHistory(key: string): boolean {
    return this.isModalOpen() && this.selectedTaskKey() === key;
  }

  closeModal(): void {
    this.historyLoads.begin();
    this.isModalOpen.set(false);
    this.selectedTaskKey.set("");
    this.setHistory([], null, null);
    this.historyError.set("");
    this.loadingStatusHistory.set(false);
  }

  loadTasks(force = false): void {
    const range = this.range();
    const isCurrent = this.taskLoads.begin();
    this.cacheLoads.begin();
    this.loading.set(true);
    issuesCommands
      .listMyTasks(range.from, range.to, force)
      .then((tasks) => {
        if (!isCurrent()) return;
        this.tasks.set(tasks);
        this.errorMessage.set("");
      })
      .catch((err) => {
        if (!isCurrent() || isMissingCredentials(err)) return;
        this.errorMessage.set("Não foi possível carregar suas tarefas do Jira.");
        error(`failed to list my tasks: ${err}`);
      })
      .finally(() => {
        if (isCurrent()) this.loading.set(false);
      });
  }
}
