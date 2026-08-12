import path from "node:path";
import { pathToFileURL } from "node:url";
import { app, BrowserWindow, ipcMain, Menu, net, protocol, shell } from "electron";
import squirrelStartup from "electron-squirrel-startup";
import { LauncherBackend } from "./backend/index";
import { dataDir } from "./backend/paths";

declare const MAIN_WINDOW_VITE_DEV_SERVER_URL: string | undefined;
declare const MAIN_WINDOW_VITE_NAME: string;

if (squirrelStartup) {
  app.quit();
}

let mainWindow: BrowserWindow | null = null;
let backend: LauncherBackend | null = null;

function emitToRenderer(event: string, payload: unknown): void {
  for (const window of BrowserWindow.getAllWindows()) {
    window.webContents.send(`launcher:event:${event}`, payload);
  }
}

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1100,
    height: 750,
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

  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    void window.loadURL(MAIN_WINDOW_VITE_DEV_SERVER_URL);
  } else {
    void window.loadFile(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));
  }

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
  return window;
}

function validateSender(event: Electron.IpcMainInvokeEvent): void {
  if (mainWindow && event.sender !== mainWindow.webContents) {
    throw new Error("Unauthorized renderer");
  }
}

function registerIpcHandlers(): void {
  ipcMain.handle("launcher:invoke", async (event, command: string, args: unknown) => {
    validateSender(event);
    if (!backend) throw new Error("Launcher backend is not ready");
    return backend.invoke(command, args);
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

  app.whenReady().then(() => {
    Menu.setApplicationMenu(null);
    app.setAsDefaultProtocolClient("industrialislauncher");
    registerFileProtocol();
    registerIpcHandlers();
    backend = new LauncherBackend({ emit: emitToRenderer });
    mainWindow = createWindow();
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

app.on("before-quit", () => {
  backend?.dispose();
});
