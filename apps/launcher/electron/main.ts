import path from "node:path";
import { pathToFileURL } from "node:url";
import { app, BrowserWindow, ipcMain, Menu, net, protocol, screen, shell } from "electron";
import squirrelStartup from "electron-squirrel-startup";
import { LauncherBackend } from "./backend/index";
import { sanitizeMinecraftRelPath } from "./backend/minecraft-files";
import { dataDir, validateInstanceId } from "./backend/paths";
import { loadLauncherSettings } from "./backend/settings";
import type { LauncherSettings } from "./backend/types";

declare const MAIN_WINDOW_VITE_DEV_SERVER_URL: string | undefined;
declare const MAIN_WINDOW_VITE_NAME: string;

if (squirrelStartup) {
  app.quit();
}

let mainWindow: BrowserWindow | null = null;
let backend: LauncherBackend | null = null;
let isQuitting = false;
const editorWindows = new Map<string, BrowserWindow>();

function emitToRenderer(event: string, payload: unknown): void {
  for (const window of BrowserWindow.getAllWindows()) {
    window.webContents.send(`launcher:event:${event}`, payload);
  }
}

function createWindow(settings: LauncherSettings): BrowserWindow {
  const window = new BrowserWindow({
    width: settings.window_width,
    height: settings.window_height,
    minWidth: 800,
    minHeight: 600,
    title: "Industrialis Launcher",
    titleBarStyle: "hidden",
    ...(process.platform === "darwin" ? { trafficLightPosition: { x: 14, y: 14 } } : {}),
    backgroundColor: "#0a0a0a",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  window.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url);
    return { action: "deny" };
  });

  window.on("closed", () => {
    if (mainWindow === window) mainWindow = null;
  });

  const emitMaximizedState = () => {
    window.webContents.send("launcher:event:window-maximized", {
      maximized: window.isMaximized(),
    });
  };
  window.on("maximize", emitMaximizedState);
  window.on("unmaximize", emitMaximizedState);
  if (settings.launch_maximized) window.maximize();

  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    void window.loadURL(MAIN_WINDOW_VITE_DEV_SERVER_URL);
  } else {
    void window.loadFile(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));
  }
  return window;
}

