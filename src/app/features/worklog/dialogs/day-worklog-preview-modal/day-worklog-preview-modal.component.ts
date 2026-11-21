import { Component, EventEmitter, HostListener, Input, OnChanges, Output } from "@angular/core";
import { CommonModule } from "@angular/common";
import { A11yModule } from "@angular/cdk/a11y";
import { error } from "@tauri-apps/plugin-log";
import { WorklogDraft } from "../../data-access/worklog";
import { WorkDayTimeline } from "../../../../core/pontomais/pontomais.service";
import { WorklogService } from "../../data-access/worklog.service";
import { ToastService } from "../../../../core/feedback/toast.service";
import { weekday } from "../../../../shared/date/local-date";
import { IconComponent } from "../../../../shared/ui/icon/icon.component";
import { minutesRegisteredElsewhere } from "./registered-elsewhere";
import { clock, DraftError, jiraOffset, validateDrafts } from "./draft-validation";

const WEEKDAYS = ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"];
const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

interface DraftNote {
  kind: "" | "info" | "err";
  text: string;
}

@Component({
  selector: "app-day-worklog-preview-modal",
  imports: [IconComponent, CommonModule, A11yModule],
  templateUrl: "./day-worklog-preview-modal.component.html",
  styleUrl: "./day-worklog-preview-modal.component.css",
})
export class DayWorklogPreviewModalComponent implements OnChanges {
  @Input() isOpen = false;
  @Input() day: WorkDayTimeline | null = null;
  @Input() drafts: WorklogDraft[] = [];
  @Input() timeZone = "";
  @Output() close = new EventEmitter<void>();
  @Output() sent = new EventEmitter<void>();

  sending = false;
  confirming = false;
  errors: Record<number, DraftError> = {};
  notes: DraftNote[] = [];
  total = 0;
  requests = "";

  constructor(
    private worklogService: WorklogService,
    private toastService: ToastService,
  ) {}

  ngOnChanges(): void {
    if (!this.isOpen) {
      this.confirming = false;
    }
    this.refresh();
  }

  @HostListener("document:keydown.escape")
  onEscape(): void {
    if (this.isOpen) this.onClose();
  }

  get toSend(): WorklogDraft[] {
    return this.drafts.filter((d) => d.state === "pending" || d.state === "failed");
  }

  get allSent(): boolean {
    return this.drafts.length > 0 && this.drafts.every((d) => d.state === "sent");
  }

  get hasErrors(): boolean {
    return Object.keys(this.errors).length > 0;
  }

  get title(): string {
    if (!this.day) return "";
    const [, m, d] = this.day.date.split("-").map(Number);
    return `Registrar worklogs de ${WEEKDAYS[weekday(this.day.date)]}, ${d} de ${MONTHS[m - 1]}`;
  }

  get punchSummary(): string {
    if (!this.day) return "";
    const intervals = this.day.punchIntervals.map((iv) => `${this.clock(iv.start)}–${this.clock(iv.end)}`).join(", ");
    return `Ponto: ${this.hfmt(this.day.totalMinutes)}${intervals ? ` (${intervals})` : ""}.`;
  }

  worklogLabel(count: number): string {
    return `${count} ${count === 1 ? "worklog" : "worklogs"}`;
  }

  isLocked(draft: WorklogDraft): boolean {
    return this.sending || this.confirming || draft.state === "sent" || draft.state === "sending";
  }

  rowError(draft: WorklogDraft): string {
    return this.errors[draft.id]?.text || (draft.state === "failed" && draft.error ? draft.error : "");
  }

  badStart(draft: WorklogDraft): boolean {
    return this.errors[draft.id]?.field === "start";
  }

  badDuration(draft: WorklogDraft): boolean {
    return this.errors[draft.id]?.field === "duration";
  }

  badComment(draft: WorklogDraft): boolean {
    return this.errors[draft.id]?.field === "comment";
  }

  onStartInput(draft: WorklogDraft, value: string): void {
    if (!value) {
      draft.start = NaN;
    } else {
      const [h, m] = value.split(":").map(Number);
      draft.start = h * 60 + m;
      draft.startText = value;
    }
    this.touch(draft);
  }

  startText(draft: WorklogDraft): string {
    return draft.startText ?? this.clock(draft.start);
  }

  onDurationInput(draft: WorklogDraft, value: string): void {
    draft.durationRaw = value;
    draft.duration = this.parseDuration(value);
    this.touch(draft);
  }

  onCommentInput(draft: WorklogDraft, value: string): void {
    draft.comment = value;
    this.touch(draft);
  }

  removeDraft(draft: WorklogDraft): void {
    const index = this.drafts.indexOf(draft);
    if (index >= 0) this.drafts.splice(index, 1);
    this.refresh();
  }

  askSend(): void {
    if (this.hasErrors || !this.toSend.length) return;
    this.confirming = true;
  }

  cancelSend(): void {
    this.confirming = false;
  }

  async confirmSend(): Promise<void> {
    this.refresh();
    if (this.hasErrors || !this.toSend.length) {
      this.confirming = false;
      return;
    }
    await this.send(this.toSend);
  }

