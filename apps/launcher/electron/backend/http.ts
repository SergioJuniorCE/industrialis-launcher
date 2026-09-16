export const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;

export interface JsonResult {
  status: number;
  body: any;
}

export interface RequestJsonOptions {
  timeoutMs?: number;
}

export async function requestJson(url: string, init: RequestInit, options: RequestJsonOptions = {}): Promise<JsonResult> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
  let response: Response;
  try {
    response = await fetch(url, { ...init, signal: init.signal ?? AbortSignal.timeout(timeoutMs) });
  } catch (error) {
    const timedOut = error instanceof Error && error.name === "TimeoutError";
    throw new Error(
      timedOut
        ? `request to ${url} timed out after ${timeoutMs / 1000}s. Check your internet connection and try again.`
        : `request to ${url} failed (${error instanceof Error ? error.message : String(error)}). Check your internet connection and try again.`,
    );
  }
  const text = await response.text();
  let body: any = {};
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = { raw: text };
  }
  return { status: response.status, body };
}

export function formatHttpResult(result: { status: number; body: unknown }): string {
  return `HTTP ${result.status}: ${JSON.stringify(result.body)}`;
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export interface RetryOptions {
  maxAttempts: number;
  baseDelayMs: number;
  maxDelayMs?: number;
  shouldRetry?: (error: unknown) => boolean;
}

export async function retryWithBackoff<T>(operation: () => Promise<T>, options: RetryOptions): Promise<T> {
  const { maxAttempts, baseDelayMs, maxDelayMs = 30_000, shouldRetry = () => true } = options;
  if (!Number.isInteger(maxAttempts) || maxAttempts < 1) throw new Error("maxAttempts must be a positive integer");
  let attempt = 0;
  for (;;) {
    attempt += 1;
    try {
      return await operation();
    } catch (error) {
      if (attempt >= maxAttempts || !shouldRetry(error)) throw error;
      await sleep(Math.min(baseDelayMs * 2 ** (attempt - 1), maxDelayMs));
    }
  }
}
