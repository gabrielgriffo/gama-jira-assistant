import { Component, OnInit, computed, inject } from "@angular/core";
import { RouterOutlet, RouterLink, RouterLinkActive } from "@angular/router";
import { CommonModule } from "@angular/common";
import { JiraStateService } from "./core/auth/jira-state.service";
import { PontoMaisStateService } from "./core/pontomais/pontomais-state.service";
import { SyncStateService } from "./core/sync/sync-state.service";
import { TitlebarComponent } from "./core/layout/titlebar/titlebar.component";
import { SettingsDialogComponent } from "./features/settings/dialogs/settings-dialog/settings-dialog.component";
import { SettingsDialogService } from "./features/settings/settings-dialog.service";
import { IconComponent } from "./shared/ui/icon/icon.component";
import { ToastComponent } from "./shared/ui/toast/toast.component";
import { ThemeService } from "./core/preferences/theme.service";

@Component({
  selector: "app-root",
  imports: [
    IconComponent,
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    CommonModule,
    TitlebarComponent,
    SettingsDialogComponent,
    ToastComponent,
  ],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnInit {
  private jiraStateService = inject(JiraStateService);
  private pontoMaisStateService = inject(PontoMaisStateService);
  private syncStateService = inject(SyncStateService);
  readonly settingsDialog = inject(SettingsDialogService);
  private readonly theme = inject(ThemeService);

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

  ngOnInit() {
    this.theme.initialize();
    this.initStateServices();
  }

  initStateServices() {
    this.jiraStateService.initialize();
    this.pontoMaisStateService.initialize();
    this.syncStateService.initialize();

    this.jiraStateService.refresh();
    this.pontoMaisStateService.refresh();
  }
}
