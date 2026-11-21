import type { ChangelogItem, IssueChange } from "../../../core/models";

export interface StatusRef {
  id: string;
  name: string;
  categoryKey: string;
}

const CATEGORY_COLORS: Record<string, string> = {
  new: "var(--cat-new)",
  indeterminate: "var(--cat-ind)",
  done: "var(--cat-done)",
};

const CATEGORY_BADGES: Record<string, string> = {
  new: "",
  indeterminate: "info",
  done: "ok",
};

export function isStatusChange(item: ChangelogItem): boolean {
  return (item.fieldId ?? item.field) === "status";
}

function findStatus(id: string | null, name: string | null, statuses: readonly StatusRef[]): StatusRef | undefined {
  const wanted = name?.trim().toLowerCase();
  return (
    (id ? statuses.find((s) => s.id === id) : undefined) ??
    (wanted ? statuses.find((s) => s.name.trim().toLowerCase() === wanted) : undefined)
  );
}

export function statusDotColor(entry: IssueChange, statuses: readonly StatusRef[]): string | null {
  const change = [...entry.items].reverse().find(isStatusChange);
  if (!change) return null;
  const status = findStatus(change.toId, change.toString, statuses);
  return (status && CATEGORY_COLORS[status.categoryKey]) ?? null;
}

export function statusBadge(id: string | null, name: string | null, statuses: readonly StatusRef[]): string {
  const status = findStatus(id, name, statuses);
  return (status && CATEGORY_BADGES[status.categoryKey]) ?? "";
}
