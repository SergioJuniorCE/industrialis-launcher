export interface DesktopEvent {
  payload: unknown;
}

export interface ElectronLauncherApi {
  invoke(command: string, args?: unknown): Promise<unknown>;
  listen(event: string, listener: (payload: unknown) => void): () => void;
  openUrl(url: string): Promise<void>;
  convertFileSrc(path: string): string;
  hideWindow(): Promise<void>;
}

declare global {
  interface Window {
    electronAPI?: ElectronLauncherApi;
  }
}

function getApi(): ElectronLauncherApi {
  if (!window.electronAPI) {
    throw new Error("The Electron launcher API is unavailable. Start the desktop app.");
  }
  return window.electronAPI;
}

export function isDesktop(): boolean {
  return Boolean(window.electronAPI);
}

export function invoke<T>(command: string, args?: unknown): Promise<T> {
  return getApi().invoke(command, args) as Promise<T>;
}

export function listen<T>(
  event: string,
  listener: (event: DesktopEvent & { payload: T }) => void,
): Promise<() => void> {
  if (!window.electronAPI) return Promise.resolve(() => undefined);
  const unsubscribe = getApi().listen(event, (payload) => listener({ payload: payload as T }));
  return Promise.resolve(unsubscribe);
}

export function openUrl(url: string): Promise<void> {
  return getApi().openUrl(url);
}

export function convertFileSrc(path: string): string {
  return getApi().convertFileSrc(path);
}

export function hideWindow(): Promise<void> {
  return getApi().hideWindow();
}
