import { Injectable, NgZone, inject, signal } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { syncCommands } from "../core/tauri";
import { DateRange } from "../shared/date/local-date";

export type RangeSyncResource = "issues" | "worklogs" | "period";

export function rangeSyncKey(resource: RangeSyncResource, range: DateRange): string {
  return `${resource}:${range.from}..${range.to}`;
}

export function changelogSyncKey(issueKey: string): string {
  return `changelog:${issueKey}`;
}

@Injectable({
  providedIn: "root",
})
export class SyncStateService {
  private readonly ngZone = inject(NgZone);
  private readonly _active = signal<ReadonlySet<string>>(new Set());
  private readonly finishedListeners = new Set<(key: string) => void>();
  private initialized = false;

  readonly active = this._active.asReadonly();

  async initialize(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;

    await syncCommands.onSyncStarted((key) => this.ngZone.run(() => this.toggle(key, true)));
    await syncCommands.onSyncFinished((key) =>
      this.ngZone.run(() => {
        this.toggle(key, false);
        this.finishedListeners.forEach((listener) => listener(key));
      }),
    );

    try {
      const keys = await syncCommands.getActiveSyncs();
      this.ngZone.run(() => this._active.set(new Set(keys)));
    } catch (err) {
      error(`failed to load active syncs: ${err}`);
    }
  }

  onFinished(listener: (key: string) => void): () => void {
    this.finishedListeners.add(listener);
    return () => this.finishedListeners.delete(listener);
  }

  isSyncing(key: string): boolean {
    return this._active().has(key);
  }

  isRangeSyncing(resource: RangeSyncResource, range: DateRange): boolean {
    return this.isSyncing(rangeSyncKey(resource, range));
  }

  isChangelogSyncing(issueKey: string): boolean {
    return !!issueKey && this.isSyncing(changelogSyncKey(issueKey));
  }

  private toggle(key: string, active: boolean): void {
    this._active.update((current) => {
      if (current.has(key) === active) return current;
      const next = new Set(current);
      if (active) {
        next.add(key);
      } else {
        next.delete(key);
      }
      return next;
    });
  }
}
