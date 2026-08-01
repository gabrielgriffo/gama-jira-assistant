import {
  AfterViewInit,
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  computed,
  DestroyRef,
  ElementRef,
  inject,
  input,
  NgZone,
  OnInit,
  output,
  signal,
  viewChild,
} from "@angular/core";
import { CdkTrapFocus } from "@angular/cdk/a11y";
import {
  DateRange,
  daysBetween,
  LocalDate,
  monthOf,
  rangeLabel,
  shiftMonth,
  shortDate,
  YearMonth,
} from "../../date/local-date";
import { matchPreset, RangePreset } from "../../date/range-presets";
import { buildMonths, DraftRange, MonthView, pickDay, visibleBounds } from "./range-calendar";

const INITIAL_MONTHS = 12;
const MONTHS_PER_BATCH = 6;
const LOAD_THRESHOLD_PX = 320;
const STICKY_HEADER_PX = 64;

@Component({
  selector: "gama-date-range-panel",
  imports: [CdkTrapFocus],
  templateUrl: "./date-range-panel.component.html",
  styleUrl: "./date-range-panel.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DateRangePanelComponent implements OnInit, AfterViewInit {
  readonly value = input.required<DateRange>();
  readonly today = input.required<LocalDate>();
  readonly presets = input.required<RangePreset[]>();
  readonly minMonth = input<YearMonth | null>(null);
  readonly applyText = input.required<string>();
  readonly apply = output<DateRange>();
  readonly cancel = output<void>();

  protected readonly weekdays = ["D", "S", "T", "Q", "Q", "S", "S"];
  protected readonly draft = signal<DraftRange>({ start: null, end: null });
  protected readonly hover = signal<LocalDate | null>(null);
  protected readonly months = signal<MonthView[]>([]);

  protected readonly bounds = computed(() => visibleBounds(this.draft(), this.hover()));

  protected readonly activePresetId = computed(() => {
    const { start, end } = this.draft();
    if (!start || !end) return null;
    return matchPreset({ from: start, to: end }, this.today(), this.presets())?.id ?? null;
  });

  protected readonly summary = computed(() => {
    const b = this.bounds();
    if (!b) return "Escolha o primeiro dia";
    if (!this.draft().end && !this.hover()) return `${shortDate(b.lo)} · escolha o último dia`;
    const n = daysBetween(b.lo, b.hi);
    return `${rangeLabel({ from: b.lo, to: b.hi })} · ${n} ${n === 1 ? "dia" : "dias"}`;
  });

  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>("scroller");
  private readonly cdr = inject(ChangeDetectorRef);
  private readonly zone = inject(NgZone);
  private readonly destroyRef = inject(DestroyRef);

  ngOnInit(): void {
    const { from, to } = this.value();
    this.draft.set({ start: from, end: to });

    const current = monthOf(this.today());
    let first = [monthOf(from), shiftMonth(current, -(INITIAL_MONTHS - 1))].sort()[0];
    const min = this.minMonth();
    if (min && first < min) first = min;
    this.months.set(buildMonths(first, current, this.today()));
  }

  ngAfterViewInit(): void {
    const el = this.scroller().nativeElement;
    el.scrollTop = el.scrollHeight;
    const { start, end } = this.draft();
    if (start) this.reveal(start, end ?? start);

    const onScroll = () => {
      if (el.scrollTop < LOAD_THRESHOLD_PX) this.zone.run(() => this.prependMonths(MONTHS_PER_BATCH));
    };
    this.zone.runOutsideAngular(() => el.addEventListener("scroll", onScroll, { passive: true }));
    this.destroyRef.onDestroy(() => el.removeEventListener("scroll", onScroll));
  }

  protected pick(day: LocalDate): void {
    this.draft.update((d) => pickDay(d, day));
    this.hover.set(null);
  }

  protected preview(day: LocalDate | null): void {
    const { start, end } = this.draft();
    if (start && !end) this.hover.set(day);
  }

  protected choosePreset(preset: RangePreset): void {
    const r = preset.resolve(this.today());
    this.draft.set({ start: r.from, end: r.to });
    this.hover.set(null);
    this.prependUntil(monthOf(r.from));
    this.reveal(r.from, r.to);
  }

  protected confirm(): void {
    const { start, end } = this.draft();
    if (start) this.apply.emit({ from: start, to: end ?? start });
  }

  protected isStart(day: LocalDate): boolean {
    return this.bounds()?.lo === day;
  }

  protected isEnd(day: LocalDate): boolean {
    return this.bounds()?.hi === day;
  }

  protected isIn(day: LocalDate): boolean {
    const b = this.bounds();
    return !!b && day > b.lo && day < b.hi;
  }

  private prependMonths(count: number): void {
    const first = this.months()[0].key;
    let from = shiftMonth(first, -count);
    const min = this.minMonth();
    if (min && from < min) from = min;
    if (from >= first) return;

    const el = this.scroller().nativeElement;
    const heightBefore = el.scrollHeight;
    this.months.update((list) => [...buildMonths(from, shiftMonth(first, -1), this.today()), ...list]);
    this.cdr.detectChanges();
    el.scrollTop += el.scrollHeight - heightBefore;
  }

  private prependUntil(month: YearMonth): void {
    while (this.months()[0].key > month) {
      const before = this.months()[0].key;
      this.prependMonths(MONTHS_PER_BATCH);
      if (this.months()[0].key === before) break;
    }
  }

  private reveal(from: LocalDate, to: LocalDate): void {
    const el = this.scroller().nativeElement;
    const first = el.querySelector<HTMLElement>(`[data-date="${from}"]`);
    const last = el.querySelector<HTMLElement>(`[data-date="${to}"]`);
    if (!first || !last) return;
    const startY = first.offsetTop - STICKY_HEADER_PX - 4;
    const endY = last.offsetTop + last.offsetHeight + 8 - el.clientHeight;
    el.scrollTop = endY <= startY ? Math.min(Math.max(el.scrollTop, endY), startY) : startY;
  }
}