function createMinecraftEditorWindow(rawInstanceId: string, rawFilePath?: string): BrowserWindow {
  const instanceId = validateInstanceId(rawInstanceId);
  const filePath = rawFilePath ? sanitizeMinecraftRelPath(rawFilePath) : undefined;
  const key = JSON.stringify([instanceId, filePath ?? null]);
  const existing = editorWindows.get(key);
  if (existing && !existing.isDestroyed()) {
    if (existing.isMinimized()) existing.restore();
    existing.focus();
    return existing;
  }

  const display = mainWindow ? screen.getDisplayMatching(mainWindow.getBounds()) : screen.getPrimaryDisplay();
  const { x: areaX, y: areaY, width: areaWidth, height: areaHeight } = display.workArea;
  const width = Math.min(1200, areaWidth);
  const height = Math.min(820, areaHeight);
  const window = new BrowserWindow({
    x: areaX + Math.round((areaWidth - width) / 2),
    y: areaY + Math.round((areaHeight - height) / 2),
    width,
    height,
    minWidth: Math.min(900, width),
    minHeight: Math.min(650, height),
    resizable: true,
    maximizable: true,
    title: `${instanceId} — Minecraft file editor`,
    autoHideMenuBar: true,
    backgroundColor: "#0a0a0a",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  editorWindows.set(key, window);
  window.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url);
    return { action: "deny" };
  });
  window.on("closed", () => {
    if (editorWindows.get(key) === window) editorWindows.delete(key);
  });

  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    const url = new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL);
    url.searchParams.set("editorInstance", instanceId);
    if (filePath) url.searchParams.set("editorPath", filePath);
    void window.loadURL(url.toString());
  } else {
    void window.loadFile(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`), {
      query: { editorInstance: instanceId, ...(filePath ? { editorPath: filePath } : {}) },
    });
  }
  return window;
}

function validateSender(event: Electron.IpcMainInvokeEvent): void {
  const senderWindow = BrowserWindow.fromWebContents(event.sender);
  const isMainWindow = senderWindow !== null && senderWindow === mainWindow;
  const isEditorWindow = senderWindow !== null && [...editorWindows.values()].some((window) => window === senderWindow);
  if (!isMainWindow && !isEditorWindow) throw new Error("Unauthorized renderer");
}

function registerIpcHandlers(): void {
  ipcMain.handle("launcher:invoke", async (event, command: string, args: unknown) => {
    validateSender(event);
    if (!backend) throw new Error("Launcher backend is not ready");
    return backend.invoke(command, args);
  });

  ipcMain.handle("launcher:open-minecraft-editor-window", (event, rawArgs: unknown) => {
    validateSender(event);
    if (event.sender !== mainWindow?.webContents) throw new Error("Only the launcher window can open an editor window");
    const args = (rawArgs && typeof rawArgs === "object" ? rawArgs : {}) as { instanceId?: unknown; filePath?: unknown };
    const instanceId = String(args.instanceId ?? "");
    const filePath = args.filePath == null ? undefined : String(args.filePath);
    createMinecraftEditorWindow(instanceId, filePath);
  });

  ipcMain.handle("launcher:open-url", async (event, url: string) => {
    validateSender(event);
    if (!/^https?:\/\//i.test(url)) throw new Error("Only HTTP(S) URLs may be opened");
    await shell.openExternal(url);
  });

  ipcMain.handle("launcher:hide-window", (event) => {
    validateSender(event);
    BrowserWindow.fromWebContents(event.sender)?.hide();
  });

  ipcMain.handle("launcher:minimize-window", (event) => {
    validateSender(event);
    BrowserWindow.fromWebContents(event.sender)?.minimize();
  });

  ipcMain.handle("launcher:toggle-maximize-window", (event) => {
    validateSender(event);
    const window = BrowserWindow.fromWebContents(event.sender);
    if (!window) return false;
    const maximized = !window.isMaximized();
    if (maximized) window.maximize();
    else window.unmaximize();
    return maximized;
  });

  ipcMain.handle("launcher:is-window-maximized", (event) => {
    validateSender(event);
    return BrowserWindow.fromWebContents(event.sender)?.isMaximized() ?? false;
  });

  ipcMain.handle("launcher:close-window", (event) => {
    validateSender(event);
    const window = BrowserWindow.fromWebContents(event.sender);
    if (window) setImmediate(() => window.close());
  });
}

function registerFileProtocol(): void {
  protocol.handle("industrialis-file", (request) => {
    const encodedPath = new URL(request.url).pathname.replace(/^\//u, "");
    const filePath = path.resolve(decodeURIComponent(encodedPath));
    const root = path.resolve(dataDir());
    if (filePath !== root && !filePath.startsWith(`${root}${path.sep}`)) {
      return new Response("Forbidden", { status: 403 });
    }
    return net.fetch(pathToFileURL(filePath).toString());
  });
}

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  app.on("second-instance", (_event, commandLine) => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
    backend?.handleDeepLinks(commandLine.filter((value) => value.startsWith("industrialislauncher:")));
  });

  app.whenReady().then(async () => {
    Menu.setApplicationMenu(null);
    app.setAsDefaultProtocolClient("industrialislauncher");
    registerFileProtocol();
    registerIpcHandlers();
    const launcherSettings = await loadLauncherSettings();
    backend = new LauncherBackend({ emit: emitToRenderer });
    mainWindow = createWindow(launcherSettings);
    backend.handleDeepLinks(process.argv.filter((value) => value.startsWith("industrialislauncher:")));
  });
}

app.on("open-url", (event, url) => {
  event.preventDefault();
  backend?.handleDeepLinks([url]);
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("before-quit", (event) => {
  if (isQuitting || !backend) return;
  event.preventDefault();
  isQuitting = true;
  void backend
    .dispose()
    .catch(() => undefined)
    .finally(() => app.quit());
});
