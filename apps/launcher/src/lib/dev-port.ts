export const DEFAULT_DEV_PORT = 5173;

/**
 * Resolve the Vite dev-server port. `VITE_PORT` wins over `PORT` so a
 * repo-wide `PORT` (used by other apps) never silently moves the launcher
 * renderer that Electron expects at a fixed URL. Only strict decimal ports
 * 1–65535 are accepted — anything else (empty, `5197abc`, `-1`, `0`,
 * out-of-range) falls back to the default. `strictPort` stays enabled in
 * vite.config.ts, so a collision fails fast instead of silently serving
 * the launcher on an unexpected port.
 */
export function resolveDevPort(env: NodeJS.ProcessEnv = process.env): number {
  const raw = (env.VITE_PORT ?? env.PORT ?? String(DEFAULT_DEV_PORT)).trim();
  if (!/^\d+$/.test(raw)) return DEFAULT_DEV_PORT;
  const port = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(port) || port < 1 || port > 65_535) return DEFAULT_DEV_PORT;
  return port;
}
