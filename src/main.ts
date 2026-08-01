import { bootstrapApplication } from "@angular/platform-browser";
import { isDevMode } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { AppComponent } from "./app/app.component";
import { appConfig } from "./app/app.config";

if (!isDevMode()) {
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

bootstrapApplication(AppComponent, appConfig).catch((err) => error(`bootstrap failed: ${err}`));
