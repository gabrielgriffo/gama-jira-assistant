import { Routes } from "@angular/router";

export const WORKLOG_ROUTES: Routes = [
  {
    path: "worklog",
    loadComponent: () => import("./pages/worklog-generate/worklog-generate.page").then((m) => m.WorklogGeneratePage),
  },
  {
    path: "worklog-history",
    loadComponent: () => import("./pages/worklog-history/worklog-history.page").then((m) => m.WorklogHistoryPage),
  },
];
