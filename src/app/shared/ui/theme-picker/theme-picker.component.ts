import { booleanAttribute, ChangeDetectionStrategy, Component, inject, input, signal } from "@angular/core";
import { ThemeService } from "../../../core/preferences/theme.service";
import type { Theme } from "../../../core/preferences/theme";

@Component({
  selector: "gama-theme-picker",
  templateUrl: "./theme-picker.component.html",
  styleUrl: "./theme-picker.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    "[class.large]": "large()",
  },
})
export class ThemePickerComponent {
  private readonly themeService = inject(ThemeService);

  readonly large = input(false, { transform: booleanAttribute });

  readonly theme = signal<Theme>(this.themeService.theme);

  set(theme: Theme): void {
    this.theme.set(theme);
    this.themeService.set(theme);
  }
}
