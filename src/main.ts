import { bootstrapApplication } from "@angular/platform-browser";
import { isDevMode } from "@angular/core";
import { error } from "@tauri-apps/plugin-log";
import { AppComponent } from "./app/app.component";
import { appConfig } from "./app/app.config";
import { windowCommands } from "./app/core/tauri";

if (!isDevMode()) {
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

const showWindow = () =>
  requestAnimationFrame(() => windowCommands.showMain().catch((err) => error(`show window failed: ${err}`)));

bootstrapApplication(AppComponent, appConfig)
  .catch((err) => error(`bootstrap failed: ${err}`))
  .finally(showWindow);
