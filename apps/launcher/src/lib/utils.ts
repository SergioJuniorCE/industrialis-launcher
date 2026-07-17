import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function keyedByOccurrence<T>(
  items: readonly T[],
  getBaseKey: (item: T) => string,
): Array<{ key: string; value: T }> {
  const counts = new Map<string, number>();
  return items.map((value) => {
    const baseKey = getBaseKey(value);
    const occurrence = counts.get(baseKey) ?? 0;
    counts.set(baseKey, occurrence + 1);
    return { key: `${baseKey}:${occurrence}`, value };
  });
}
