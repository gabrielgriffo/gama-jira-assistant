import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
  provideZoneChangeDetection,
} from "@angular/core";
import { provideRouter } from "@angular/router";

import { routes } from "./app.routes";
import { AppSettingsService } from "./core/preferences/app-settings.service";
import { StatusCatalogStore } from "./core/issues/status-catalog.store";

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideZoneChangeDetection({ eventCoalescing: true }),
    provideRouter(routes),
    provideAppInitializer(() => inject(AppSettingsService).load()),
    provideAppInitializer(() => inject(StatusCatalogStore).load()),
  ],
};
