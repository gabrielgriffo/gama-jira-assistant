import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { WorklogSubmission } from "../models/worklog";

@Injectable({
  providedIn: "root",
})
export class WorklogService {
  async saveAndSyncWorklogsToJira(worklogs: WorklogSubmission[]): Promise<number> {
    return await invoke<number>("save_and_sync_worklogs_to_jira", { worklogs });
  }
}
