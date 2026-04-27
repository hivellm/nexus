/**
 * Local query-history ring kept on the client. The server's
 * `/audit/log` carries server-side events; this store carries the
 * specific Cypher queries this user ran from this GUI, with their
 * latency + row count, so the Audit panel's "Query History"
 * section renders something useful even when the server's audit
 * stream is empty.
 *
 * Capped at `MAX_HISTORY` entries to keep localStorage small.
 */
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface QueryHistoryEntry {
  id: string;
  ts: string;
  query: string;
  ms: number;
  rows: number;
  ok: boolean;
}

const MAX_HISTORY = 200;

interface QueryHistoryState {
  entries: QueryHistoryEntry[];
  push: (entry: Omit<QueryHistoryEntry, 'id' | 'ts'>) => void;
  clear: () => void;
}

export const useQueryHistoryStore = create<QueryHistoryState>()(
  persist(
    (set) => ({
      entries: [],
      push: (entry) =>
        set((s) => {
          const next: QueryHistoryEntry = {
            ...entry,
            id: `qh-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`,
            ts: new Date().toISOString(),
          };
          const merged = [next, ...s.entries].slice(0, MAX_HISTORY);
          return { entries: merged };
        }),
      clear: () => set({ entries: [] }),
    }),
    { name: 'nexus_query_history' },
  ),
);
