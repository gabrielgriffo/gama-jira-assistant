export interface TextMetrics {
  textOverflow: string;
  lineClamp: string;
  scrollWidth: number;
  clientWidth: number;
  scrollHeight: number;
  clientHeight: number;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Placement {
  left: number;
  top: number;
  side: "top" | "bottom";
  originX: number;
}

export type Align = "start" | "center";

export const OPEN_DELAY_MS = 600;
export const SKIP_DELAY_MS = 300;
export const CONTROL_SELECTOR = 'button, a[href], [role="button"], [role="option"], [role="tab"], [role="switch"]';
export const IGNORED_SELECTOR = 'input, select, textarea, [contenteditable="true"], app-titlebar .controls';

const GAP = 6;
const MARGIN = 8;
const PADDING_X = 8;

export function canTruncate(metrics: Pick<TextMetrics, "textOverflow" | "lineClamp">): boolean {
  return metrics.textOverflow === "ellipsis" || isClamped(metrics.lineClamp);
}

export function isTruncated(metrics: TextMetrics): boolean {
  if (metrics.textOverflow === "ellipsis" && metrics.scrollWidth > metrics.clientWidth) return true;
  return isClamped(metrics.lineClamp) && metrics.scrollHeight > metrics.clientHeight;
}

export function openDelay(lastClosedAt: number | null, now: number, open: boolean): number {
  if (open) return 0;
  if (lastClosedAt !== null && now - lastClosedAt <= SKIP_DELAY_MS) return 0;
  return OPEN_DELAY_MS;
}

export function tooltipText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

export function iconLabel(visibleText: string, ariaLabel: string | null): string | null {
  if (tooltipText(visibleText)) return null;
  return tooltipText(ariaLabel ?? "") || null;
}

export function placeTooltip(
  anchor: Box,
  tip: { width: number; height: number },
  viewport: { width: number; height: number },
  align: Align = "start",
): Placement {
  const maxLeft = Math.max(MARGIN, viewport.width - MARGIN - tip.width);
  const wanted = align === "center" ? anchor.left + anchor.width / 2 - tip.width / 2 : anchor.left - PADDING_X;
  const left = Math.round(Math.min(Math.max(wanted, MARGIN), maxLeft));
  const pointX = align === "center" ? anchor.left + anchor.width / 2 : anchor.left + PADDING_X;
  const above = anchor.top - GAP - tip.height;
  const fitsAbove = above >= MARGIN;
  const fitsBelow = anchor.top + anchor.height + GAP + tip.height <= viewport.height - MARGIN;
  const side = fitsAbove || !fitsBelow ? "top" : "bottom";
  const top = Math.round(side === "top" ? Math.max(above, MARGIN) : anchor.top + anchor.height + GAP);
  const originX = Math.round(Math.min(Math.max(pointX - left, 0), tip.width));
  return { left, top, side, originX };
}

function isClamped(lineClamp: string): boolean {
  return lineClamp !== "" && lineClamp !== "none";
}
