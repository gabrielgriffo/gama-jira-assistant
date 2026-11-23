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

export const OPEN_DELAY_MS = 600;
export const SKIP_DELAY_MS = 300;
export const CONTROL_SELECTOR =
  'button, a[href], input, select, textarea, [role="button"], [role="option"], [role="tab"], [role="switch"], [contenteditable="true"]';

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

export function placeTooltip(
  anchor: Box,
  tip: { width: number; height: number },
  viewport: { width: number; height: number },
): Placement {
  const maxLeft = Math.max(MARGIN, viewport.width - MARGIN - tip.width);
  const left = Math.round(Math.min(Math.max(anchor.left - PADDING_X, MARGIN), maxLeft));
  const above = anchor.top - GAP - tip.height;
  const fitsAbove = above >= MARGIN;
  const fitsBelow = anchor.top + anchor.height + GAP + tip.height <= viewport.height - MARGIN;
  const side = fitsAbove || !fitsBelow ? "top" : "bottom";
  const top = Math.round(side === "top" ? Math.max(above, MARGIN) : anchor.top + anchor.height + GAP);
  const originX = Math.round(Math.min(Math.max(anchor.left + PADDING_X - left, 0), tip.width));
  return { left, top, side, originX };
}

function isClamped(lineClamp: string): boolean {
  return lineClamp !== "" && lineClamp !== "none";
}
