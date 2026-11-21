import { Injectable, inject } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { preferencesCommands } from "../tauri";
import { AppSettingsService } from "./app-settings.service";
import { resolveTheme, Theme } from "./theme";

const STORAGE_KEY = "gama-theme";

@Injectable({ providedIn: "root" })
export class ThemeService {
  private readonly settings = inject(AppSettingsService);
  private current: Theme = "light";

  get theme(): Theme {
    return this.current;
  }

  initialize(): void {
    const cached = readCache();
    this.current = resolveTheme(cached, this.settings.savedTheme);
    this.apply();
    if (this.settings.savedTheme !== this.current) this.persist();
  }

  set(theme: Theme): void {
    this.current = theme;
    this.apply();
    this.persist();
  }

  private apply(): void {
    if (this.current === "system") {
      document.documentElement.removeAttribute("data-theme");
    } else {
      document.documentElement.setAttribute("data-theme", this.current);
    }
    try {
      localStorage.setItem(STORAGE_KEY, this.current);
    } catch {}
  }

  private persist(): void {
    const theme = this.current;
    preferencesCommands
      .updateTheme(theme)
      .then(() => this.settings.rememberTheme(theme))
      .catch((err) => error(`failed to save theme: ${err}`));
  }
}

function readCache(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}
