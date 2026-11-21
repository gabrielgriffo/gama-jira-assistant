import { Routes } from "@angular/router";

export const TASKS_ROUTES: Routes = [
  {
    path: "tasks",
    loadComponent: () => import("./pages/tasks/tasks.page").then((m) => m.TasksPage),
  },
];
