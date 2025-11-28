// IPC bridge for Electron communication
// Falls back to browser APIs when running in development mode without Electron

const isElectron = (): boolean => {
  return typeof window !== 'undefined' && window.electronAPI !== undefined;
};

export const ipcBridge = {
  // Window controls
  minimizeWindow: (): void => {
    if (isElectron()) {
      window.electronAPI!.windowControl('minimize');
    }
  },

  toggleMaximize: (): void => {
    if (isElectron()) {
      window.electronAPI!.windowControl('maximize');
    }
  },

  closeWindow: (): void => {
    if (isElectron()) {
      window.electronAPI!.windowControl('close');
    }
  },

  isMaximized: async (): Promise<boolean> => {
    if (isElectron()) {
      return window.electronAPI!.isMaximized();
    }
    return false;
  },

  // App info
  getVersion: async (): Promise<string> => {
    if (isElectron()) {
      return window.electronAPI!.getAppVersion();
    }
    return '0.1.0-dev';
  },

  getPlatform: (): string => {
    if (isElectron()) {
      return window.electronAPI!.platform;
    }
    return 'web';
  },

  // File operations
  openFile: async (options?: {
    title?: string;
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string;
  }): Promise<string | null> => {
    if (isElectron()) {
      return window.electronAPI!.showOpenDialog(options);
    }
    // Fallback for web: use file input
    return new Promise((resolve) => {
      const input = document.createElement('input');
      input.type = 'file';
      if (options?.filters) {
        input.accept = options.filters
          .flatMap(f => f.extensions.map(ext => `.${ext}`))
          .join(',');
      }
      input.onchange = () => {
        const file = input.files?.[0];
        resolve(file ? file.name : null);
      };
      input.click();
    });
  },

  saveFile: async (options?: {
    title?: string;
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string;
  }): Promise<string | null> => {
    if (isElectron()) {
      return window.electronAPI!.showSaveDialog(options);
    }
    // Web fallback: prompt for filename
    const filename = prompt('Enter filename:', options?.defaultPath || 'export.json');
    return filename;
  },

  readFile: async (filePath: string): Promise<string> => {
    if (isElectron()) {
      return window.electronAPI!.readFile(filePath);
    }
    throw new Error('File reading not supported in browser mode');
  },

  writeFile: async (filePath: string, content: string): Promise<void> => {
    if (isElectron()) {
      return window.electronAPI!.writeFile(filePath, content);
    }
    // Web fallback: trigger download
    const blob = new Blob([content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filePath.split('/').pop() || 'file.txt';
    a.click();
    URL.revokeObjectURL(url);
  },

  // Notifications
  showNotification: (options: { title: string; body: string }): void => {
    if ('Notification' in window && Notification.permission === 'granted') {
      new Notification(options.title, { body: options.body });
    }
  },

  // External links
  openExternal: async (url: string): Promise<void> => {
    if (isElectron()) {
      return window.electronAPI!.openExternal(url);
    }
    window.open(url, '_blank');
  },
};
