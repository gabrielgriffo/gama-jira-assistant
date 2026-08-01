import { Component, NgZone, OnDestroy, OnInit, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { RouterModule } from "@angular/router";
import { UnlistenFn } from "@tauri-apps/api/event";
import { error } from "@tauri-apps/plugin-log";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Issue, IssueChange, IssueChangelogResponse } from "../../core/models";
import { issuesCommands, changelogCommands, connectionCommands, preferencesCommands } from "../../core/tauri";
import { StatusHistoryModalComponent } from "../../components/status-history-modal/status-history-modal.component";
import { DateRangePickerComponent } from "../../shared/ui/date-range-picker/date-range-picker.component";
import { DateRange, localTimeZone, relativeDateTime, todayIn } from "../../shared/date/local-date";
import { fromRangePreference, resolvePreset, toRangePreference } from "../../shared/date/range-presets";
import { SyncStateService } from "../../services/sync-state.service";

@Component({
  selector: "app-tasks",
  imports: [CommonModule, FormsModule, RouterModule, StatusHistoryModalComponent, DateRangePickerComponent],
  templateUrl: "./tasks.page.html",
  styleUrl: "./tasks.page.css",
})
export class TasksPage implements OnInit, OnDestroy {
  tasks: Issue[] = [];
  loading = false;
  errorMessage = "";
  isModalOpen = false;
  selectedTaskKey = "";
  statusHistory: IssueChange[] = [];
  earliestDate: string | null = null;
  latestDate: string | null = null;
  fetchingStatusHistory = false;

  timeZone = localTimeZone();
  range: DateRange = resolvePreset("30d", todayIn(this.timeZone));

  private unlistenUpdated?: UnlistenFn;
  private unlistenSynced?: UnlistenFn;
  private reloadingCache = false;
  private reloadQueued = false;
  private unlistenSyncFailed?: UnlistenFn;
  private unlistenStatusHistorySynced?: UnlistenFn;
  private unlistenStatusHistorySyncFailed?: UnlistenFn;

  private readonly syncState = inject(SyncStateService);

  constructor(private readonly ngZone: NgZone) {}

  get syncing(): boolean {
    return this.syncState.isRangeSyncing("issues", this.range);
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

  applyRange(range: DateRange): void {
    this.range = range;
    preferencesCommands
      .updateRangePreference("tasks", toRangePreference(range, todayIn(this.timeZone)))
      .catch((err) => error(`failed to save tasks range: ${err}`));
    this.loadTasks();
  }

  updatedLabel(task: Issue): string {
    return task.jiraUpdatedAt ? relativeDateTime(task.jiraUpdatedAt, this.timeZone) : "—";
  }

  trackTask(_: number, task: Issue): string {
    return task.id;
  }

  statusColor(task: Issue): string {
    switch (task.statusCategoryKey) {
      case "new":
        return "var(--cat-new)";
      case "done":
        return "var(--cat-done)";
      default:
        return "var(--cat-ind)";
    }
  }

  async ngOnInit(): Promise<void> {
    this.unlistenUpdated = await issuesCommands.onTasksUpdated(() => {
      this.ngZone.run(() => this.reloadCachedTasks());
    });

    this.unlistenSynced = await issuesCommands.onTasksSynced(() => {
      this.ngZone.run(() => {
        this.reloadCachedTasks();
      });
    });

    this.unlistenSyncFailed = await issuesCommands.onTasksSyncFailed((reason) => {
      if (!reason.includes("nenhuma credencial")) {
        error(`background task sync failed: ${reason}`);
      }
      this.ngZone.run(() => {
        if (!this.tasks.length) {
          this.errorMessage = "Não foi possível atualizar suas tarefas do Jira.";
        }
      });
    });

    this.unlistenStatusHistorySynced = await changelogCommands.onStatusHistorySynced((issueKey) => {
      this.ngZone.run(() => {
        if (this.isModalOpen && this.selectedTaskKey === issueKey) {
          this.reloadStatusHistory();
        }
      });
    });

    this.unlistenStatusHistorySyncFailed = await changelogCommands.onStatusHistorySyncFailed(
      ({ issue_key, reason }) => {
        if (!reason.includes("nenhuma credencial")) {
          error(`background status history sync failed for ${issue_key}: ${reason}`);
        }
        this.ngZone.run(() => {
          if (this.isModalOpen && this.selectedTaskKey === issue_key) {
            if (!this.statusHistory.length) {
              this.errorMessage = "Não foi possível carregar o histórico de status.";
            }
          }
        });
      },
    );

    await this.loadSettings();
    this.loadTasks();
  }

  private async loadSettings(): Promise<void> {
    try {
      this.timeZone = (await connectionCommands.getJiraTimeZone()) || localTimeZone();
    } catch (err) {
      error(`failed to load Jira time zone: ${err}`);
    }

    try {
      const preferences = await preferencesCommands.getPreferences();
      this.range = fromRangePreference(preferences.tasksRange, todayIn(this.timeZone));
    } catch (err) {
      error(`failed to load tasks range preference: ${err}`);
      this.range = resolvePreset("30d", todayIn(this.timeZone));
    }
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
        const tasks = await issuesCommands.getCachedTasks(this.range.from, this.range.to);
        this.ngZone.run(() => {
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
    this.unlistenUpdated?.();
    this.unlistenSynced?.();
    this.unlistenSyncFailed?.();
    this.unlistenStatusHistorySynced?.();
    this.unlistenStatusHistorySyncFailed?.();
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

    this.selectedTaskKey = task.key;
    this.isModalOpen = true;
    this.statusHistory = [];
    this.earliestDate = null;
    this.latestDate = null;
    this.errorMessage = "";
    this.fetchingStatusHistory = true;

    try {
      const response = await changelogCommands.getStatusHistory(task.key);
      this.statusHistory = response.entries;
      this.earliestDate = response.earliestDate;
      this.latestDate = response.latestDate;
    } catch (err) {
      error(`failed to get status history ${task.key}: ${err}`);
      this.errorMessage = "Não foi possível carregar o histórico de status.";
    } finally {
      this.fetchingStatusHistory = false;
    }
  }

  private async reloadStatusHistory(): Promise<void> {
    if (!this.selectedTaskKey) return;

    try {
      const response = await changelogCommands.getCachedStatusHistory(this.selectedTaskKey);
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

  loadTasks(force = false): void {
    this.loading = true;
    issuesCommands
      .listMyTasks(this.range.from, this.range.to, force)
      .then((tasks) => {
        this.tasks = tasks;
        this.errorMessage = "";
      })
      .catch((err) => {
        if (String(err).includes("nenhuma credencial")) return;
        this.errorMessage = "Não foi possível carregar suas tarefas do Jira.";
        error(`failed to list my tasks: ${err}`);
      })
      .finally(() => {
        this.loading = false;
      });
  }
}
