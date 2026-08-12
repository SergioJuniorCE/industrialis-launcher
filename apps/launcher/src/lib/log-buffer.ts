export const MAX_RETAINED_LOG_LINES = 5_000;

export function takeLogTail<T>(items: readonly T[], maxLines = MAX_RETAINED_LOG_LINES): T[] {
  if (maxLines <= 0) return [];
  return items.length > maxLines ? items.slice(-maxLines) : [...items];
}

export function appendLogTail<T>(current: readonly T[], incoming: readonly T[], maxLines = MAX_RETAINED_LOG_LINES): T[] {
  if (maxLines <= 0) return [];
  if (incoming.length >= maxLines) return incoming.slice(-maxLines);

  const retainedCurrent = Math.max(0, maxLines - incoming.length);
  const retained = retainedCurrent === 0 ? [] : current.slice(-retainedCurrent);
  return [...retained, ...incoming];
}
