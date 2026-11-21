import { Injectable, computed, inject, signal } from "@angular/core";
import { JiraStateService } from "../auth/jira-state.service";

@Injectable({
  providedIn: "root",
})
export class LocalDataService {
  private readonly jiraState = inject(JiraStateService);
  private readonly _version = signal(0);

  readonly version = this._version.asReadonly();

  readonly sessionKey = computed(() => {
    if (!this.jiraState.resolved()) return null;
    const state = this.jiraState.state();
    const account = state.connected ? `${state.instance ?? ""}/${state.user?.accountId ?? ""}` : "-";
    return `${account}|${this._version()}`;
  });

  markCleared(): void {
    this._version.update((version) => version + 1);
  }
}
