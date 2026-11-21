import { Injectable } from "@angular/core";
import { invoke } from "../../../core/tauri/tauri-client";
import { JiraWorklogHistoryResponse } from "./jira-worklog";

@Injectable({
  providedIn: "root",
})
export class JiraWorklogService {
  async getWorklogsForPeriod(fromDate: string, toDate: string): Promise<JiraWorklogHistoryResponse> {
    return await invoke<JiraWorklogHistoryResponse>("get_jira_worklogs_for_period", {
      fromDate,
      toDate,
    });
  }

  async syncWorklogs(fromDate: string, toDate: string, force = false): Promise<number | null> {
    return await invoke<number | null>("sync_jira_worklogs", {
      fromDate,
      toDate,
      force,
    });
  }
}
