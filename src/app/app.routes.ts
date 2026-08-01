import { Routes } from "@angular/router";

export const routes: Routes = [
  {
    path: "tasks",
    loadComponent: () => import("./features/tasks/tasks.page").then((m) => m.TasksPage),
  },
  {
    path: "worklog",
    loadComponent: () => import("./features/worklog-generate/worklog-generate.page").then((m) => m.WorklogGeneratePage),
  },
  {
    path: "worklog-history",
    loadComponent: () => import("./features/worklog-history/worklog-history.page").then((m) => m.WorklogHistoryPage),
  },
  {
    path: "settings",
    loadComponent: () => import("./features/settings/settings.page").then((m) => m.SettingsPage),
  },
  {
    path: "",
    redirectTo: "/tasks",
    pathMatch: "full",
  },
];
