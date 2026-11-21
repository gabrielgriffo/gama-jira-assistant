import { effect, inject, Injectable, signal } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { JiraStateService } from "../auth/jira-state.service";
import { JiraStatus, statusCommands } from "../tauri";

@Injectable({
  providedIn: "root",
})
export class StatusCatalogStore {
  private readonly _statuses = signal<JiraStatus[]>([]);
  private readonly _syncedAt = signal<string | null>(null);
  private lastConnected: boolean | null = null;

  readonly statuses = this._statuses.asReadonly();
  readonly syncedAt = this._syncedAt.asReadonly();

  constructor() {
    const jiraState = inject(JiraStateService);
    effect(() => {
      const resolved = jiraState.resolved();
      const connected = jiraState.state().connected;
      if (!resolved) return;
      if (this.lastConnected !== null && this.lastConnected !== connected) this.load();
      this.lastConnected = connected;
    });
  }

  async load(): Promise<void> {
    try {
      const [statuses, syncedAt] = await Promise.all([
        statusCommands.getCatalog(),
        statusCommands.getCatalogSyncedAt(),
      ]);
      this._statuses.set(statuses);
      this._syncedAt.set(syncedAt);
    } catch (err) {
      error(`failed to load Jira status catalog: ${err}`);
    }
  }

  markDevelopment(updates: { statusId: string; isDevelopment: boolean }[]): void {
    const changed = new Map(updates.map((u) => [u.statusId, u.isDevelopment]));
    this._statuses.update((list) =>
      list.map((s) => (changed.has(s.id) ? { ...s, isDevelopment: changed.get(s.id)! } : s)),
    );
  }
}
