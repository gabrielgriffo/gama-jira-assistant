import { Injectable, signal } from "@angular/core";
import type { SettingsSection } from "./pages/settings/settings.page";

@Injectable({
  providedIn: "root",
})
export class SettingsDialogService {
  private readonly _open = signal(false);
  private readonly _section = signal<SettingsSection>("general");

  readonly open = this._open.asReadonly();
  readonly section = this._section.asReadonly();

  show(section: SettingsSection = "general"): void {
    this._section.set(section);
    this._open.set(true);
  }

  hide(): void {
    this._open.set(false);
  }
}
