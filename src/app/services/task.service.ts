import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { StatusHistoryResponse } from "../models/status-history";

@Injectable({ providedIn: "root" })
export class TaskService {
  /**
   * Busca o histórico de status de uma task específica do Jira.
   */
  getStatusHistory(issueKey: string): Promise<StatusHistoryResponse> {
    return invoke<StatusHistoryResponse>("get_status_history", { issueKey });
  }

  /**
   * Retorna apenas o histórico em cache, sem disparar sync em background.
   */
  getCachedStatusHistory(issueKey: string): Promise<StatusHistoryResponse> {
    return invoke<StatusHistoryResponse>("get_cached_status_history", { issueKey });
  }

  onStatusHistorySynced(callback: (issueKey: string) => void): Promise<UnlistenFn> {
    return listen<string>("status-history-synced", (event) => callback(event.payload));
  }

  onStatusHistorySyncFailed(callback: (payload: { issue_key: string; reason: string }) => void): Promise<UnlistenFn> {
    return listen<{ issue_key: string; reason: string }>("status-history-sync-failed", (event) =>
      callback(event.payload),
    );
  }
}
