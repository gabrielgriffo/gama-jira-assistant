export type InputModality = "keyboard" | "pointer";

const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "AltGraph"]);

export function modalityOf(event: { type: string; key?: string }): InputModality | null {
  if (event.type === "pointerdown") return "pointer";
  if (event.type === "keydown" && !MODIFIERS.has(event.key ?? "")) return "keyboard";
  return null;
}

export function trackInputModality(doc: Pick<Document, "addEventListener" | "documentElement">): void {
  const update = (event: Event) => {
    const modality = modalityOf(event as KeyboardEvent);
    if (modality) doc.documentElement.dataset["input"] = modality;
  };
  doc.addEventListener("keydown", update, true);
  doc.addEventListener("pointerdown", update, true);
}
