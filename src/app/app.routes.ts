import { Routes } from "@angular/router";
import { TASKS_ROUTES } from "./features/tasks/tasks.routes";
import { WORKLOG_ROUTES } from "./features/worklog/worklog.routes";

export const routes: Routes = [
  ...TASKS_ROUTES,
  ...WORKLOG_ROUTES,
  {
    path: "",
    redirectTo: "/tasks",
    pathMatch: "full",
  },
];
