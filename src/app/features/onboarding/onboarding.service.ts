import { inject, Injectable, signal } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { AppSettingsService } from "../../core/preferences/app-settings.service";
import { onboardingDecision } from "./onboarding-plan";
import type { OnboardingProgress } from "../../core/tauri";

export type OnboardingStatus = "checking" | "active" | "off";

@Injectable({
  providedIn: "root",
})
export class OnboardingService {
  private readonly settings = inject(AppSettingsService);
  private readonly _status = signal<OnboardingStatus>("checking");

  readonly status = this._status.asReadonly();

  get themeSeen(): boolean {
    return this.settings.onboarding?.themeSeen ?? false;
  }

  decide(jiraConnected: boolean, pontoMaisConnected: boolean): void {
    const progress = this.settings.onboarding;
    switch (onboardingDecision(progress, jiraConnected, pontoMaisConnected)) {
      case "show":
        this._status.set("active");
        if (!progress) void this.save({ completed: false, themeSeen: false });
        return;
      case "existing-user":
        this._status.set("off");
        void this.save({ completed: true, themeSeen: true });
        return;
      case "hide":
        this._status.set("off");
    }
  }

  markThemeSeen(): void {
    if (!this.themeSeen) void this.save({ completed: false, themeSeen: true });
  }

  async finish(): Promise<void> {
    await this.save({ completed: true, themeSeen: true });
    this._status.set("off");
  }

  private async save(progress: OnboardingProgress): Promise<void> {
    try {
      await this.settings.saveOnboarding(progress);
    } catch (err) {
      error(`failed to save onboarding progress: ${err}`);
    }
  }
}
