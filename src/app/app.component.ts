import { Component, OnInit, computed, inject } from "@angular/core";
import { RouterOutlet, RouterLink, RouterLinkActive, Router, NavigationEnd } from "@angular/router";
import { CommonModule } from "@angular/common";
import { filter } from "rxjs/operators";
import { JiraStateService } from "./services/jira-state.service";
import { PontoMaisStateService } from "./services/pontomais-state.service";
import { SyncStateService } from "./services/sync-state.service";

@Component({
  selector: "app-root",
  imports: [RouterOutlet, RouterLink, RouterLinkActive, CommonModule],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnInit {
  private router = inject(Router);
  private jiraStateService = inject(JiraStateService);
  private pontoMaisStateService = inject(PontoMaisStateService);
  private syncStateService = inject(SyncStateService);

  currentRoute = "/tasks";

  readonly userName = computed(() => {
    const state = this.jiraStateService.state();
    if (!state.connected) return "Jira desconectado";
    return state.user?.displayName || state.email || "Conectado ao Jira";
  });

  readonly jiraInstance = computed(() => {
    const state = this.jiraStateService.state();
    if (!state.connected || !state.instance) return "Conecte em Configurações";
    return state.instance.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  });

  ngOnInit() {
    this.currentRoute = this.router.url;

    this.router.events.pipe(filter((event) => event instanceof NavigationEnd)).subscribe((event: any) => {
      this.currentRoute = event.urlAfterRedirects;
    });

    this.initTheme();
    this.initStateServices();
  }

  initStateServices() {
    this.jiraStateService.initialize();
    this.pontoMaisStateService.initialize();
    this.syncStateService.initialize();

    this.jiraStateService.refresh();
    this.pontoMaisStateService.refresh();
  }

  initTheme() {
    let saved: string | null = null;
    try {
      saved = localStorage.getItem("gama-theme");
    } catch (e) {
      saved = null;
    }
    this.applyTheme(saved || "light");
  }

  applyTheme(theme: string) {
    const validThemes = ["light", "dark", "system"];
    const finalTheme = validThemes.includes(theme) ? theme : "light";

    if (finalTheme === "system") {
      document.documentElement.removeAttribute("data-theme");
    } else {
      document.documentElement.setAttribute("data-theme", finalTheme);
    }

    try {
      localStorage.setItem("gama-theme", finalTheme);
    } catch (e) {}
  }
}
