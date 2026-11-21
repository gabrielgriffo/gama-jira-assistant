import { Injectable } from "@angular/core";
import { invoke } from "../tauri/tauri-client";

export interface TimelineEntry {
  time: string;
  entryType: string;
  disabled: boolean;
}

export interface MinuteRange {
  start: number;
  end: number;
}

export interface TaskWorkedInfo {
  key: string;
  issueId: string;
  summary: string;
  workedMinutes: number;
  startTime: string | null;
  devSegments: MinuteRange[];
  countedSegments: MinuteRange[];
}

export interface WorkDayTimeline {
  date: string;
  isToday: boolean;
  totalHours: number;
  totalMinutes: number;
  entries: TimelineEntry[];
  punchIntervals: MinuteRange[];
  incomplete: boolean;
  open: boolean;
  tasksWorked: TaskWorkedInfo[];
  registeredMinutes: number;
  unconfirmedMinutes: number;
  gamaSyncedCount: number;
  hasSyncedWorklog: boolean;
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
