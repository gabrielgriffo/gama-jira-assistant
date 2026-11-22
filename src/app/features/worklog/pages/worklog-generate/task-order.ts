interface OrderableTask {
  key: string;
  devSegments: { start: number }[];
  countedSegments: { start: number }[];
}

function firstStart(task: OrderableTask): number {
  return task.countedSegments[0]?.start ?? task.devSegments[0]?.start ?? Number.POSITIVE_INFINITY;
}

export function chronologicalOrder<T extends OrderableTask>(tasks: readonly T[]): T[] {
  return [...tasks].sort((a, b) => firstStart(a) - firstStart(b) || a.key.localeCompare(b.key));
}
