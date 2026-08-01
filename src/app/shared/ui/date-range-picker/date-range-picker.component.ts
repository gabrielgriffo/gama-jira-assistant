import {
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  input,
  output,
  signal,
  viewChild,
} from "@angular/core";
import { CdkConnectedOverlay, CdkOverlayOrigin, ConnectedPosition } from "@angular/cdk/overlay";
import { DateRange, LocalDate, rangeLabel, todayIn, YearMonth } from "../../date/local-date";
import { DEFAULT_RANGE_PRESETS, matchPreset, RangePreset } from "../../date/range-presets";
import { DateRangePanelComponent } from "./date-range-panel.component";

@Component({
  selector: "gama-date-range-picker",
  imports: [CdkOverlayOrigin, CdkConnectedOverlay, DateRangePanelComponent],
  templateUrl: "./date-range-picker.component.html",
  styleUrl: "./date-range-picker.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DateRangePickerComponent {
  readonly value = input.required<DateRange>();
  readonly timeZone = input.required<string>();
  readonly presets = input<RangePreset[]>(DEFAULT_RANGE_PRESETS);
  readonly minMonth = input<YearMonth | null>(null);
  readonly applyText = input("Aplicar");
  readonly titlePrefix = input("Período");
  readonly applied = output<DateRange>();

  protected readonly open = signal(false);
  protected readonly today = signal<LocalDate>("");

  private readonly trigger = viewChild.required<ElementRef<HTMLButtonElement>>("trigger");

  protected readonly datesLabel = computed(() => rangeLabel(this.value()));
  protected readonly label = computed(
    () => matchPreset(this.value(), todayIn(this.timeZone()), this.presets())?.label ?? this.datesLabel(),
  );

  protected readonly positions: ConnectedPosition[] = [
    { originX: "start", originY: "bottom", overlayX: "start", overlayY: "top", offsetY: 8 },
    { originX: "start", originY: "top", overlayX: "start", overlayY: "bottom", offsetY: -8 },
  ];

  protected toggle(): void {
    if (this.open()) {
      this.close();
      return;
    }
    this.today.set(todayIn(this.timeZone()));
    this.open.set(true);
  }

  protected close(): void {
    this.open.set(false);
  }

  protected onOutsideClick(event: MouseEvent): void {
    if (!this.trigger().nativeElement.contains(event.target as Node)) this.close();
  }

  protected onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      this.close();
    }
  }

  protected onApply(range: DateRange): void {
    this.close();
    this.applied.emit(range);
  }
}
