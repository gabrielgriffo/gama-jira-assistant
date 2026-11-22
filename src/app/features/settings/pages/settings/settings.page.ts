import { Component, HostListener, OnInit, effect, inject, input } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { A11yModule } from "@angular/cdk/a11y";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import { JiraStateService } from "../../../../core/auth/jira-state.service";
import { PontoMaisStateService } from "../../../../core/pontomais/pontomais-state.service";
import { JiraStatus, preferencesCommands, statusCommands } from "../../../../core/tauri";
import { ToastService } from "../../../../core/feedback/toast.service";
import { SelectComponent, SelectOption } from "../../../../shared/ui/select/select.component";
import { AppSettingsService } from "../../../../core/preferences/app-settings.service";
import { StatusCatalogStore } from "../../../../core/issues/status-catalog.store";
import { relativeDateTime } from "../../../../shared/date/local-date";
import { LocalDataService } from "../../../../core/sync/local-data.service";
import { changeTracker } from "../../../../shared/async/change-tracker";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { ThemeService } from "../../../../core/preferences/theme.service";
import type { Theme } from "../../../../core/preferences/theme";
import { UpdateService } from "../../../../core/update/update.service";

const STATUS_COLUMNS = 3;

const MANUAL_SYNC_ONLY = 4294967295;

const SYNC_INTERVAL_OPTIONS: SelectOption<number>[] = [
  { value: MANUAL_SYNC_ONLY, label: "Somente manual" },
  { value: 0, label: "Sem intervalo" },
  { value: 1, label: "1 minuto" },
  { value: 5, label: "5 minutos" },
  { value: 15, label: "15 minutos" },
  { value: 30, label: "30 minutos" },
  { value: 60, label: "1 hora" },
];

const STATUS_CATEGORIES: Record<string, { label: string; badge: string; order: number }> = {
  new: { label: "A fazer", badge: "", order: 0 },
  indeterminate: { label: "Em andamento", badge: "info", order: 1 },
  done: { label: "Concluído", badge: "ok", order: 2 },
};

interface StatusColumn {
  statuses: JiraStatus[];
  fillers: number[];
}

export type SettingsSection = "general" | "account";

@Component({
  selector: "app-settings",
  standalone: true,
  imports: [IconComponent, CommonModule, FormsModule, A11yModule, RouterModule, SelectComponent],
  templateUrl: "./settings.page.html",
  styleUrl: "./settings.page.css",
})
export class SettingsPage implements OnInit {
  private readonly themeService = inject(ThemeService);
  readonly section = input<SettingsSection>("general");
  private jiraStateService = inject(JiraStateService);
  private pontoMaisStateService = inject(PontoMaisStateService);
  private toastService = inject(ToastService);
  private appSettings = inject(AppSettingsService);
  private statusCatalog = inject(StatusCatalogStore);
  private localData = inject(LocalDataService);
  readonly update = inject(UpdateService);

  theme: Theme = "light";

  readonly syncIntervalOptions = SYNC_INTERVAL_OPTIONS;
  readonly manualSyncOnly = MANUAL_SYNC_ONLY;
  minSyncIntervalMinutes = this.appSettings.minSyncIntervalMinutes;
  syncIntervalSaving = false;

  readonly jiraState = this.jiraStateService.state;
  readonly jiraStateError = this.jiraStateService.error;
  readonly pontoMaisState = this.pontoMaisStateService.state;

  jiraLoading = false;
  jiraError?: string;

  pontoMaisLoading = false;
  pontoMaisError?: string;
  showPontoMaisForm = false;
  pontoMaisLogin = "";
  pontoMaisPassword = "";
  pontoMaisAutoReconnect = false;

  showLoginForm = false;
  jiraUrl = "";
  jiraEmail = "";
  jiraToken = "";

  jiraStatuses: JiraStatus[] = [];
  statusColumns: StatusColumn[] = [];
  statusesSyncing = false;
  statusesSaving = false;
  statusesDirty = false;
  statusesSyncedLabel = "";
  private savedDevelopment = new Map<string, boolean>();

  clearingLocalData = false;

  constructor() {
    const catalogChanged = changeTracker<unknown>();
    effect(() => {
      const statuses = this.statusCatalog.statuses();
      this.statusCatalog.syncedAt();
      if (catalogChanged(statuses)) this.applyStatuses(true);
    });
  }

  ngOnInit(): void {
    this.loadTheme();
    this.applyStatuses(false);
  }

