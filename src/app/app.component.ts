import { DOCUMENT } from "@angular/common";
import { ChangeDetectionStrategy, Component, OnInit, computed, inject } from "@angular/core";
import { RouterOutlet, RouterLink, RouterLinkActive } from "@angular/router";

import { JiraStateService } from "./core/auth/jira-state.service";
import { trackInputModality } from "./core/input/input-modality";
import { PontoMaisStateService } from "./core/pontomais/pontomais-state.service";
import { SyncStateService } from "./core/sync/sync-state.service";
import { TitlebarComponent } from "./core/layout/titlebar/titlebar.component";
import { SettingsDialogComponent } from "./features/settings/dialogs/settings-dialog/settings-dialog.component";
import { SettingsDialogService } from "./features/settings/settings-dialog.service";
import { IconComponent } from "./shared/ui/icon/icon.component";
import { ToastComponent } from "./shared/ui/toast/toast.component";
import { TruncationTooltipComponent } from "./shared/ui/truncation-tooltip/truncation-tooltip.component";
import { ThemeService } from "./core/preferences/theme.service";
import { UpdateService } from "./core/update/update.service";
import { OnboardingComponent } from "./features/onboarding/onboarding.component";
import { OnboardingService } from "./features/onboarding/onboarding.service";

@Component({
  selector: "app-root",
  imports: [
    IconComponent,
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    TitlebarComponent,
    SettingsDialogComponent,
    ToastComponent,
    TruncationTooltipComponent,
    OnboardingComponent,
  ],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppComponent implements OnInit {
  private jiraStateService = inject(JiraStateService);
  private pontoMaisStateService = inject(PontoMaisStateService);
  private syncStateService = inject(SyncStateService);
  readonly settingsDialog = inject(SettingsDialogService);
  private readonly theme = inject(ThemeService);
  readonly update = inject(UpdateService);
  readonly onboarding = inject(OnboardingService);

  readonly userName = computed(() => {
    const state = this.jiraStateService.state();
    if (!state.connected) return "Jira desconectado";
    return state.user?.displayName || state.email || "Conectado ao Jira";
  });

  readonly jiraInstance = computed(() => {
    const state = this.jiraStateService.state();
    if (!state.connected || !state.instance) return "Conecte-se em Configurações";
    return state.instance.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  });

  constructor() {
    trackInputModality(inject(DOCUMENT));
  }

  ngOnInit() {
    this.theme.initialize();
    this.initStateServices();
    void this.update.initialize();
  }

  initStateServices() {
    this.jiraStateService.initialize();
    this.pontoMaisStateService.initialize();
    this.syncStateService.initialize();

    void Promise.all([this.jiraStateService.refresh(), this.pontoMaisStateService.refresh()]).then(() =>
      this.onboarding.decide(this.jiraStateService.state().connected, this.pontoMaisStateService.state().connected),
    );
  }
}
