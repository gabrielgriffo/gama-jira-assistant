import { Injectable, signal } from "@angular/core";
import { JiraAuthService } from "./jira-auth.service";
import { CurrentUser } from "../models/current-user";

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
  private readonly _state = signal<JiraState>(DISCONNECTED);

  readonly state = this._state.asReadonly();

  constructor(private readonly jiraAuthService: JiraAuthService) {}

  initialize(): void {
    try {
      LEGACY_STORAGE_KEYS.forEach((key) => localStorage.removeItem(key));
    } catch (error) {
      console.error("Erro ao limpar dados antigos do Jira:", error);
    }
  }

  async refresh(): Promise<void> {
    try {
      const [connected, info] = await Promise.all([
        this.jiraAuthService.checkConnection(),
        this.jiraAuthService.getConnectionInfo(),
      ]);

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
      console.error("Erro ao atualizar estado do Jira:", error);
      this._state.set(DISCONNECTED);
    }
  }

  async login(baseUrl: string, email: string, apiToken: string): Promise<void> {
    await this.jiraAuthService.login(baseUrl, email, apiToken);
    await this.refresh();
  }

  async logout(): Promise<void> {
    await this.jiraAuthService.logout();
    this._state.set(DISCONNECTED);
  }

  clear(): void {
    this._state.set(DISCONNECTED);
  }
}
