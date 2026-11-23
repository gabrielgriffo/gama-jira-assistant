import { ChangeDetectionStrategy, Component, input, output } from "@angular/core";
import { IconComponent } from "../../../shared/ui/icon/icon.component";

export type SetupStep = "theme" | "jira" | "pontomais" | "date" | "import";
export type ChecklistState = "current" | "done" | "skipped" | "pending";

export interface ChecklistItem {
  id: SetupStep;
  label: string;
  detail: string;
  state: ChecklistState;
  enabled: boolean;
}

@Component({
  selector: "app-onboarding-checklist",
  imports: [IconComponent],
  templateUrl: "./onboarding-checklist.component.html",
  styleUrl: "./onboarding-checklist.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class OnboardingChecklistComponent {
  readonly items = input.required<ChecklistItem[]>();
  readonly canSkipAll = input(false);
  readonly busy = input(false);
  readonly pick = output<SetupStep>();
  readonly skipAll = output<void>();
}
