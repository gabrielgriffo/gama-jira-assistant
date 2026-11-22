import { Injectable, computed, isDevMode, signal } from "@angular/core";
import { getVersion } from "@tauri-apps/api/app";
import { error as logError, info as logInfo } from "@tauri-apps/plugin-log";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { updateStatusText, type UpdateStatus } from "./update-status";

const CHECK_TIMEOUT_MS = 30_000;
const DOWNLOAD_TIMEOUT_MS = 10 * 60_000;

@Injectable({ providedIn: "root" })
export class UpdateService {
  readonly status = signal<UpdateStatus>({ phase: "idle" });
  readonly currentVersion = signal("");
  readonly statusText = computed(() => updateStatusText(this.status(), this.currentVersion()));
  readonly busy = computed(() => ["checking", "downloading", "installing"].includes(this.status().phase));
  private readonly dismissed = signal(false);
  readonly showBanner = computed(() => this.status().phase === "ready" && !this.dismissed());

  private pending: Update | null = null;

  async initialize(): Promise<void> {
    this.currentVersion.set(await getVersion().catch(() => ""));
    if (isDevMode()) {
      this.status.set({ phase: "unavailable" });
      return;
    }
    await this.checkAndDownload(false);
  }

  async checkNow(): Promise<void> {
    if (isDevMode()) {
      this.status.set({ phase: "unavailable" });
      return;
    }
    await this.checkAndDownload(true);
  }

  dismiss(): void {
    this.dismissed.set(true);
  }

  async install(): Promise<void> {
    const update = this.pending;
    if (!update || this.status().phase === "installing") return;

    this.status.set({ phase: "installing", version: update.version });
    void logInfo(`[UPDATER] Instalando a versão ${update.version}`);
    try {
      await update.install();
      await relaunch();
    } catch (err) {
      void logError(`[UPDATER] Falha ao instalar a versão ${update.version}: ${err}`);
      this.status.set({ phase: "error", error: String(err) });
    }
  }

  private async checkAndDownload(manual: boolean): Promise<void> {
    const phase = this.status().phase;
    if (this.busy() || phase === "ready") return;

    this.status.set({ phase: "checking" });
    void logInfo(`[UPDATER] Verificando atualizações (${manual ? "manual" : "ao abrir"})`);
    try {
      const update = await check({ timeout: CHECK_TIMEOUT_MS });
      if (!update) {
        void logInfo("[UPDATER] Nenhuma atualização disponível");
        this.status.set({ phase: manual ? "up-to-date" : "idle" });
        return;
      }

      void logInfo(`[UPDATER] Versão ${update.version} encontrada; baixando`);
      this.status.set({ phase: "downloading", version: update.version, downloaded: 0 });
      let downloaded = 0;
      let total: number | undefined;
      await update.download(
        (event) => {
          if (event.event === "Started") total = event.data.contentLength;
          if (event.event === "Progress") downloaded += event.data.chunkLength;
          this.status.set({ phase: "downloading", version: update.version, downloaded, total });
        },
        { timeout: DOWNLOAD_TIMEOUT_MS },
      );

      this.pending = update;
      this.dismissed.set(false);
      this.status.set({ phase: "ready", version: update.version });
      void logInfo(`[UPDATER] Versão ${update.version} baixada e pronta para instalar`);
    } catch (err) {
      void logError(`[UPDATER] Falha ao verificar ou baixar a atualização: ${err}`);
      this.status.set(manual ? { phase: "error", error: String(err) } : { phase: "idle" });
    }
  }
}
