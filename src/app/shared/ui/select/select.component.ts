import { ChangeDetectionStrategy, Component, computed, ElementRef, input, output, signal, viewChild } from "@angular/core";
import { CdkConnectedOverlay, CdkOverlayOrigin, ConnectedPosition } from "@angular/cdk/overlay";

export interface SelectOption<T> {
  value: T;
  label: string;
}

@Component({
  selector: "gama-select",
  imports: [CdkOverlayOrigin, CdkConnectedOverlay],
  templateUrl: "./select.component.html",
  styleUrl: "./select.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SelectComponent<T> {
  readonly options = input.required<SelectOption<T>[]>();
  readonly value = input.required<T>();
  readonly disabled = input(false);
  readonly ariaLabel = input("");
  readonly changed = output<T>();

  protected readonly open = signal(false);
  protected readonly activeIndex = signal(-1);

  private readonly trigger = viewChild.required<ElementRef<HTMLButtonElement>>("trigger");

  protected readonly selectedIndex = computed(() => this.options().findIndex((o) => o.value === this.value()));
  protected readonly label = computed(() => this.options()[this.selectedIndex()]?.label ?? "");

  protected readonly positions: ConnectedPosition[] = [
    { originX: "end", originY: "bottom", overlayX: "end", overlayY: "top", offsetY: 8 },
    { originX: "end", originY: "top", overlayX: "end", overlayY: "bottom", offsetY: -8 },
  ];

  protected toggle(): void {
    if (this.open()) {
      this.close();
      return;
    }
    this.activeIndex.set(Math.max(this.selectedIndex(), 0));
    this.open.set(true);
  }

  protected close(): void {
    this.open.set(false);
  }

  protected onTriggerKeydown(event: KeyboardEvent): void {
    if (this.open()) {
      if (event.key === "Enter" || event.key === " ") event.preventDefault();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      this.toggle();
    }
  }

  protected onOutsideClick(event: MouseEvent): void {
    if (!this.trigger().nativeElement.contains(event.target as Node)) this.close();
  }

  protected onKeydown(event: KeyboardEvent): void {
    const count = this.options().length;
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        this.close();
        this.trigger().nativeElement.focus();
        break;
      case "ArrowDown":
        event.preventDefault();
        this.activeIndex.update((i) => (i + 1) % count);
        break;
      case "ArrowUp":
        event.preventDefault();
        this.activeIndex.update((i) => (i - 1 + count) % count);
        break;
      case "Home":
        event.preventDefault();
        this.activeIndex.set(0);
        break;
      case "End":
        event.preventDefault();
        this.activeIndex.set(count - 1);
        break;
      case "Enter":
      case " ": {
        event.preventDefault();
        const option = this.options()[this.activeIndex()];
        if (option) this.choose(option);
        break;
      }
    }
  }

  protected choose(option: SelectOption<T>): void {
    this.close();
    this.trigger().nativeElement.focus();
    if (option.value !== this.value()) this.changed.emit(option.value);
  }
}
