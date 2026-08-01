import { Component, HostListener, OnInit, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { A11yModule } from "@angular/cdk/a11y";
import { RouterModule } from "@angular/router";
import { error } from "@tauri-apps/plugin-log";
import { JiraStateService } from "../../services/jira-state.service";
import { PontoMaisStateService } from "../../services/pontomais-state.service";
import { JiraStatus, preferencesCommands, statusCommands } from "../../core/tauri";
import { ToastService } from "../../core/feedback/toast.service";
import { ToastComponent } from "../../shared/ui/toast/toast.component";
import { SelectComponent, SelectOption } from "../../shared/ui/select/select.component";
import { localTimeZone, relativeDateTime } from "../../shared/date/local-date";

const STATUS_COLUMNS = 3;

const SYNC_INTERVAL_OPTIONS: SelectOption<number>[] = [
  { value: 0, label: "Sempre sincronizar" },
  { value: 1, label: "1 minuto" },
  { value: 5, label: "5 minutos" },
  { value: 15, label: "15 minutos" },
  { value: 30, label: "30 minutos" },
  { value: 60, label: "1 hora" },
];

const STATUS_CATEGORIES: Record<string, { label: string; color: string; order: number }> = {
  new: { label: "A fazer", color: "var(--cat-new)", order: 0 },
  indeterminate: { label: "Em andamento", color: "var(--cat-ind)", order: 1 },
  done: { label: "Concluído", color: "var(--cat-done)", order: 2 },
};

interface StatusColumn {
  statuses: JiraStatus[];
  fillers: number[];
}

@Component({
  selector: "app-settings",
  standalone: true,
  imports: [CommonModule, FormsModule, A11yModule, RouterModule, ToastComponent, SelectComponent],
  templateUrl: "./settings.page.html",
  styleUrl: "./settings.page.css",
})
export class SettingsPage implements OnInit {
  private jiraStateService = inject(JiraStateService);
  private pontoMaisStateService = inject(PontoMaisStateService);
  private toastService = inject(ToastService);

  theme: "light" | "dark" | "system" = "light";

  readonly syncIntervalOptions = SYNC_INTERVAL_OPTIONS;
  minSyncIntervalMinutes = 5;
  syncIntervalSaving = false;

  readonly jiraState = this.jiraStateService.state;
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
  statusesLoading = false;
  statusesSyncing = false;
  statusesSaving = false;
  statusesDirty = false;
  statusesSyncedLabel = "";
  private savedDevelopment = new Map<string, boolean>();

  clearingLocalData = false;

  ngOnInit(): void {
    this.loadTheme();
    this.loadSyncInterval();
    this.loadJiraStatuses();
  }

  async loadSyncInterval(): Promise<void> {
    try {
      const preferences = await preferencesCommands.getPreferences();
      this.minSyncIntervalMinutes = preferences.minSyncIntervalMinutes;
    } catch (err) {
      error(`failed to load min sync interval: ${err}`);
    }
  }

  async setSyncInterval(minutes: number): Promise<void> {
    const previous = this.minSyncIntervalMinutes;
    this.minSyncIntervalMinutes = minutes;
    this.syncIntervalSaving = true;
    try {
      await preferencesCommands.updateMinSyncInterval(minutes);
    } catch (err) {
      error(`failed to save min sync interval: ${err}`);
      this.minSyncIntervalMinutes = previous;
      this.toastService.error("Não foi possível salvar o intervalo de sincronização.");
    } finally {
      this.syncIntervalSaving = false;
    }
  }

  loadTheme(): void {
    try {
      const saved = localStorage.getItem("gama-theme");
      if (saved === "light" || saved === "dark" || saved === "system") {
        this.theme = saved;
      }
    } catch (e) {}
  }

  setTheme(theme: "light" | "dark" | "system"): void {
    this.theme = theme;
    if (theme === "system") {
      document.documentElement.removeAttribute("data-theme");
    } else {
      document.documentElement.setAttribute("data-theme", theme);
    }
    try {
      localStorage.setItem("gama-theme", theme);
    } catch (e) {}
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
    } catch (error) {
      this.jiraError = "Não foi possível conectar ao Jira. Verifique a URL, o e-mail e o token.";
      console.error("Erro ao fazer login:", error);
    } finally {
      this.jiraLoading = false;
    }
  }

  async disconnectJira(): Promise<void> {
    if (!confirm("Deseja desconectar do Jira?")) {
      return;
    }

    this.jiraLoading = true;
    this.jiraError = undefined;

    try {
      await this.jiraStateService.logout();
      this.jiraUrl = "";
      this.jiraEmail = "";
      this.jiraToken = "";
    } catch (error) {
      this.jiraError = "Não foi possível sair da conta.";
      console.error("Erro ao fazer logout:", error);
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
      await this.pontoMaisStateService.connect(this.pontoMaisLogin, this.pontoMaisPassword, this.pontoMaisAutoReconnect);
      this.showPontoMaisForm = false;
      this.toastService.success("PontoMais conectado.");
    } catch (err) {
      error(`failed to connect PontoMais: ${err}`);
      this.pontoMaisError = `Não foi possível conectar ao PontoMais: ${err}`;
    } finally {
      this.pontoMaisPassword = "";
      this.pontoMaisLoading = false;
    }
  }

  async disconnectPontoMais(): Promise<void> {
    if (!confirm("Deseja desconectar do PontoMais?")) {
      return;
    }

    this.pontoMaisLoading = true;
    this.pontoMaisError = undefined;

    try {
      await this.pontoMaisStateService.disconnect();
    } catch (error) {
      this.pontoMaisError = "Não foi possível desconectar do PontoMais.";
      console.error(error);
    } finally {
      this.pontoMaisLoading = false;
    }
  }

  async clearLocalData(): Promise<void> {
    if (
      !confirm(
        "Apagar os dados locais? Tarefas, histórico de status, worklogs e batidas de ponto em cache serão removidos deste computador. As contas continuam conectadas.",
      )
    ) {
      return;
    }

    this.clearingLocalData = true;
    try {
      await preferencesCommands.clearLocalData();
      await this.refreshStatuses(false);
      this.toastService.success("Dados locais apagados.");
    } catch (err) {
      error(`failed to clear local data: ${err}`);
      this.toastService.error("Não foi possível apagar os dados locais.");
    } finally {
      this.clearingLocalData = false;
    }
  }

  async loadJiraStatuses(): Promise<void> {
    this.statusesLoading = true;
    try {
      await this.refreshStatuses(false);
    } catch (err) {
      error(`failed to load Jira status catalog: ${err}`);
      this.toastService.error("Não foi possível carregar os status do Jira.");
    } finally {
      this.statusesLoading = false;
    }
  }

  async syncJiraStatusCatalog(): Promise<void> {
    this.statusesSyncing = true;
    try {
      await statusCommands.syncCatalog();
      await this.refreshStatuses(true);
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
      this.checkStatusesDirty();
      this.toastService.success("Configuração de status salva.");
    } catch (err) {
      error(`failed to save development statuses: ${err}`);
      this.toastService.error("Não foi possível salvar a configuração de status.");
    } finally {
      this.statusesSaving = false;
    }
  }

  statusColor(status: JiraStatus): string {
    return STATUS_CATEGORIES[status.categoryKey]?.color ?? "var(--cat-new)";
  }

  statusTitle(status: JiraStatus): string {
    const category = STATUS_CATEGORIES[status.categoryKey]?.label ?? status.categoryName;
    return `${status.name} (${category})`;
  }

  trackStatus(_: number, status: JiraStatus): string {
    return status.id;
  }

  private async refreshStatuses(keepDrafts: boolean): Promise<void> {
    const drafts = new Map<string, boolean>();
    if (keepDrafts) {
      for (const status of this.jiraStatuses) {
        if (this.savedDevelopment.get(status.id) !== status.isDevelopment) {
          drafts.set(status.id, status.isDevelopment);
        }
      }
    }

    const [statuses, syncedAt] = await Promise.all([statusCommands.getCatalog(), statusCommands.getCatalogSyncedAt()]);

    this.savedDevelopment = new Map(statuses.map((s) => [s.id, s.isDevelopment]));
    for (const status of statuses) {
      const draft = drafts.get(status.id);
      if (draft !== undefined) status.isDevelopment = draft;
    }

    this.jiraStatuses = [...statuses].sort(
      (a, b) => this.categoryOrder(a) - this.categoryOrder(b) || a.name.localeCompare(b.name, "pt-BR"),
    );
    this.statusColumns = this.splitColumns(this.jiraStatuses, STATUS_COLUMNS);
    this.statusesSyncedLabel = syncedAt ? `Atualizado ${relativeDateTime(syncedAt, localTimeZone())}` : "";
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