  async setSyncInterval(minutes: number): Promise<void> {
    const previous = this.minSyncIntervalMinutes;
    this.minSyncIntervalMinutes = minutes;
    this.syncIntervalSaving = true;
    try {
      await this.appSettings.saveMinSyncInterval(minutes);
    } catch (err) {
      error(`failed to save min sync interval: ${err}`);
      this.minSyncIntervalMinutes = previous;
      this.toastService.error("Não foi possível salvar o intervalo de sincronização.");
    } finally {
      this.syncIntervalSaving = false;
    }
  }

  loadTheme(): void {
    this.theme = this.themeService.theme;
  }

  setTheme(theme: Theme): void {
    this.theme = theme;
    this.themeService.set(theme);
  }

  get hasOpenForm(): boolean {
    return this.showLoginForm || this.showPontoMaisForm;
  }

  @HostListener("document:keydown.escape")
  onEscape(): void {
    if (this.showLoginForm) this.closeLoginForm();
    if (this.showPontoMaisForm) this.closePontoMaisForm();
  }

  onOverlayClick(event: MouseEvent, modal: "jira" | "pontomais"): void {
    if (event.target !== event.currentTarget) return;
    if (modal === "jira") this.closeLoginForm();
    else this.closePontoMaisForm();
  }

  openLoginForm(): void {
    this.jiraError = undefined;
    this.showLoginForm = true;
  }

  closeLoginForm(): void {
    if (this.jiraLoading) return;
    this.showLoginForm = false;
    this.jiraToken = "";
    this.jiraError = undefined;
  }

  async connectJira(): Promise<void> {
    if (this.jiraLoading) return;
    this.jiraLoading = true;
    this.jiraError = undefined;

    try {
      await this.jiraStateService.login(this.jiraUrl, this.jiraEmail, this.jiraToken);
      this.showLoginForm = false;
      this.jiraUrl = "";
      this.jiraEmail = "";
      this.jiraToken = "";
    } catch (err) {
      this.jiraError = `Não foi possível conectar ao Jira: ${err}`;
      error(`failed to connect Jira: ${err}`);
    } finally {
      this.jiraLoading = false;
    }
  }

  async disconnectJira(): Promise<void> {
    if (!confirm("Deseja desconectar a conta do Jira?")) {
      return;
    }

    this.jiraLoading = true;
    this.jiraError = undefined;

    try {
      await this.jiraStateService.logout();
      this.jiraUrl = "";
      this.jiraEmail = "";
      this.jiraToken = "";
    } catch (err) {
      this.jiraError = "Não foi possível desconectar a conta do Jira.";
      error(`failed to disconnect Jira: ${err}`);
    } finally {
      this.jiraLoading = false;
    }
  }

  openPontoMaisForm(): void {
    this.pontoMaisError = undefined;
    this.pontoMaisLogin = this.pontoMaisState().account ?? "";
    this.pontoMaisPassword = "";
    this.pontoMaisAutoReconnect = this.pontoMaisState().autoReconnect;
    this.showPontoMaisForm = true;
  }

  closePontoMaisForm(): void {
    if (this.pontoMaisLoading) return;
    this.showPontoMaisForm = false;
    this.pontoMaisPassword = "";
    this.pontoMaisError = undefined;
  }

  get canConnectPontoMais(): boolean {
    return !this.pontoMaisLoading && this.pontoMaisLogin.trim().length > 0 && this.pontoMaisPassword.length > 0;
  }

  async connectPontoMais(): Promise<void> {
    if (!this.canConnectPontoMais) return;

    this.pontoMaisLoading = true;
    this.pontoMaisError = undefined;

    try {
      await this.pontoMaisStateService.connect(
        this.pontoMaisLogin,
        this.pontoMaisPassword,
        this.pontoMaisAutoReconnect,
      );
      this.showPontoMaisForm = false;
      this.toastService.success("Conta do PontoMais conectada.");
    } catch (err) {
      error(`failed to connect PontoMais: ${err}`);
      this.pontoMaisError = `Não foi possível conectar ao PontoMais: ${err}`;
    } finally {
      this.pontoMaisPassword = "";
      this.pontoMaisLoading = false;
    }
  }

  async disconnectPontoMais(): Promise<void> {
    if (!confirm("Deseja desconectar a conta do PontoMais?")) {
      return;
    }

    this.pontoMaisLoading = true;
    this.pontoMaisError = undefined;

    try {
      await this.pontoMaisStateService.disconnect();
    } catch (err) {
      this.pontoMaisError = "Não foi possível desconectar a conta do PontoMais.";
      error(`failed to disconnect PontoMais: ${err}`);
    } finally {
      this.pontoMaisLoading = false;
    }
  }

