import { Injectable, signal, inject } from "@angular/core";
import { error as logError } from "@tauri-apps/plugin-log";
import { PontoMaisService, PontoMaisStatus } from "./pontomais.service";

export type PontoMaisState = PontoMaisStatus;

interface PontoMaisStateCache {
  data: PontoMaisState;
  updatedAt: string;
}

const EMPTY_STATE: PontoMaisState = {
  connected: false,
  account: null,
  autoReconnect: false,
};

@Injectable({
  providedIn: "root",
})
export class PontoMaisStateService {
  private readonly pontoMaisService = inject(PontoMaisService);

  private readonly storageKey = "pontomais-state";

  private readonly _state = signal<PontoMaisState>(EMPTY_STATE);

  readonly state = this._state.asReadonly();

  initialize(): void {
    const cached = this.loadCache();

    if (cached) {
      this._state.set(cached);
    }
  }

  async refresh(): Promise<void> {
    try {
      this.setState(await this.pontoMaisService.getStatus());
    } catch (err) {
      logError(`failed to refresh PontoMais state: ${err}`);
      this.setState(EMPTY_STATE);
    }
  }

  async connect(login: string, password: string, autoReconnect: boolean): Promise<void> {
    try {
      await this.pontoMaisService.connect(login, password, autoReconnect);
    } finally {
      await this.refresh();
    }
  }

  async disconnect(): Promise<void> {
    try {
      await this.pontoMaisService.disconnect();
    } finally {
      await this.refresh();
    }
  }

  private setState(state: PontoMaisState): void {
    this._state.set(state);
    this.saveCache(state);
  }

  private saveCache(state: PontoMaisState): void {
    const cache: PontoMaisStateCache = {
      data: state,
      updatedAt: new Date().toISOString(),
    };

    try {
      localStorage.setItem(this.storageKey, JSON.stringify(cache));
    } catch (err) {
      logError(`failed to save PontoMais state cache: ${err}`);
    }
  }

  private loadCache(): PontoMaisState | null {
    try {
      const value = localStorage.getItem(this.storageKey);

      if (!value) {
        return null;
      }

      const cache = JSON.parse(value) as PontoMaisStateCache;
      return { ...EMPTY_STATE, ...cache.data };
    } catch (err) {
      logError(`failed to load PontoMais state cache: ${err}`);
      localStorage.removeItem(this.storageKey);
      return null;
    }
  }

  clear(): void {
    localStorage.removeItem(this.storageKey);
    this._state.set(EMPTY_STATE);
  }
}
