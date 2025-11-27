"use strict";
const electron = require("electron");
const path = require("node:path");
const fs = require("node:fs/promises");
process.env.DIST = path.join(__dirname, "../dist");
process.env.VITE_PUBLIC = electron.app.isPackaged ? process.env.DIST : path.join(process.env.DIST, "../public");
let win = null;
const preload = path.join(__dirname, "preload.cjs");
const VITE_DEV_SERVER_URL = process.env["VITE_DEV_SERVER_URL"];
function createWindow() {
  win = new electron.BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 1e3,
    minHeight: 700,
    icon: path.join(process.env.VITE_PUBLIC || "", "icon.png"),
    frame: false,
    transparent: false,
    backgroundColor: "#0f172a",
    webPreferences: {
      preload,
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: false
    },
    ...process.platform === "darwin" && {
      titleBarStyle: "hiddenInset",
      trafficLightPosition: { x: 10, y: 10 }
    }
  });
  win.webContents.on("did-finish-load", () => {
    win == null ? void 0 : win.webContents.send("main-process-message", (/* @__PURE__ */ new Date()).toLocaleString());
  });
  if (VITE_DEV_SERVER_URL) {
    win.loadURL(VITE_DEV_SERVER_URL);
    win.webContents.openDevTools();
  } else {
    win.loadFile(path.join(process.env.DIST, "index.html"));
  }
}
electron.app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    electron.app.quit();
    win = null;
  }
});
electron.app.on("activate", () => {
  if (electron.BrowserWindow.getAllWindows().length === 0) {
    createWindow();
  }
});
electron.app.whenReady().then(createWindow);
electron.ipcMain.handle("app:getVersion", () => {
  return electron.app.getVersion();
});
electron.ipcMain.handle("app:getPlatform", () => {
  return process.platform;
});
electron.ipcMain.handle("dialog:openFile", async (_, options) => {
  if (!win) return null;
  const result = await electron.dialog.showOpenDialog(win, {
    title: (options == null ? void 0 : options.title) || "Open File",
    filters: (options == null ? void 0 : options.filters) || [
      { name: "Cypher Files", extensions: ["cypher", "cql"] },
      { name: "JSON Files", extensions: ["json"] },
      { name: "All Files", extensions: ["*"] }
    ],
    defaultPath: options == null ? void 0 : options.defaultPath,
    properties: ["openFile"]
  });
  if (result.canceled || result.filePaths.length === 0) {
    return null;
  }
  return result.filePaths[0];
});
electron.ipcMain.handle("dialog:saveFile", async (_, options) => {
  if (!win) return null;
  const result = await electron.dialog.showSaveDialog(win, {
    title: (options == null ? void 0 : options.title) || "Save File",
    filters: (options == null ? void 0 : options.filters) || [
      { name: "JSON Files", extensions: ["json"] },
      { name: "CSV Files", extensions: ["csv"] },
      { name: "All Files", extensions: ["*"] }
    ],
    defaultPath: options == null ? void 0 : options.defaultPath
  });
  if (result.canceled || !result.filePath) {
    return null;
  }
  return result.filePath;
});
electron.ipcMain.handle("fs:readFile", async (_, filePath) => {
  try {
    const content = await fs.readFile(filePath, "utf-8");
    return content;
  } catch (error) {
    throw new Error(`Failed to read file: ${error.message}`);
  }
});
electron.ipcMain.handle("fs:writeFile", async (_, filePath, content) => {
  try {
    await fs.writeFile(filePath, content, "utf-8");
  } catch (error) {
    throw new Error(`Failed to write file: ${error.message}`);
  }
});
electron.ipcMain.on("window:minimize", () => {
  if (win) {
    win.minimize();
  }
});
electron.ipcMain.on("window:maximize", () => {
  if (win) {
    if (win.isMaximized()) {
      win.unmaximize();
    } else {
      win.maximize();
    }
  }
});
electron.ipcMain.on("window:close", () => {
  if (win) {
    win.close();
  }
});
electron.ipcMain.handle("window:isMaximized", () => {
  return (win == null ? void 0 : win.isMaximized()) || false;
});
electron.ipcMain.on("notification:show", (_, options) => {
  if (electron.Notification.isSupported()) {
    new electron.Notification({
      title: options.title,
      body: options.body
    }).show();
  }
});
electron.ipcMain.handle("shell:openExternal", async (_, url) => {
  await electron.shell.openExternal(url);
});
