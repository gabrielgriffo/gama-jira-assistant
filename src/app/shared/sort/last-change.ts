export interface LastChangeSortable {
  key: string;
  lastChangeAt: string | null;
}

export function compareLastChange(a: string | null, b: string | null): number {
  if (a === b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return a < b ? 1 : -1;
}

export function compareIssueKey(a: string, b: string): number {
  return a.localeCompare(b, "pt-BR", { numeric: true });
}

export function sortByLastChange<T extends LastChangeSortable>(items: readonly T[]): T[] {
  return [...items].sort((a, b) => compareLastChange(a.lastChangeAt, b.lastChangeAt) || compareIssueKey(a.key, b.key));
}
