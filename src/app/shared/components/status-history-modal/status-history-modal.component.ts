import { ChangeDetectionStrategy, Component, inject, input, output } from "@angular/core";
import { A11yModule } from "@angular/cdk/a11y";
import { ChangelogItem, IssueChange } from "../../../core/models";
import { StatusCatalogStore } from "../../../core/issues/status-catalog.store";
import { AppSettingsService } from "../../../core/preferences/app-settings.service";
import { brDate, zonedDateTime } from "../../date/local-date";
import { IconComponent } from "../../ui/icon/icon.component";
import { isStatusChange, statusBadge, statusDotColor } from "./status-dot";

const FIELD_LABELS: Record<string, string> = {
  status: "Status",
  assignee: "Responsável",
};

@Component({
  selector: "app-status-history-modal",
  imports: [IconComponent, A11yModule],
  templateUrl: "./status-history-modal.component.html",
  styleUrl: "./status-history-modal.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    "(document:keydown.escape)": "onEscape($event)",
  },
})
export class StatusHistoryModalComponent {
  readonly isOpen = input(false);
  readonly statusHistory = input<IssueChange[]>([]);
  readonly taskKey = input("");
  readonly loading = input(false);
  readonly syncing = input(false);
  readonly earliestDate = input<string | null>(null);
  readonly latestDate = input<string | null>(null);
  readonly error = input("");
  readonly close = output<void>();
  readonly sync = output<void>();

  private readonly statusCatalog = inject(StatusCatalogStore);
  private readonly settings = inject(AppSettingsService);

  dotColor(entry: IssueChange): string | null {
    return statusDotColor(entry, this.statusCatalog.statuses());
  }

  readonly isStatusChange = isStatusChange;

  badgeFor(id: string | null, name: string | null): string {
    return statusBadge(id, name, this.statusCatalog.statuses());
  }

  onSync(): void {
    this.sync.emit();
  }

  onClose(): void {
    this.close.emit();
  }

  onEscape(event: Event): void {
    if (!this.isOpen() || event.defaultPrevented) return;
    event.preventDefault();
    this.onClose();
  }

  onBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) {
      this.onClose();
    }
  }

  formatDay(dateString: string | null): string {
    const zoned = dateString ? zonedDateTime(dateString, this.settings.timeZone) : null;
    if (!zoned) return dateString ?? "—";
    return brDate(zoned.date);
  }

  formatWhen(dateString: string | null): string {
    const zoned = dateString ? zonedDateTime(dateString, this.settings.timeZone) : null;
    if (!zoned) return dateString ?? "—";
    const [, month, day] = zoned.date.split("-");
    return `${day}/${month}, ${zoned.time}`;
  }

  fieldLabel(item: ChangelogItem): string {
    return FIELD_LABELS[item.fieldId ?? item.field] ?? item.field;
  }
}
