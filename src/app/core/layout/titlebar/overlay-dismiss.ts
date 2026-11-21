export const POINTER_SLOP = 4;

export function topOverlay(overlays: ArrayLike<Pick<Element, "closest">>): Element | null {
  for (let i = overlays.length - 1; i >= 0; i--) {
    const overlay = overlays[i];
    if (!overlay.closest(".modal-leave")) return overlay as Element;
  }
  return null;
}

export function leftSlop(start: { x: number; y: number }, now: { x: number; y: number }): boolean {
  return Math.abs(now.x - start.x) > POINTER_SLOP || Math.abs(now.y - start.y) > POINTER_SLOP;
}
