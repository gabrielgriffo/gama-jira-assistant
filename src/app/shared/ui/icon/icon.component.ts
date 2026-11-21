import { ChangeDetectionStrategy, Component, computed, inject, input } from "@angular/core";
import { DomSanitizer, SafeHtml } from "@angular/platform-browser";
import { IconName, ICONS } from "./icons";

@Component({
  selector: "app-icon",
  template: "",
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    "aria-hidden": "true",
    "[innerHTML]": "svg()",
  },
})
export class IconComponent {
  readonly name = input.required<IconName>();

  private readonly sanitizer = inject(DomSanitizer);

  protected readonly svg = computed<SafeHtml>(() => this.sanitizer.bypassSecurityTrustHtml(ICONS[this.name()]));
}
