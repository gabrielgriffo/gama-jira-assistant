import { Component, OnDestroy, OnInit, inject, signal } from "@angular/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { SettingsDialogService } from "../../../features/settings/settings-dialog.service";
import { IconComponent } from "../../../shared/ui/icon/icon.component";
import { leftSlop, topOverlay } from "./overlay-dismiss";

@Component({
  selector: "app-titlebar",
  imports: [IconComponent],
  templateUrl: "./titlebar.component.html",
  styleUrl: "./titlebar.component.css",
  host: {
    "[class.mac]": "isMac",
    "(click)": "onClick($event)",
    "(mousedown)": "onMouseDown($event)",
  },
})
export class TitlebarComponent implements OnInit, OnDestroy {
  readonly isMac = /Mac/i.test(navigator.userAgent);
  readonly maximized = signal(false);
  private window = getCurrentWindow();
  private unlisten?: UnlistenFn;
  private unlistenMoved?: UnlistenFn;
  private moves = 0;
  private stopPending?: () => void;
  private readonly settingsDialog = inject(SettingsDialogService);

  async ngOnInit() {
    this.unlistenMoved = await this.window.onMoved(() => this.moves++);
    if (this.isMac) return;
    await this.syncMaximized();
    this.unlisten = await this.window.onResized(() => this.syncMaximized());
  }

  ngOnDestroy() {
    this.unlisten?.();
    this.unlistenMoved?.();
    this.stopPending?.();
  }

  onClick(event: MouseEvent) {
    if (this.inDragRegion(event)) return;
    this.dismissOverlay();
  }

  onMouseDown(event: MouseEvent) {
    if (event.button !== 0 || event.detail !== 1 || !this.inDragRegion(event) || !this.currentOverlay()) return;
    this.stopPending?.();
    const start = { x: event.clientX, y: event.clientY };
    const moves = this.moves;
    const finish = (e: Event) => {
      if (e instanceof MouseEvent && e.type === "mousemove" && !leftSlop(start, { x: e.clientX, y: e.clientY })) return;
      this.stopPending?.();
      if (this.moves === moves) this.dismissOverlay();
    };
    const types = ["mousemove", "mousedown", "keydown"] as const;
    types.forEach((type) => document.addEventListener(type, finish, true));
    this.stopPending = () => {
      types.forEach((type) => document.removeEventListener(type, finish, true));
      this.stopPending = undefined;
    };
  }

  private inDragRegion(event: Event): boolean {
    return event.target instanceof Element && !!event.target.closest("[data-tauri-drag-region]");
  }

  private currentOverlay(): Element | null {
    return topOverlay(document.querySelectorAll(".overlay"));
  }

  private dismissOverlay() {
    this.currentOverlay()?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  }

  openSettings() {
    this.settingsDialog.show();
  }

  minimize() {
    this.window.minimize();
  }

  toggleMaximize() {
    this.window.toggleMaximize();
  }

  close() {
    this.window.close();
  }

  private async syncMaximized() {
    this.maximized.set(await this.window.isMaximized());
  }
}
