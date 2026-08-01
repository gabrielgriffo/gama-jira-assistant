import { Injectable, signal } from "@angular/core";
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
  private readonly storageKey = "pontomais-state";

  private readonly _state = signal<PontoMaisState>(EMPTY_STATE);

  readonly state = this._state.asReadonly();

  constructor(private readonly pontoMaisService: PontoMaisService) {}

  initialize(): void {
    const cached = this.loadCache();

    if (cached) {
      this._state.set(cached);
    }
  }

  async refresh(): Promise<void> {
    try {
      this.setState(await this.pontoMaisService.getStatus());
    } catch (error) {
      console.error("Erro ao atualizar estado do PontoMais:", error);
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
    } catch (error) {
      console.error("Erro ao salvar cache do PontoMais:", error);
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
    } catch (error) {
      console.error("Erro ao carregar cache do PontoMais:", error);
      localStorage.removeItem(this.storageKey);
      return null;
    }
  }

  clear(): void {
    localStorage.removeItem(this.storageKey);
    this._state.set(EMPTY_STATE);
  }
}
