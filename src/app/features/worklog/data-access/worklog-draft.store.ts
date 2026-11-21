import { Injectable, effect, inject } from "@angular/core";
import { LocalDataService } from "../../../core/sync/local-data.service";
import { changeTracker } from "../../../shared/async/change-tracker";
import { WorklogDraft } from "./worklog";

@Injectable({
  providedIn: "root",
})
export class WorklogDraftStore {
  readonly drafts = new Map<string, WorklogDraft[]>();
  readonly generated = new Map<string, string>();
  private seq = 0;

  constructor() {
    const localData = inject(LocalDataService);
    const changed = changeTracker<string>();
    effect(() => {
      if (changed(localData.sessionKey())) this.clear();
    });
  }

  nextId(): number {
    return ++this.seq;
  }

  clear(): void {
    this.drafts.clear();
    this.generated.clear();
  }
}
