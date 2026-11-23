import { DOCUMENT } from "@angular/common";
import { ChangeDetectionStrategy, Component, DestroyRef, ElementRef, NgZone, inject, viewChild } from "@angular/core";
import {
  type Align,
  CONTROL_SELECTOR,
  IGNORED_SELECTOR,
  canTruncate,
  iconLabel,
  isTruncated,
  openDelay,
  placeTooltip,
  tooltipText,
} from "./truncation";

const MAX_DEPTH = 6;

interface Anchor {
  el: Element;
  text: string;
  align: Align;
}

function textOf(el: Element): string {
  return tooltipText(
    Array.from(el.childNodes, (node) =>
      node instanceof HTMLElement ? node.innerText : node.nodeType === Node.TEXT_NODE ? (node.nodeValue ?? "") : "",
    ).join(" "),
  );
}

@Component({
  selector: "app-truncation-tooltip",
  template: `<div #tip class="trunc-tip" aria-hidden="true"></div>`,
  styles: [
    `
      .trunc-tip {
        position: fixed;
        top: 0;
        left: 0;
        z-index: 10000;
        max-width: min(420px, calc(100vw - 16px));
        padding: 6px 8px;
        border-radius: 6px;
        background: var(--inverse);
        color: var(--on-inverse);
        box-shadow: var(--shadow-pop);
        font-size: 12px;
        line-height: 16px;
        white-space: normal;
        overflow-wrap: anywhere;
        pointer-events: none;
        opacity: 0;
        transform: scale(0.97);
        visibility: hidden;
        transition:
          opacity 125ms var(--ease-out),
          transform 125ms var(--ease-out),
          visibility 0s 125ms;
      }

      .trunc-tip.open {
        opacity: 1;
        transform: none;
        visibility: visible;
        transition-delay: 0s;
      }

      .trunc-tip.instant {
        transition-duration: 0s;
        transition-delay: 0s;
      }

      @media (prefers-reduced-motion: reduce) {
        .trunc-tip {
          transform: none;
        }
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TruncationTooltipComponent {
  private readonly doc = inject(DOCUMENT);
  private readonly tip = viewChild.required<ElementRef<HTMLDivElement>>("tip");
  private anchor: Anchor | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private open = false;
  private lastClosedAt: number | null = null;

  constructor() {
    const view = this.doc.defaultView;
    const listeners: [EventTarget, string, EventListener, boolean][] = [
      [this.doc, "pointerover", (e) => this.onOver(e as PointerEvent), false],
      [this.doc, "pointerout", (e) => this.onOut(e as PointerEvent), false],
      [this.doc, "focusin", (e) => this.onFocus(e), true],
      [this.doc, "focusout", (e) => this.onBlur(e), true],
      [this.doc, "pointerdown", () => this.hide(true), true],
      [this.doc, "keydown", () => this.hide(true), true],
      [this.doc, "wheel", () => this.hide(true), true],
      [this.doc, "scroll", () => this.hide(true), true],
    ];
    if (view) listeners.push([view, "blur", () => this.hide(true), false]);
    inject(NgZone).runOutsideAngular(() => {
      for (const [target, type, fn, capture] of listeners)
        target.addEventListener(type, fn, { capture, passive: true });
    });
    inject(DestroyRef).onDestroy(() => {
      clearTimeout(this.timer);
      for (const [target, type, fn, capture] of listeners) target.removeEventListener(type, fn, capture);
    });
  }

  private onOver(event: PointerEvent): void {
    if (event.pointerType === "touch") return;
    const target = event.target instanceof Element ? event.target : null;
    const anchor = target ? this.findAnchor(target) : null;
    if (anchor?.el === this.anchor?.el) return;
    if (!anchor) {
      if (this.anchor && target && !this.anchor.el.contains(target)) this.hide(false);
      return;
    }
    this.schedule(anchor, false);
  }

  private onOut(event: PointerEvent): void {
    if (!this.anchor) return;
    const next = event.relatedTarget instanceof Node ? event.relatedTarget : null;
    if (next && this.anchor.el.contains(next)) return;
    this.hide(false);
  }

  private onFocus(event: Event): void {
    const target = event.target instanceof Element ? event.target : null;
    if (!target?.matches(":focus-visible")) return;
    const anchor = this.findFocusAnchor(target);
    if (anchor) this.schedule(anchor, true);
  }

  private onBlur(event: Event): void {
    const target = event.target instanceof Node ? event.target : null;
    if (this.anchor && target && (target === this.anchor.el || target.contains(this.anchor.el))) this.hide(true);
  }

  private findAnchor(target: Element): Anchor | null {
    if (target.closest(IGNORED_SELECTOR)) return null;
    const control = target.closest(CONTROL_SELECTOR);
    if (control?.getAttribute("aria-expanded") === "true") return null;
    let el: Element | null = target;
    for (let depth = 0; el && el !== this.doc.body && depth < MAX_DEPTH; depth++, el = el.parentElement) {
      if (this.isCut(el)) return { el, text: textOf(el), align: "start" };
      if (el === control) break;
    }
    return control ? this.labelAnchor(control) : null;
  }

  private findFocusAnchor(control: Element): Anchor | null {
    if (control.closest(IGNORED_SELECTOR) || !control.matches(CONTROL_SELECTOR)) return null;
    if (control.getAttribute("aria-expanded") === "true") return null;
    const cut = [control, ...Array.from(control.querySelectorAll("*"))].find((el) => this.isCut(el));
    if (cut) return { el: control, text: textOf(cut), align: "start" };
    return this.labelAnchor(control);
  }

  private labelAnchor(control: Element): Anchor | null {
    const visible = control instanceof HTMLElement ? control.innerText : (control.textContent ?? "");
    const text = iconLabel(visible, control.getAttribute("aria-label"));
    return text ? { el: control, text, align: "center" } : null;
  }

  private isCut(el: Element): boolean {
    const view = this.doc.defaultView;
    if (!view) return false;
    const style = view.getComputedStyle(el);
    const lineClamp = style.getPropertyValue("-webkit-line-clamp");
    if (!canTruncate({ textOverflow: style.textOverflow, lineClamp })) return false;
    return isTruncated({
      textOverflow: style.textOverflow,
      lineClamp,
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
    });
  }

  private schedule(anchor: Anchor, immediate: boolean): void {
    clearTimeout(this.timer);
    this.anchor = anchor;
    const delay = immediate ? 0 : openDelay(this.lastClosedAt, performance.now(), this.open);
    if (delay === 0) {
      this.show(anchor, true);
      return;
    }
    this.timer = setTimeout(() => this.show(anchor, false), delay);
  }

  private show(anchor: Anchor, instant: boolean): void {
    if (this.anchor !== anchor || !anchor.el.isConnected || !anchor.text) return;
    const tip = this.tip().nativeElement;
    tip.classList.toggle("instant", instant);
    tip.textContent = anchor.text;
    const view = this.doc.defaultView!;
    const place = placeTooltip(
      anchor.el.getBoundingClientRect(),
      { width: tip.offsetWidth, height: tip.offsetHeight },
      { width: view.innerWidth, height: view.innerHeight },
      anchor.align,
    );
    tip.style.left = `${place.left}px`;
    tip.style.top = `${place.top}px`;
    tip.style.transformOrigin = `${place.originX}px ${place.side === "top" ? "bottom" : "top"}`;
    tip.classList.add("open");
    this.open = true;
  }

  private hide(immediate: boolean): void {
    clearTimeout(this.timer);
    this.anchor = null;
    if (!this.open) return;
    const tip = this.tip().nativeElement;
    tip.classList.toggle("instant", immediate);
    tip.classList.remove("open");
    this.open = false;
    this.lastClosedAt = performance.now();
  }
}
