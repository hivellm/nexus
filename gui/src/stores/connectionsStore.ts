/**
 * User-managed connection list. The titlebar's "host · graph"
 * breadcrumb reads `currentConnection`; the left panel's
 * `ConnectionsPanel` (item 5.2) writes to the list.
 *
 * Persisted to localStorage so the app reopens to the last-used
 * connection. The Electron-IPC bridge for shared connection
 * storage (item 5.3) replaces the persistence layer when it
 * lands; the store API stays.
 */
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type ConnectionRole = 'master' | 'replica' | 'standalone';
export type ConnectionStatus = 'connected' | 'idle' | 'error';

export interface Connection {
  id: string;
  name: string;
  url: string;
  role: ConnectionRole;
  status: ConnectionStatus;
}

interface ConnectionsState {
  connections: Connection[];
  currentConnectionId: string | null;
  setCurrent: (id: string) => void;
  upsert: (conn: Connection) => void;
  remove: (id: string) => void;
}

const DEFAULT_CONN: Connection = {
  id: 'local',
  name: 'localhost',
  url: 'http://localhost:15474',
  role: 'standalone',
  status: 'idle',
};

export const useConnectionsStore = create<ConnectionsState>()(
  persist(
    (set) => ({
      connections: [DEFAULT_CONN],
      currentConnectionId: DEFAULT_CONN.id,
      setCurrent: (id) => set({ currentConnectionId: id }),
      upsert: (conn) =>
        set((s) => ({
          connections: s.connections.some((c) => c.id === conn.id)
            ? s.connections.map((c) => (c.id === conn.id ? conn : c))
            : [...s.connections, conn],
        })),
      remove: (id) =>
        set((s) => ({
          connections: s.connections.filter((c) => c.id !== id),
          currentConnectionId:
            s.currentConnectionId === id
              ? (s.connections[0]?.id ?? null)
              : s.currentConnectionId,
        })),
    }),
    { name: 'nexus_connections' },
  ),
);

export function selectCurrentConnection(state: ConnectionsState): Connection | null {
  return state.connections.find((c) => c.id === state.currentConnectionId) ?? null;
}