  async clearLocalData(): Promise<void> {
    if (
      !confirm(
        "Deseja apagar os dados locais? As tarefas, o histórico de status, os worklogs e as marcações de ponto armazenados neste computador serão removidos. As contas permanecerão conectadas.",
      )
    ) {
      return;
    }

    this.clearingLocalData = true;
    try {
      await preferencesCommands.clearLocalData();
      this.localData.markCleared();
      await this.statusCatalog.load();
      this.applyStatuses(false);
      this.toastService.success("Dados locais apagados.");
    } catch (err) {
      error(`failed to clear local data: ${err}`);
      this.toastService.error("Não foi possível apagar os dados locais.");
    } finally {
      this.clearingLocalData = false;
    }
  }

  async syncJiraStatusCatalog(): Promise<void> {
    this.statusesSyncing = true;
    try {
      await statusCommands.syncCatalog();
      await this.statusCatalog.load();
      this.applyStatuses(true);
    } catch (err) {
      error(`failed to sync Jira status catalog: ${err}`);
      this.toastService.error("Não foi possível sincronizar os status do Jira.");
    } finally {
      this.statusesSyncing = false;
    }
  }

  toggleStatusDevelopment(status: JiraStatus, checked: boolean): void {
    status.isDevelopment = checked;
    this.checkStatusesDirty();
  }

  checkStatusesDirty(): void {
    this.statusesDirty = this.pendingStatusUpdates().length > 0;
  }

  async saveStatusChanges(): Promise<void> {
    const updates = this.pendingStatusUpdates();
    if (!updates.length) return;

    this.statusesSaving = true;
    try {
      await statusCommands.updateIsDevelopment(updates);
      updates.forEach((u) => this.savedDevelopment.set(u.statusId, u.isDevelopment));
      this.statusCatalog.markDevelopment(updates);
      this.checkStatusesDirty();
      this.toastService.success("Status de desenvolvimento salvos.");
    } catch (err) {
      error(`failed to save development statuses: ${err}`);
      this.toastService.error("Não foi possível salvar os status de desenvolvimento.");
    } finally {
      this.statusesSaving = false;
    }
  }

  statusBadge(status: JiraStatus): string {
    return STATUS_CATEGORIES[status.categoryKey]?.badge ?? "info";
  }

  trackStatus(_: number, status: JiraStatus): string {
    return status.id;
  }

  private applyStatuses(keepDrafts: boolean): void {
    const drafts = new Map<string, boolean>();
    if (keepDrafts) {
      for (const status of this.jiraStatuses) {
        if (this.savedDevelopment.get(status.id) !== status.isDevelopment) {
          drafts.set(status.id, status.isDevelopment);
        }
      }
    }

    const statuses = this.statusCatalog.statuses().map((s) => ({ ...s }));
    const syncedAt = this.statusCatalog.syncedAt();

    this.savedDevelopment = new Map(statuses.map((s) => [s.id, s.isDevelopment]));
    for (const status of statuses) {
      const draft = drafts.get(status.id);
      if (draft !== undefined) status.isDevelopment = draft;
    }

    this.jiraStatuses = [...statuses].sort(
      (a, b) => this.categoryOrder(a) - this.categoryOrder(b) || a.name.localeCompare(b.name, "pt-BR"),
    );
    this.statusColumns = this.splitColumns(this.jiraStatuses, STATUS_COLUMNS);
    this.statusesSyncedLabel = syncedAt ? `Atualizado ${relativeDateTime(syncedAt, this.appSettings.timeZone)}` : "";
    this.checkStatusesDirty();
  }

  private pendingStatusUpdates(): { statusId: string; isDevelopment: boolean }[] {
    return this.jiraStatuses
      .filter((s) => this.savedDevelopment.get(s.id) !== s.isDevelopment)
      .map((s) => ({ statusId: s.id, isDevelopment: s.isDevelopment }));
  }

  private categoryOrder(status: JiraStatus): number {
    return STATUS_CATEGORIES[status.categoryKey]?.order ?? 3;
  }

  private splitColumns(items: JiraStatus[], count: number): StatusColumn[] {
    const base = Math.floor(items.length / count);
    const extra = items.length % count;
    const columns: JiraStatus[][] = [];
    let index = 0;
    for (let c = 0; c < count; c++) {
      const size = base + (c < extra ? 1 : 0);
      columns.push(items.slice(index, index + size));
      index += size;
    }
    const tallest = columns[0]?.length ?? 0;
    return columns.map((statuses) => ({
      statuses,
      fillers: Array.from({ length: tallest - statuses.length }, (_, i) => i),
    }));
  }
}
