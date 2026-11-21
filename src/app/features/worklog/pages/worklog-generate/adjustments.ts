interface AdjustableDay {
  date: string;
  totalMinutes: number;
  tasksWorked: { issueId: string; workedMinutes: number }[];
}

interface AdjustableTask {
  issueId: string;
}

export class Adjustments {
  private readonly values = new Map<string, { base: string; delta: number }>();

  get(day: AdjustableDay, task: AdjustableTask): number {
    const stored = this.values.get(key(day, task));
    return stored && stored.base === dayBase(day) ? stored.delta : 0;
  }

  set(day: AdjustableDay, task: AdjustableTask, delta: number): void {
    this.values.set(key(day, task), { base: dayBase(day), delta });
  }

  delete(day: AdjustableDay, task: AdjustableTask): void {
    this.values.delete(key(day, task));
  }

  clearDay(date: string): void {
    [...this.values.keys()].filter((k) => k.startsWith(`${date}|`)).forEach((k) => this.values.delete(k));
  }

  clear(): void {
    this.values.clear();
  }
}

function key(day: AdjustableDay, task: AdjustableTask): string {
  return `${day.date}|${task.issueId}`;
}

function dayBase(day: AdjustableDay): string {
  return [day.totalMinutes, ...day.tasksWorked.map((t) => `${t.issueId}:${t.workedMinutes}`)].join("|");
}
