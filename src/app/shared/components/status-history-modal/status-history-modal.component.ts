import { Component, EventEmitter, HostListener, inject, Input, Output } from "@angular/core";
import { A11yModule } from "@angular/cdk/a11y";
import { CommonModule } from "@angular/common";
import { ChangelogItem, IssueChange } from "../../../core/models";
import { StatusCatalogStore } from "../../../core/issues/status-catalog.store";
import { IconComponent } from "../../ui/icon/icon.component";
import { isStatusChange, statusBadge, statusDotColor } from "./status-dot";

const FIELD_LABELS: Record<string, string> = {
  status: "Status",
  assignee: "Responsável",
};

@Component({
  selector: "app-status-history-modal",
  imports: [IconComponent, CommonModule, A11yModule],
  templateUrl: "./status-history-modal.component.html",
  styleUrl: "./status-history-modal.component.css",
})
export class StatusHistoryModalComponent {
  @Input() isOpen = false;
  @Input() statusHistory: IssueChange[] = [];
  @Input() taskKey = "";
  @Input() loading = false;
  @Input() syncing = false;
  @Input() earliestDate: string | null = null;
  @Input() latestDate: string | null = null;
  @Input() error = "";
  @Output() close = new EventEmitter<void>();
  @Output() sync = new EventEmitter<void>();

  private readonly statusCatalog = inject(StatusCatalogStore);

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

  @HostListener("document:keydown.escape", ["$event"])
  onEscape(event: Event): void {
    if (!this.isOpen || event.defaultPrevented) return;
    event.preventDefault();
    this.onClose();
  }

  onBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) {
      this.onClose();
    }
  }

  formatDay(dateString: string | null): string {
    const date = this.parse(dateString);
    if (!date) return dateString ?? "—";
    return date.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit", year: "numeric" });
  }

  formatWhen(dateString: string | null): string {
    const date = this.parse(dateString);
    if (!date) return dateString ?? "—";
    const day = date.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit" });
    const time = date.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
    return `${day}, ${time}`;
  }

  private parse(dateString: string | null): Date | null {
    if (!dateString) return null;
    const date = new Date(dateString);
    return Number.isNaN(date.getTime()) ? null : date;
  }

  fieldLabel(item: ChangelogItem): string {
    return FIELD_LABELS[item.fieldId ?? item.field] ?? item.field;
  }

  trackEntry(_: number, entry: IssueChange): string {
    return entry.changelogId;
  }
}
