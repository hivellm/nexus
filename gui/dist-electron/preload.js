"use strict";
const electron = require("electron");
electron.contextBridge.exposeInMainWorld("electronAPI", {
  // App info
  getVersion: () => electron.ipcRenderer.invoke("app:getVersion"),
  getPlatform: () => electron.ipcRenderer.invoke("app:getPlatform"),
  // File dialogs
  openFile: (options) => electron.ipcRenderer.invoke("dialog:openFile", options),
  saveFile: (options) => electron.ipcRenderer.invoke("dialog:saveFile", options),
  // File system
  readFile: (filePath) => electron.ipcRenderer.invoke("fs:readFile", filePath),
  writeFile: (filePath, content) => electron.ipcRenderer.invoke("fs:writeFile", filePath, content),
  // Window controls
  minimizeWindow: () => electron.ipcRenderer.send("window:minimize"),
  maximizeWindow: () => electron.ipcRenderer.send("window:maximize"),
  closeWindow: () => electron.ipcRenderer.send("window:close"),
  isMaximized: () => electron.ipcRenderer.invoke("window:isMaximized"),
  // Notifications
  showNotification: (options) => electron.ipcRenderer.send("notification:show", options),
  // Shell
  openExternal: (url) => electron.ipcRenderer.invoke("shell:openExternal", url),
  // Listen for messages from main process
  onMainMessage: (callback) => {
    electron.ipcRenderer.on("main-process-message", (_, message) => callback(message));
  }
});
