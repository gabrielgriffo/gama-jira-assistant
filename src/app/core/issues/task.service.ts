import { Injectable } from "@angular/core";
import { invoke } from "../tauri/tauri-client";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { StatusHistoryResponse } from "./status-history";

@Injectable({ providedIn: "root" })
export class TaskService {
  /**
   * Busca o histórico de status de uma task específica do Jira.
   */
  getStatusHistory(issueKey: string, force = false): Promise<StatusHistoryResponse> {
    return invoke<StatusHistoryResponse>("get_status_history", { issueKey, force });
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

  onStatusHistorySyncFailed(callback: (payload: { issueKey: string; reason: string }) => void): Promise<UnlistenFn> {
    return listen<{ issueKey: string; reason: string }>("status-history-sync-failed", (event) =>
      callback(event.payload),
    );
  }
}
