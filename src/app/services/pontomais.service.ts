import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";

export interface TimelineEntry {
  time: string;
  entry_type: string;
  disabled: boolean;
}

export interface MinuteRange {
  start: number;
  end: number;
}

export interface TaskWorkedInfo {
  key: string;
  issue_id: string;
  summary: string;
  worked_minutes: number;
  start_time: string | null;
  dev_segments: MinuteRange[];
  counted_segments: MinuteRange[];
}

export interface WorkDayTimeline {
  date: string;
  is_today: boolean;
  total_hours: number;
  total_minutes: number;
  entries: TimelineEntry[];
  punch_intervals: MinuteRange[];
  incomplete: boolean;
  open: boolean;
  tasks_worked: TaskWorkedInfo[];
  registered_minutes: number;
  gama_synced_count: number;
  has_synced_worklog: boolean;
}

export interface PontoMaisStatus {
  connected: boolean;
  account: string | null;
  autoReconnect: boolean;
}

@Injectable({
  providedIn: "root",
})
export class PontoMaisService {
  async getStatus(): Promise<PontoMaisStatus> {
    return await invoke<PontoMaisStatus>("pontomais_status");
  }

  async connect(login: string, password: string, autoReconnect: boolean): Promise<void> {
    await invoke("pontomais_connect", { login, password, autoReconnect });
  }

  async disconnect(): Promise<void> {
    await invoke("pontomais_disconnect");
  }

  async syncPontoMaisEntries(fromDate: string, toDate: string): Promise<number> {
    return await invoke<number>("sync_pontomais_entries", {
      fromDate,
      toDate,
    });
  }

  async getPontoMaisTimeline(fromDate: string, toDate: string): Promise<WorkDayTimeline[]> {
    return await invoke<WorkDayTimeline[]>("get_pontomais_timeline", {
      fromDate,
      toDate,
    });
  }
}
