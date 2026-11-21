import type { Issue } from "../../../../core/models/issue";
import { compareIssueKey, compareLastChange } from "../../../../shared/sort/last-change";

export type TaskTab = "all" | "new" | "development" | "done";
export type TaskCategory = "new" | "indeterminate" | "done";

export interface TaskTabCount {
  id: TaskTab;
  label: string;
  count: number;
}

type FilterableTask = Pick<Issue, "key" | "summary" | "statusCategoryKey" | "statusIsDevelopment">;

const TAB_LABELS: Record<TaskTab, string> = {
  all: "Todas",
  new: "A fazer",
  development: "Em desenvolvimento",
  done: "Concluído",
};

const TAB_ORDER: TaskTab[] = ["all", "development", "new", "done"];

export function categoryOf(task: FilterableTask): TaskCategory {
  if (task.statusCategoryKey === "new" || task.statusCategoryKey === "done") return task.statusCategoryKey;
  return "indeterminate";
}

export function tabCounts(tasks: FilterableTask[]): TaskTabCount[] {
  return TAB_ORDER.map((id) => ({
    id,
    label: TAB_LABELS[id],
    count: tasks.filter((task) => inTab(task, id)).length,
  }));
}

function inTab(task: FilterableTask, tab: TaskTab): boolean {
  if (tab === "all") return true;
  if (tab === "development") return task.statusIsDevelopment;
  return categoryOf(task) === tab;
}

export function filterTasks<T extends FilterableTask>(tasks: T[], tab: TaskTab, query: string): T[] {
  const needle = normalize(query);
  return tasks.filter(
    (task) =>
      inTab(task, tab) && (!needle || normalize(task.key).includes(needle) || normalize(task.summary).includes(needle)),
  );
}

function normalize(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().trim();
}

type SortableTask = Pick<Issue, "key" | "lastChangeAt" | "statusIsDevelopment">;

export function sortTasks<T extends SortableTask>(tasks: readonly T[], developmentFirst: boolean): T[] {
  return [...tasks].sort(
    (a, b) =>
      (developmentFirst ? Number(b.statusIsDevelopment) - Number(a.statusIsDevelopment) : 0) ||
      compareLastChange(a.lastChangeAt, b.lastChangeAt) ||
      compareIssueKey(a.key, b.key),
  );
}
