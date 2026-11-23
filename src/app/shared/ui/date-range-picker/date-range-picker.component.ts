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
import { LocalDate, rangeLabel, todayIn, YearMonth } from "../../date/local-date";
import { DEFAULT_RANGE_PRESETS, RangePreset, RangeSelection, selectedPreset } from "../../date/range-presets";
import { DateRangePanelComponent } from "./date-range-panel.component";
import { IconComponent } from "../icon/icon.component";

@Component({
  selector: "gama-date-range-picker",
  imports: [IconComponent, CdkOverlayOrigin, CdkConnectedOverlay, DateRangePanelComponent],
  templateUrl: "./date-range-picker.component.html",
  styleUrl: "./date-range-picker.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DateRangePickerComponent {
  readonly value = input.required<RangeSelection>();
  readonly timeZone = input.required<string>();
  readonly currentDay = input<LocalDate | null>(null);
  readonly presets = input<RangePreset[]>(DEFAULT_RANGE_PRESETS);
  readonly minMonth = input<YearMonth | null>(null);
  readonly applyText = input("Aplicar");
  readonly applied = output<RangeSelection>();

  protected readonly open = signal(false);
  protected readonly today = signal<LocalDate>("");

  private readonly trigger = viewChild.required<ElementRef<HTMLButtonElement>>("trigger");

  protected readonly datesLabel = computed(() => rangeLabel(this.value()));
  protected readonly label = computed(
    () =>
      selectedPreset(this.value(), this.currentDay() ?? todayIn(this.timeZone()), this.presets())?.label ??
      this.datesLabel(),
  );

  protected readonly positions: ConnectedPosition[] = [
    { originX: "start", originY: "bottom", overlayX: "start", overlayY: "top", offsetY: 8 },
    { originX: "end", originY: "top", overlayX: "start", overlayY: "top", offsetX: 8, offsetY: -4 },
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

  protected onApply(range: RangeSelection): void {
    this.close();
    this.applied.emit(range);
  }
}