  async retry(draft: WorklogDraft): Promise<void> {
    if (this.errors[draft.id]) return;
    await this.send([draft]);
  }

  onClose(): void {
    if (this.sending) return;
    this.confirming = false;
    this.close.emit();
  }

  onBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) this.onClose();
  }

  trackDraft(_: number, draft: WorklogDraft): number {
    return draft.id;
  }

  hfmt(minutes: number): string {
    const m = Math.abs(Math.round(minutes));
    return `${Math.floor(m / 60)}h${String(m % 60).padStart(2, "0")}`;
  }

  clock(minutes: number): string {
    return Number.isFinite(minutes) ? clock(minutes) : "";
  }

  private touch(draft: WorklogDraft): void {
    if (draft.state === "failed") draft.error = null;
    this.refresh();
  }

  private async send(drafts: WorklogDraft[]): Promise<void> {
    if (this.sending || !drafts.length) return;
    this.sending = true;
    this.confirming = false;

    let ok = 0;
    let failed = 0;
    for (const draft of drafts) {
      draft.state = "sending";
      draft.error = null;
      try {
        const synced = await this.worklogService.saveAndSyncWorklogsToJira([
          {
            date: draft.date,
            issueKey: draft.issueKey,
            issueSummary: draft.issueSummary,
            status: "",
            hours: draft.duration / 60,
            startTimestamp: this.clock(draft.start),
            endTimestamp: "",
            comment: draft.comment.trim() || undefined,
          },
        ]);
        if (synced > 0) {
          draft.state = "sent";
          ok++;
        } else {
          draft.state = "failed";
          draft.error = "O Jira não confirmou o registro. Tente reenviar.";
          failed++;
        }
      } catch (err) {
        error(`failed to submit worklog ${draft.issueKey}: ${err}`);
        draft.state = "failed";
        draft.error = `Não foi possível registrar o worklog: ${err}`;
        failed++;
      }
      this.refresh();
    }

    this.sending = false;
    this.refresh();

    if (failed) {
      this.toastService.error(
        `${ok} ${ok === 1 ? "worklog enviado" : "worklogs enviados"} e ${failed} com falha. Os worklogs com falha podem ser reenviados.`,
      );
    } else {
      this.toastService.success(`${ok} ${ok === 1 ? "worklog enviado" : "worklogs enviados"} para o Jira.`);
    }
    if (ok) this.sent.emit();
  }

  private refresh(): void {
    this.errors = this.validate();
    this.total = this.drafts.filter((d) => Number.isFinite(d.duration)).reduce((sum, d) => sum + d.duration, 0);
    this.notes = this.buildNotes();
    this.requests = this.buildRequests();
  }

  private validate(): Record<number, DraftError> {
    return validateDrafts(this.drafts, this.timeZone);
  }

  private buildNotes(): DraftNote[] {
    if (!this.day) return [];
    const notes: DraftNote[] = [];
    const otherRegistered = minutesRegisteredElsewhere(this.day, this.drafts);
    if (otherRegistered > 0) {
      notes.push({
        kind: "info",
        text: `Este dia já possui ${this.hfmt(otherRegistered)} registradas no Jira. Verifique se o mesmo trabalho não será registrado em duplicidade.`,
      });
    }
    if (this.total > this.day.totalMinutes) {
      notes.push({
        kind: "",
        text: `O total dos worklogs (${this.hfmt(this.total)}) excede o ponto do dia (${this.hfmt(this.day.totalMinutes)}).`,
      });
    }
    const count = Object.keys(this.errors).length;
    if (count) {
      notes.push({
        kind: "err",
        text: `${count === 1 ? "Um worklog requer" : `${count} worklogs requerem`} ajustes antes do envio.`,
      });
    }
    return notes;
  }

  private buildRequests(): string {
    const pending = this.toSend;
    if (!pending.length) return "Nenhum worklog pendente de envio.";
    return pending
      .map((d) => {
        const body: Record<string, unknown> = {
          started: `${d.date}T${this.clock(d.start)}:00.000${jiraOffset(d.date, d.start, this.timeZone)}`,
          timeSpentSeconds: Number.isFinite(d.duration) ? d.duration * 60 : null,
        };
        const comment = d.comment.trim();
        if (comment) {
          body["comment"] = {
            type: "doc",
            version: 1,
            content: [{ type: "paragraph", content: [{ type: "text", text: comment }] }],
          };
        }
        return `POST /rest/api/3/issue/${d.issueKey}/worklog\n${JSON.stringify(body, null, 2)}`;
      })
      .join("\n\n");
  }

  private parseDuration(value: string): number {
    const s = value.trim();
    let m = s.match(/^(\d{1,2}):([0-5]\d)$/);
    if (m) return +m[1] * 60 + +m[2];
    m = s.match(/^(\d{1,2})h\s*([0-5]?\d)?m?$/i);
    if (m) return +m[1] * 60 + (+m[2] || 0);
    m = s.match(/^(\d{1,3})m$/i);
    if (m) return +m[1];
    return NaN;
  }
}
