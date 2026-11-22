import { ChangeDetectionStrategy, Component, inject, output, signal, viewChild } from "@angular/core";
import { A11yModule } from "@angular/cdk/a11y";
import { SettingsPage, SettingsSection } from "../../pages/settings/settings.page";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { SettingsDialogService } from "../../settings-dialog.service";

@Component({
  selector: "app-settings-dialog",
  imports: [IconComponent, A11yModule, SettingsPage],
  templateUrl: "./settings-dialog.component.html",
  styleUrl: "./settings-dialog.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    "(document:keydown.escape)": "onEscape($event)",
  },
})
export class SettingsDialogComponent {
  readonly close = output<void>();

  protected readonly sections: { id: SettingsSection; label: string }[] = [
    { id: "general", label: "Geral" },
    { id: "account", label: "Conta" },
  ];
  protected readonly section = signal<SettingsSection>(inject(SettingsDialogService).section());

  private readonly settings = viewChild.required(SettingsPage);

  onEscape(event: Event): void {
    if (event.defaultPrevented || this.settings().hasOpenForm) return;
    this.close.emit();
  }

  onBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) this.close.emit();
  }
}
