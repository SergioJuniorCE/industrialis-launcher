import { contextBridge, ipcRenderer } from "electron";

const electronAPI = {
  invoke: (command: string, args?: unknown): Promise<unknown> =>
    ipcRenderer.invoke("launcher:invoke", command, args),
  listen: (event: string, listener: (payload: unknown) => void): (() => void) => {
    const channel = `launcher:event:${event}`;
    const handler = (_event: Electron.IpcRendererEvent, payload: unknown) => listener(payload);
    ipcRenderer.on(channel, handler);
    return () => ipcRenderer.removeListener(channel, handler);
  },
  openUrl: (url: string): Promise<void> => ipcRenderer.invoke("launcher:open-url", url),
  convertFileSrc: (path: string): string =>
    `industrialis-file://local/${encodeURIComponent(path)}`,
  hideWindow: (): Promise<void> => ipcRenderer.invoke("launcher:hide-window"),
};

contextBridge.exposeInMainWorld("electronAPI", electronAPI);
