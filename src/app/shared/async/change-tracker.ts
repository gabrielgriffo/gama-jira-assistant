export function changeTracker<T>(): (value: T | null) => boolean {
  let seen = false;
  let last: T | null = null;
  return (value) => {
    if (value === null) return false;
    if (!seen) {
      seen = true;
      last = value;
      return false;
    }
    if (value === last) return false;
    last = value;
    return true;
  };
}
