import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { CurrentUser } from "../models/current-user";

export interface JiraConnectionInfo {
  instance: string;
  user: CurrentUser;
}

@Injectable({ providedIn: "root" })
export class JiraAuthService {
  login(baseUrl: string, email: string, apiToken: string): Promise<CurrentUser> {
    return invoke<CurrentUser>("login", { baseUrl, email, apiToken });
  }

  /**
   * Whether there's an active, previously-verified Jira connection. Never
   * returns the base URL/e-mail/token themselves — just a boolean.
   */
  checkConnection(): Promise<boolean> {
    return invoke<boolean>("get_connection_status");
  }

  getConnectionInfo(): Promise<JiraConnectionInfo | null> {
    return invoke<JiraConnectionInfo | null>("get_connection_info");
  }

  logout(): Promise<void> {
    return invoke<void>("logout");
  }
}
