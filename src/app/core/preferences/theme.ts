export type Theme = "light" | "dark" | "system";

const THEMES: readonly Theme[] = ["light", "dark", "system"];

export function isTheme(value: unknown): value is Theme {
  return typeof value === "string" && (THEMES as readonly string[]).includes(value);
}

export function resolveTheme(cached: unknown, saved: unknown): Theme {
  if (isTheme(cached)) return cached;
  if (isTheme(saved)) return saved;
  return "light";
}
