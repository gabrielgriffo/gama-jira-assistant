import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { A11yModule } from "@angular/cdk/a11y";
import { error } from "@tauri-apps/plugin-log";
import { JiraStateService } from "../../core/auth/jira-state.service";
import { PontoMaisStateService } from "../../core/pontomais/pontomais-state.service";
import { AppSettingsService } from "../../core/preferences/app-settings.service";
import { IconComponent } from "../../shared/ui/icon/icon.component";
import { ThemePickerComponent } from "../../shared/ui/theme-picker/theme-picker.component";
import { DateRangePickerComponent } from "../../shared/ui/date-range-picker/date-range-picker.component";
import { brDate, daysBetween, monthOf, rangeLabel } from "../../shared/date/local-date";
import { RangeSelection } from "../../shared/date/range-presets";
import { FirstImportService } from "./first-import.service";
import { OnboardingService } from "./onboarding.service";
import {
  ChecklistItem,
  ChecklistState,
  OnboardingChecklistComponent,
  SetupStep,
} from "./checklist/onboarding-checklist.component";
import {
  defaultImportRange,
  earliestImportStart,
  importStepText,
  resumeStep,
  ImportStep,
  isValidImportRange,
  SKIP_IMPACT,
  skipAllImpact,
  SkippableStep,
} from "./onboarding-plan";

const ORDER: SetupStep[] = ["theme", "jira", "pontomais", "date", "import"];

@Component({
  selector: "app-onboarding",
  imports: [
    FormsModule,
    A11yModule,
    IconComponent,
    ThemePickerComponent,
    DateRangePickerComponent,
    OnboardingChecklistComponent,
  ],
  templateUrl: "./onboarding.component.html",
  styleUrl: "./onboarding.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    "(document:keydown.escape)": "cancelSkip()",
  },
})
export class OnboardingComponent {
  private readonly jiraState = inject(JiraStateService);
  private readonly pontoMaisState = inject(PontoMaisStateService);
  private readonly settings = inject(AppSettingsService);
  private readonly onboarding = inject(OnboardingService);
  readonly firstImport = inject(FirstImportService);

  readonly step = signal<SetupStep>(
    resumeStep(this.onboarding.themeSeen, this.jiraState.state().connected, this.pontoMaisState.state().connected),
  );
  readonly skipped = signal<ReadonlySet<SetupStep>>(new Set());
  readonly confirmSkip = signal<SkippableStep | "all" | null>(null);
  readonly finishing = signal(false);
  protected readonly stepped = signal(false);
  private readonly themeSeen = signal(this.onboarding.themeSeen);
  readonly stepCount = ORDER.length;

  readonly jiraConnected = computed(() => this.jiraState.state().connected);
  readonly pontoMaisConnected = computed(() => this.pontoMaisState.state().connected);
  readonly anyAccount = computed(() => this.jiraConnected() || this.pontoMaisConnected());
  readonly jiraAccount = computed(() => this.jiraState.state().email ?? "sua conta");
  readonly pontoMaisAccount = computed(() => this.pontoMaisState.state().account ?? "sua conta");

  readonly jiraUrl = signal("");
  readonly jiraEmail = signal("");
  readonly jiraToken = signal("");
  readonly jiraLoading = signal(false);
  readonly jiraError = signal("");
  readonly canConnectJira = computed(
    () =>
      !this.jiraLoading() &&
      this.jiraUrl().trim().length > 0 &&
      this.jiraEmail().trim().length > 0 &&
      this.jiraToken().length > 0,
  );

  readonly pontoMaisLogin = signal("");
  readonly pontoMaisPassword = signal("");
  readonly pontoMaisAutoReconnect = signal(false);
  readonly pontoMaisLoading = signal(false);
  readonly pontoMaisError = signal("");
  readonly canConnectPontoMais = computed(
    () => !this.pontoMaisLoading() && this.pontoMaisLogin().trim().length > 0 && this.pontoMaisPassword().length > 0,
  );

  readonly today = this.settings.today;
  readonly timeZone = computed(() => this.settings.timeZone);
  readonly minStart = computed(() => earliestImportStart(this.today()));
  readonly minMonth = computed(() => monthOf(this.minStart()));
  readonly importRange = signal<RangeSelection>(defaultImportRange(this.settings.today()));
  readonly validRange = computed(() => isValidImportRange(this.importRange(), this.today()));
  readonly importDays = computed(() =>
    this.validRange() ? daysBetween(this.importRange().from, this.importRange().to) : 0,
  );

  readonly importStarted = computed(() => this.firstImport.phase() !== "idle");
  readonly skipImpact = computed(() => {
    const step = this.confirmSkip();
    if (step === "all") return skipAllImpact(this.jiraConnected(), this.pontoMaisConnected());
    return step ? SKIP_IMPACT[step] : null;
  });

  readonly completed = computed<ReadonlySet<SetupStep>>(() => {
    const done = new Set<SetupStep>(this.skipped());
    if (this.themeSeen()) done.add("theme");
    if (this.jiraConnected()) done.add("jira");
    if (this.pontoMaisConnected()) done.add("pontomais");
    if (this.importStarted()) done.add("date");
    const phase = this.firstImport.phase();
    const nothingToImport = this.step() === "import" && phase === "idle";
    if (nothingToImport || (phase === "finished" && !this.firstImport.hasFailures())) done.add("import");
    return done;
  });
  readonly completedCount = computed(() => this.completed().size);

