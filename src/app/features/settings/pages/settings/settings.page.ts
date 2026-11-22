import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { A11yModule } from "@angular/cdk/a11y";
import { error } from "@tauri-apps/plugin-log";
import { JiraStateService } from "../../../../core/auth/jira-state.service";
import { PontoMaisStateService } from "../../../../core/pontomais/pontomais-state.service";
import { AppPage, JiraStatus, linkCommands, preferencesCommands, statusCommands } from "../../../../core/tauri";
import { getTauriVersion } from "@tauri-apps/api/app";
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

export type SettingsSection = "general" | "account" | "about";

const APP_DESCRIPTION =
  "Assistente desktop para o Jira Cloud que calcula o tempo trabalhado em cada tarefa a partir das marcações de ponto do PontoMais.";
const APP_AUTHOR = "Gabriel Griffo";

@Component({
  selector: "app-settings",
  imports: [IconComponent, FormsModule, A11yModule, SelectComponent],
  templateUrl: "./settings.page.html",
  styleUrl: "./settings.page.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    "(document:keydown.escape)": "onEscape()",
  },
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

  readonly appDescription = APP_DESCRIPTION;
  readonly appAuthor = APP_AUTHOR;
  readonly tauriVersion = signal("");

  readonly theme = signal<Theme>("light");

  readonly syncIntervalOptions = SYNC_INTERVAL_OPTIONS;
  readonly manualSyncOnly = MANUAL_SYNC_ONLY;
  readonly minSyncIntervalMinutes = signal(this.appSettings.minSyncIntervalMinutes);
  readonly syncIntervalSaving = signal(false);

  readonly jiraState = this.jiraStateService.state;
  readonly jiraStateError = this.jiraStateService.error;
  readonly pontoMaisState = this.pontoMaisStateService.state;

  readonly jiraLoading = signal(false);
  readonly jiraError = signal<string | undefined>(undefined);

  readonly pontoMaisLoading = signal(false);
  readonly pontoMaisError = signal<string | undefined>(undefined);
  readonly showPontoMaisForm = signal(false);
  readonly pontoMaisLogin = signal("");
  readonly pontoMaisPassword = signal("");
  readonly pontoMaisAutoReconnect = signal(false);
  readonly canConnectPontoMais = computed(
    () => !this.pontoMaisLoading() && this.pontoMaisLogin().trim().length > 0 && this.pontoMaisPassword().length > 0,
  );

  readonly showLoginForm = signal(false);
  readonly jiraUrl = signal("");
  readonly jiraEmail = signal("");
  readonly jiraToken = signal("");

  readonly jiraStatuses = signal<JiraStatus[]>([]);
  readonly statusColumns = signal<StatusColumn[]>([]);
  readonly statusesSyncing = signal(false);
  readonly statusesSaving = signal(false);
  readonly statusesDirty = signal(false);
  readonly statusesSyncedLabel = signal("");
  readonly clearingLocalData = signal(false);

  private savedDevelopment = new Map<string, boolean>();

  constructor() {
    const catalogChanged = changeTracker<unknown>();
    effect(() => {
      const statuses = this.statusCatalog.statuses();
      this.statusCatalog.syncedAt();
      if (catalogChanged(statuses)) untracked(() => this.applyStatuses(true));
    });
  }

  ngOnInit(): void {
    getTauriVersion()
      .then((version) => this.tauriVersion.set(version))
      .catch((err) => error(`failed to read Tauri version: ${err}`));
    this.loadTheme();
    this.applyStatuses(false);
  }

  async setSyncInterval(minutes: number): Promise<void> {
    const previous = this.minSyncIntervalMinutes();
    this.minSyncIntervalMinutes.set(minutes);
    this.syncIntervalSaving.set(true);
    try {
      await this.appSettings.saveMinSyncInterval(minutes);
    } catch (err) {
      error(`failed to save min sync interval: ${err}`);
      this.minSyncIntervalMinutes.set(previous);
      this.toastService.error("Não foi possível salvar o intervalo de sincronização.");
    } finally {
      this.syncIntervalSaving.set(false);
    }
  }

  openAppPage(page: AppPage): void {
    linkCommands.openAppPage(page).catch((err) => {
      error(`failed to open ${page} page: ${err}`);
      this.toastService.error("Não foi possível abrir o navegador.");
    });
  }

  loadTheme(): void {
    this.theme.set(this.themeService.theme);
  }

  setTheme(theme: Theme): void {
    this.theme.set(theme);
    this.themeService.set(theme);
  }

  get hasOpenForm(): boolean {
    return this.showLoginForm() || this.showPontoMaisForm();
  }

  onEscape(): void {
    if (this.showLoginForm()) this.closeLoginForm();
    if (this.showPontoMaisForm()) this.closePontoMaisForm();
  }

  onOverlayClick(event: MouseEvent, modal: "jira" | "pontomais"): void {
    if (event.target !== event.currentTarget) return;
    if (modal === "jira") this.closeLoginForm();
    else this.closePontoMaisForm();
  }

  openLoginForm(): void {
    this.jiraError.set(undefined);
    this.showLoginForm.set(true);
  }

  closeLoginForm(): void {
    if (this.jiraLoading()) return;
    this.showLoginForm.set(false);
    this.jiraToken.set("");
    this.jiraError.set(undefined);
  }

  async connectJira(): Promise<void> {
    if (this.jiraLoading()) return;
    this.jiraLoading.set(true);
    this.jiraError.set(undefined);

    try {
      await this.jiraStateService.login(this.jiraUrl(), this.jiraEmail(), this.jiraToken());
      this.showLoginForm.set(false);
      this.jiraUrl.set("");
      this.jiraEmail.set("");
      this.jiraToken.set("");
    } catch (err) {
      this.jiraError.set(`Não foi possível conectar ao Jira: ${err}`);
      error(`failed to connect Jira: ${err}`);
    } finally {
      this.jiraLoading.set(false);
    }
  }

  async disconnectJira(): Promise<void> {
    if (!confirm("Deseja desconectar a conta do Jira?")) {
      return;
    }

    this.jiraLoading.set(true);
    this.jiraError.set(undefined);

    try {
      await this.jiraStateService.logout();
      this.jiraUrl.set("");
      this.jiraEmail.set("");
      this.jiraToken.set("");
    } catch (err) {
      this.jiraError.set("Não foi possível desconectar a conta do Jira.");
      error(`failed to disconnect Jira: ${err}`);
    } finally {
      this.jiraLoading.set(false);
    }
  }

  openPontoMaisForm(): void {
    this.pontoMaisError.set(undefined);
    this.pontoMaisLogin.set(this.pontoMaisState().account ?? "");
    this.pontoMaisPassword.set("");
    this.pontoMaisAutoReconnect.set(this.pontoMaisState().autoReconnect);
    this.showPontoMaisForm.set(true);
  }

  closePontoMaisForm(): void {
    if (this.pontoMaisLoading()) return;
    this.showPontoMaisForm.set(false);
    this.pontoMaisPassword.set("");
    this.pontoMaisError.set(undefined);
  }

  async connectPontoMais(): Promise<void> {
    if (!this.canConnectPontoMais()) return;

    this.pontoMaisLoading.set(true);
    this.pontoMaisError.set(undefined);

    try {
      await this.pontoMaisStateService.connect(
        this.pontoMaisLogin(),
        this.pontoMaisPassword(),
        this.pontoMaisAutoReconnect(),
      );
      this.showPontoMaisForm.set(false);
      this.toastService.success("Conta do PontoMais conectada.");
    } catch (err) {
      error(`failed to connect PontoMais: ${err}`);
      this.pontoMaisError.set(`Não foi possível conectar ao PontoMais: ${err}`);
    } finally {
      this.pontoMaisPassword.set("");
      this.pontoMaisLoading.set(false);
    }
  }

  async disconnectPontoMais(): Promise<void> {
    if (!confirm("Deseja desconectar a conta do PontoMais?")) {
      return;
    }

    this.pontoMaisLoading.set(true);
    this.pontoMaisError.set(undefined);

    try {
      await this.pontoMaisStateService.disconnect();
    } catch (err) {
      this.pontoMaisError.set("Não foi possível desconectar a conta do PontoMais.");
      error(`failed to disconnect PontoMais: ${err}`);
    } finally {
      this.pontoMaisLoading.set(false);
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

    this.clearingLocalData.set(true);
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
      this.clearingLocalData.set(false);
    }
  }

  async syncJiraStatusCatalog(): Promise<void> {
    this.statusesSyncing.set(true);
    try {
      await statusCommands.syncCatalog();
      await this.statusCatalog.load();
      this.applyStatuses(true);
    } catch (err) {
      error(`failed to sync Jira status catalog: ${err}`);
      this.toastService.error("Não foi possível sincronizar os status do Jira.");
    } finally {
      this.statusesSyncing.set(false);
    }
  }

  toggleStatusDevelopment(status: JiraStatus, checked: boolean): void {
    status.isDevelopment = checked;
    this.checkStatusesDirty();
  }

  checkStatusesDirty(): void {
    this.statusesDirty.set(this.pendingStatusUpdates().length > 0);
  }

  async saveStatusChanges(): Promise<void> {
    const updates = this.pendingStatusUpdates();
    if (!updates.length) return;

    this.statusesSaving.set(true);
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
      this.statusesSaving.set(false);
    }
  }

  statusBadge(status: JiraStatus): string {
    return STATUS_CATEGORIES[status.categoryKey]?.badge ?? "info";
  }

  private applyStatuses(keepDrafts: boolean): void {
    const drafts = new Map<string, boolean>();
    if (keepDrafts) {
      for (const status of this.jiraStatuses()) {
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

    this.jiraStatuses.set(
      [...statuses].sort(
        (a, b) => this.categoryOrder(a) - this.categoryOrder(b) || a.name.localeCompare(b.name, "pt-BR"),
      ),
    );
    this.statusColumns.set(this.splitColumns(this.jiraStatuses(), STATUS_COLUMNS));
    this.statusesSyncedLabel.set(syncedAt ? `Atualizado ${relativeDateTime(syncedAt, this.appSettings.timeZone)}` : "");
    this.checkStatusesDirty();
  }

  private pendingStatusUpdates(): { statusId: string; isDevelopment: boolean }[] {
    return this.jiraStatuses()
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
