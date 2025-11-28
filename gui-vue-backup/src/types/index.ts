// Server configuration
export interface ServerConfig {
  name: string;
  url?: string;
  host?: string;
  port?: number;
  ssl?: boolean;
  apiKey?: string;
  timeout?: number;
}

export interface Server extends ServerConfig {
  id: string;
  connected: boolean;
  lastConnected?: Date;
  status?: 'online' | 'offline' | 'connecting' | 'error' | 'connected' | 'disconnected';
  error?: string;
}

export type Theme = 'dark' | 'light';

// Declare Electron API on window
declare global {
  interface Window {
    electronAPI?: {
      windowControl: (action: string) => void;
      isMaximized: () => Promise<boolean>;
      onMaximizeChange: (callback: (isMaximized: boolean) => void) => void;
      showOpenDialog: (options: any) => Promise<string | null>;
      showSaveDialog: (options: any) => Promise<string | null>;
      readFile: (path: string) => Promise<string>;
      writeFile: (path: string, content: string) => Promise<void>;
      openExternal: (url: string) => Promise<void>;
      getAppVersion: () => Promise<string>;
      platform: string;
    };
  }
}

// Query types
export interface QueryResult {
  columns: string[];
  rows: Record<string, any>[];
  executionTime: number;
  rowCount: number;
}

export interface QueryHistory {
  id: string;
  query: string;
  timestamp: Date;
  executionTime: number;
  rowCount: number;
  success: boolean;
  error?: string;
}

// Graph types
export interface GraphNode {
  id: string | number;
  labels: string[];
  properties: Record<string, any>;
}

export interface GraphRelationship {
  id: string | number;
  type: string;
  startNode: string | number;
  endNode: string | number;
  properties: Record<string, any>;
}

export interface GraphData {
  nodes: GraphNode[];
  relationships: GraphRelationship[];
}

// Schema types
export interface LabelInfo {
  name: string;
  count: number;
  properties: PropertyInfo[];
}

export interface RelationshipTypeInfo {
  name: string;
  count: number;
  properties: PropertyInfo[];
}

export interface PropertyInfo {
  name: string;
  type: string;
  indexed: boolean;
}

export interface IndexInfo {
  name: string;
  label: string;
  properties: string[];
  type: 'btree' | 'fulltext' | 'vector';
  state: 'online' | 'populating' | 'failed';
}

// Stats types
export interface DatabaseStats {
  nodeCount: number;
  relationshipCount: number;
  labelCount: number;
  relationshipTypeCount: number;
  propertyKeyCount: number;
  indexCount: number;
  storageSize: number;
  uptime: number;
}

export interface ServerHealth {
  status: 'healthy' | 'unhealthy' | 'degraded';
  version: string;
  uptime: number;
  memory: {
    used: number;
    total: number;
    percentage: number;
  };
  storage: {
    used: number;
    total: number;
    percentage: number;
  };
}

// API response types
export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// Notification types
export interface Notification {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  message: string;
  timestamp: Date;
  read: boolean;
}
