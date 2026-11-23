import { computed, effect, inject, Injectable, signal } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { JiraStateService } from "../auth/jira-state.service";
import { connectionCommands, OnboardingProgress, Preferences, preferencesCommands, RangeScreen } from "../tauri";
import { localTimeZone, todayIn } from "../../shared/date/local-date";
import { LatestOnly } from "../../shared/async/latest-only";
import { fromRangePreference, RangePreset, RangeSelection, toRangePreference } from "../../shared/date/range-presets";

const DEFAULT_MIN_SYNC_INTERVAL_MINUTES = 5;
const CLOCK_TICK_MS = 60_000;

const RANGE_KEYS = {
  tasks: "tasksRange",
  worklogGenerate: "worklogGenerateRange",
  worklogHistory: "worklogHistoryRange",
} as const satisfies Record<RangeScreen, keyof Preferences>;

@Injectable({
  providedIn: "root",
})
export class AppSettingsService {
  private preferences: Preferences | null = null;
  private readonly zone = signal(localTimeZone());
  private readonly zoneLoads = new LatestOnly();
  private readonly clock = signal(0);
  readonly today = computed(() => {
    this.clock();
    return todayIn(this.zone());
  });
  private lastConnected: boolean | null = null;

  constructor() {
    setInterval(() => this.clock.update((tick) => tick + 1), CLOCK_TICK_MS);
    const jiraState = inject(JiraStateService);
    effect(() => {
      const resolved = jiraState.resolved();
      const connected = jiraState.state().connected;
      if (!resolved) return;
      if (this.lastConnected !== null && this.lastConnected !== connected) this.refreshTimeZone();
      this.lastConnected = connected;
    });
  }

  get timeZone(): string {
    return this.zone();
  }

  get savedTheme(): Preferences["theme"] | undefined {
    return this.preferences?.theme;
  }

  rememberTheme(theme: Preferences["theme"]): void {
    if (this.preferences) this.preferences.theme = theme;
  }

  get minSyncIntervalMinutes(): number {
    return this.preferences?.minSyncIntervalMinutes ?? DEFAULT_MIN_SYNC_INTERVAL_MINUTES;
  }

  async saveMinSyncInterval(minutes: number): Promise<void> {
    await preferencesCommands.updateMinSyncInterval(minutes);
    if (this.preferences) this.preferences.minSyncIntervalMinutes = minutes;
  }

  get onboarding(): OnboardingProgress | undefined {
    return this.preferences?.onboarding;
  }

  async saveOnboarding(progress: OnboardingProgress): Promise<void> {
    await preferencesCommands.saveOnboarding(progress);
    if (!this.preferences) return;
    const current = this.preferences.onboarding;
    this.preferences.onboarding = {
      completed: !!current?.completed || progress.completed,
      themeSeen: !!current?.themeSeen || progress.themeSeen,
    };
  }

  async load(): Promise<void> {
    const [preferences] = await Promise.allSettled([preferencesCommands.getPreferences(), this.refreshTimeZone()]);
    if (preferences.status === "fulfilled") {
      this.preferences = preferences.value;
    } else {
      error(`failed to load preferences: ${preferences.reason}`);
    }
  }

  range(screen: RangeScreen, fallbackId: string, presets?: RangePreset[]): RangeSelection {
    return fromRangePreference(this.preferences?.[RANGE_KEYS[screen]], todayIn(this.zone()), fallbackId, presets);
  }

  saveRange(screen: RangeScreen, range: RangeSelection, presets?: RangePreset[]): void {
    const saved = toRangePreference(range, todayIn(this.zone()), presets);
    if (this.preferences) this.preferences[RANGE_KEYS[screen]] = saved;
    preferencesCommands
      .updateRangePreference(screen, saved)
      .catch((err) => error(`failed to save ${screen} range: ${err}`));
  }

  private async refreshTimeZone(): Promise<void> {
    const isCurrent = this.zoneLoads.begin();
    try {
      const zone = (await connectionCommands.getJiraTimeZone()) || localTimeZone();
      if (isCurrent()) this.zone.set(zone);
    } catch (err) {
      error(`failed to load Jira time zone: ${err}`);
    }
  }
}
