import { Injectable, signal, inject } from "@angular/core";
import { error as logError } from "@tauri-apps/plugin-log";
import { JiraAuthService } from "./jira-auth.service";
import { CurrentUser } from "./current-user";
import { CommandError } from "../tauri/errors";

export interface JiraState {
  connected: boolean;
  user: CurrentUser | null;
  instance: string | null;
  email: string | null;
}

const LEGACY_STORAGE_KEYS = [
  "jira-state",
  "jira_user_email",
  "jira_base_url",
  "jira_user_display_name",
  "jira_user_account_id",
];

const DISCONNECTED: JiraState = {
  connected: false,
  user: null,
  instance: null,
  email: null,
};

@Injectable({
  providedIn: "root",
})
export class JiraStateService {
  private readonly jiraAuthService = inject(JiraAuthService);

  private readonly _state = signal<JiraState>(DISCONNECTED);

  readonly state = this._state.asReadonly();
  private readonly _resolved = signal(false);
  readonly resolved = this._resolved.asReadonly();
  private readonly _error = signal<string | null>(null);
  readonly error = this._error.asReadonly();

  initialize(): void {
    try {
      LEGACY_STORAGE_KEYS.forEach((key) => localStorage.removeItem(key));
    } catch (err) {
      logError(`failed to clear legacy Jira storage: ${err}`);
    }
  }

  async refresh(): Promise<void> {
    try {
      await this.loadState();
    } catch (err) {
      logError(`failed to refresh Jira state: ${err}`);
      this._error.set(`Não foi possível verificar a conexão com o Jira: ${err}`);
    }
  }

  private async loadState(): Promise<void> {
    try {
      const [connected, info] = await Promise.all([
        this.jiraAuthService.checkConnection(),
        this.jiraAuthService.getConnectionInfo(),
      ]);

      this._error.set(null);
      if (connected && info) {
        this._state.set({
          connected: true,
          user: info.user,
          instance: info.instance,
          email: info.user.email,
        });
      } else {
        this._state.set(DISCONNECTED);
      }
    } catch (error) {
      this._state.set(DISCONNECTED);
      throw error;
    } finally {
      this._resolved.set(true);
    }
  }

  async login(baseUrl: string, email: string, apiToken: string): Promise<void> {
    await this.jiraAuthService.login(baseUrl, email, apiToken);
    await this.loadState();
    if (!this._state().connected) throw new CommandError("connection_not_confirmed", "a conexão não foi confirmada");
  }

  async logout(): Promise<void> {
    await this.jiraAuthService.logout();
    this._error.set(null);
    this._state.set(DISCONNECTED);
  }

  clear(): void {
    this._state.set(DISCONNECTED);
  }
}
