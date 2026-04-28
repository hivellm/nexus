var __getOwnPropNames = Object.getOwnPropertyNames;
var __commonJS = (cb, mod) => function __require() {
  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
};
import { contextBridge, ipcRenderer } from "electron";
var require_preload = __commonJS({
  "preload.cjs"() {
    contextBridge.exposeInMainWorld("electronAPI", {
      // App info
      getVersion: () => ipcRenderer.invoke("app:getVersion"),
      getPlatform: () => ipcRenderer.invoke("app:getPlatform"),
      // File dialogs
      openFile: (options) => ipcRenderer.invoke("dialog:openFile", options),
      saveFile: (options) => ipcRenderer.invoke("dialog:saveFile", options),
      // File system
      readFile: (filePath) => ipcRenderer.invoke("fs:readFile", filePath),
      writeFile: (filePath, content) => ipcRenderer.invoke("fs:writeFile", filePath, content),
      // Window controls
      minimizeWindow: () => ipcRenderer.send("window:minimize"),
      maximizeWindow: () => ipcRenderer.send("window:maximize"),
      closeWindow: () => ipcRenderer.send("window:close"),
      isMaximized: () => ipcRenderer.invoke("window:isMaximized"),
      // Notifications
      showNotification: (options) => ipcRenderer.send("notification:show", options),
      // Shell
      openExternal: (url) => ipcRenderer.invoke("shell:openExternal", url),
      // Listen for messages from main process
      onMainMessage: (callback) => {
        ipcRenderer.on("main-process-message", (_, message) => callback(message));
      }
    });
  }
});
export default require_preload();
