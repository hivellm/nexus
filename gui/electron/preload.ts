import { contextBridge, ipcRenderer } from 'electron';

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld('electronAPI', {
  // App info
  getVersion: () => ipcRenderer.invoke('app:getVersion'),
  getPlatform: () => ipcRenderer.invoke('app:getPlatform'),

  // File dialogs
  openFile: (options?: {
    title?: string;
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string;
  }) => ipcRenderer.invoke('dialog:openFile', options),

  saveFile: (options?: {
    title?: string;
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string;
  }) => ipcRenderer.invoke('dialog:saveFile', options),

  // File system
  readFile: (filePath: string) => ipcRenderer.invoke('fs:readFile', filePath),
  writeFile: (filePath: string, content: string) => ipcRenderer.invoke('fs:writeFile', filePath, content),

  // Window controls
  minimizeWindow: () => ipcRenderer.send('window:minimize'),
  maximizeWindow: () => ipcRenderer.send('window:maximize'),
  closeWindow: () => ipcRenderer.send('window:close'),
  isMaximized: () => ipcRenderer.invoke('window:isMaximized'),

  // Notifications
  showNotification: (options: { title: string; body: string }) =>
    ipcRenderer.send('notification:show', options),

  // Shell
  openExternal: (url: string) => ipcRenderer.invoke('shell:openExternal', url),

  // Listen for messages from main process
  onMainMessage: (callback: (message: string) => void) => {
    ipcRenderer.on('main-process-message', (_, message) => callback(message));
  },
});

// Type definitions for the exposed API
declare global {
  interface Window {
    electronAPI: {
      getVersion: () => Promise<string>;
      getPlatform: () => Promise<string>;
      openFile: (options?: {
        title?: string;
        filters?: { name: string; extensions: string[] }[];
        defaultPath?: string;
      }) => Promise<string | null>;
      saveFile: (options?: {
        title?: string;
        filters?: { name: string; extensions: string[] }[];
        defaultPath?: string;
      }) => Promise<string | null>;
      readFile: (filePath: string) => Promise<string>;
      writeFile: (filePath: string, content: string) => Promise<void>;
      minimizeWindow: () => void;
      maximizeWindow: () => void;
      closeWindow: () => void;
      isMaximized: () => Promise<boolean>;
      showNotification: (options: { title: string; body: string }) => void;
      openExternal: (url: string) => Promise<void>;
      onMainMessage: (callback: (message: string) => void) => void;
    };
  }
}