  readonly checklist = computed<ChecklistItem[]>(() => {
    const current = this.step();
    const skipped = this.skipped();
    const locked = this.importStarted();
    const items: Omit<ChecklistItem, "state" | "enabled">[] = [
      { id: "theme", label: "Aparência", detail: "Tema do aplicativo" },
      {
        id: "jira",
        label: "Conta do Jira",
        detail: this.jiraConnected() ? (this.jiraState.state().email ?? "Conectada") : "Tarefas e worklogs",
      },
      {
        id: "pontomais",
        label: "Conta do PontoMais",
        detail: this.pontoMaisConnected() ? (this.pontoMaisState.state().account ?? "Conectada") : "Marcações de ponto",
      },
      {
        id: "date",
        label: "Primeira importação",
        detail: this.importStarted() ? rangeLabel(this.importRange()) : "Período dos dados",
      },
      { id: "import", label: "Importação", detail: this.importDetail() },
    ];
    return items.map((item) => {
      const done = this.completed().has(item.id) && !skipped.has(item.id);
      const state: ChecklistState =
        item.id === current ? "current" : done ? "done" : skipped.has(item.id) ? "skipped" : "pending";
      const enabled = item.id !== "import" && !locked && item.id !== current;
      return { ...item, state, enabled };
    });
  });

  stepText(step: ImportStep): string {
    return importStepText(step);
  }

  brDate(date: string): string {
    return brDate(date);
  }

  goTo(step: SetupStep): void {
    if (this.importStarted() || step === "import") return;
    this.moveTo(step);
  }

  next(): void {
    const index = ORDER.indexOf(this.step());
    this.moveTo(ORDER[Math.min(index + 1, ORDER.length - 1)]);
  }

  private moveTo(step: SetupStep): void {
    if (this.step() === "theme") {
      this.themeSeen.set(true);
      this.onboarding.markThemeSeen();
    }
    this.stepped.set(true);
    this.step.set(step);
  }

  askSkip(step: SkippableStep | "all"): void {
    this.confirmSkip.set(step);
  }

  cancelSkip(): void {
    this.confirmSkip.set(null);
  }

  confirmSkipStep(): void {
    const step = this.confirmSkip();
    if (!step) return;
    this.confirmSkip.set(null);
    if (step === "all") {
      void this.finish();
      return;
    }
    this.skipped.update((set) => new Set(set).add(step));
    this.next();
  }

  async connectJira(): Promise<void> {
    if (!this.canConnectJira()) return;
    this.jiraLoading.set(true);
    this.jiraError.set("");
    try {
      await this.jiraState.login(this.jiraUrl(), this.jiraEmail(), this.jiraToken());
      this.jiraToken.set("");
      this.unskip("jira");
      this.next();
    } catch (err) {
      error(`failed to connect Jira during onboarding: ${err}`);
      this.jiraError.set(`Não foi possível conectar ao Jira: ${err}`);
    } finally {
      this.jiraLoading.set(false);
    }
  }

  async disconnectJira(): Promise<void> {
    if (this.jiraLoading() || !confirm("Deseja desconectar a conta do Jira?")) return;
    this.jiraLoading.set(true);
    this.jiraError.set("");
    try {
      await this.jiraState.logout();
    } catch (err) {
      error(`failed to disconnect Jira during onboarding: ${err}`);
      this.jiraError.set("Não foi possível desconectar a conta do Jira.");
    } finally {
      this.jiraLoading.set(false);
    }
  }

  async connectPontoMais(): Promise<void> {
    if (!this.canConnectPontoMais()) return;
    this.pontoMaisLoading.set(true);
    this.pontoMaisError.set("");
    try {
      await this.pontoMaisState.connect(this.pontoMaisLogin(), this.pontoMaisPassword(), this.pontoMaisAutoReconnect());
      if (!this.pontoMaisState.state().connected) throw new Error("a conexão não foi confirmada");
      this.unskip("pontomais");
      this.next();
    } catch (err) {
      error(`failed to connect PontoMais during onboarding: ${err}`);
      this.pontoMaisError.set(`Não foi possível conectar ao PontoMais: ${err}`);
    } finally {
      this.pontoMaisPassword.set("");
      this.pontoMaisLoading.set(false);
    }
  }

  async disconnectPontoMais(): Promise<void> {
    if (this.pontoMaisLoading() || !confirm("Deseja desconectar a conta do PontoMais?")) return;
    this.pontoMaisLoading.set(true);
    this.pontoMaisError.set("");
    try {
      await this.pontoMaisState.disconnect();
    } catch (err) {
      error(`failed to disconnect PontoMais during onboarding: ${err}`);
      this.pontoMaisError.set("Não foi possível desconectar a conta do PontoMais.");
    } finally {
      this.pontoMaisLoading.set(false);
    }
  }

  startImport(): void {
    if (!this.validRange() || !this.anyAccount()) return;
    this.unskip("date");
    this.stepped.set(true);
    this.step.set("import");
    const { from, to } = this.importRange();
    void this.firstImport.start({ from, to }, this.jiraConnected(), this.pontoMaisConnected());
  }

  retryImport(): void {
    void this.firstImport.retry();
  }

  continueInBackground(): void {
    this.firstImport.continueInBackground();
    void this.finish();
  }

  async finish(): Promise<void> {
    if (this.finishing()) return;
    this.finishing.set(true);
    await this.onboarding.finish();
  }

  private importDetail(): string {
    const phase = this.firstImport.phase();
    if (phase === "running") return "Em andamento";
    if (phase === "finished") return this.firstImport.hasFailures() ? "Concluída com erros" : "Concluída";
    if (this.skipped().has("date") || (this.step() === "import" && !this.anyAccount())) return "Não realizada";
    return "Dados do Jira e do ponto";
  }

  private unskip(step: SetupStep): void {
    this.skipped.update((set) => {
      const next = new Set(set);
      next.delete(step);
      return next;
    });
  }
}
