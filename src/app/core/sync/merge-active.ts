export function mergeActiveSyncs(snapshot: string[], events: Map<string, boolean>): Set<string> {
  const active = new Set(snapshot);
  events.forEach((running, key) => (running ? active.add(key) : active.delete(key)));
  return active;
}
