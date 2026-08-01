import { Component, EventEmitter, Input, Output } from "@angular/core";
import { CommonModule } from "@angular/common";
import { ChangelogItem, IssueChange } from "../../core/models";

const FIELD_LABELS: Record<string, string> = {
  status: "Status",
  assignee: "Responsável",
};

@Component({
  selector: "app-status-history-modal",
  imports: [CommonModule],
  templateUrl: "./status-history-modal.component.html",
  styleUrl: "./status-history-modal.component.css",
})
export class StatusHistoryModalComponent {
  @Input() isOpen = false;
  @Input() statusHistory: IssueChange[] = [];
  @Input() taskKey = "";
  @Input() loading = false;
  @Input() syncingInBackground = false;
  @Input() earliestDate: string | null = null;
  @Input() latestDate: string | null = null;
  @Output() close = new EventEmitter<void>();

  onClose(): void {
    this.close.emit();
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
