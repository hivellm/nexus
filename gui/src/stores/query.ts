import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { QueryResult, QueryHistory } from '@/types';
import { useServersStore } from './servers';

const HISTORY_KEY = 'nexus-desktop-query-history';
const MAX_HISTORY = 100;

export const useQueryStore = defineStore('query', () => {
  const currentQuery = ref('MATCH (n) RETURN n LIMIT 25');
  const lastResult = ref<QueryResult | null>(null);
  const isExecuting = ref(false);
  const error = ref<string | null>(null);
  const history = ref<QueryHistory[]>([]);

  // Load history from storage
  function loadHistory(): void {
    try {
      const stored = localStorage.getItem(HISTORY_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        history.value = parsed.map((h: any) => ({
          ...h,
          timestamp: new Date(h.timestamp),
        }));
      }
    } catch (e) {
      console.error('Failed to load query history:', e);
    }
  }

  function saveHistory(): void {
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(history.value.slice(0, MAX_HISTORY)));
    } catch (e) {
      console.error('Failed to save query history:', e);
    }
  }

  loadHistory();

  const recentQueries = computed(() => history.value.slice(0, 10));

  async function executeQuery(query?: string): Promise<QueryResult | null> {
    const serversStore = useServersStore();
    const client = serversStore.activeClient;

    if (!client) {
      error.value = 'No server connected';
      return null;
    }

    const queryToExecute = query || currentQuery.value;
    if (!queryToExecute.trim()) {
      error.value = 'Query cannot be empty';
      return null;
    }

    isExecuting.value = true;
    error.value = null;

    try {
      const response = await client.executeCypher(queryToExecute);

      const historyEntry: QueryHistory = {
        id: `query-${Date.now()}`,
        query: queryToExecute,
        timestamp: new Date(),
        executionTime: response.data?.executionTime || 0,
        rowCount: response.data?.rowCount || 0,
        success: response.success,
        error: response.error,
      };

      history.value.unshift(historyEntry);
      if (history.value.length > MAX_HISTORY) {
        history.value = history.value.slice(0, MAX_HISTORY);
      }
      saveHistory();

      if (response.success && response.data) {
        lastResult.value = response.data;
        return response.data;
      } else {
        error.value = response.error || 'Query execution failed';
        return null;
      }
    } catch (e: any) {
      error.value = e.message || 'Query execution failed';
      return null;
    } finally {
      isExecuting.value = false;
    }
  }

  function setQuery(query: string): void {
    currentQuery.value = query;
  }

  function clearResult(): void {
    lastResult.value = null;
    error.value = null;
  }

  function clearHistory(): void {
    history.value = [];
    saveHistory();
  }

  function removeFromHistory(id: string): void {
    history.value = history.value.filter((h) => h.id !== id);
    saveHistory();
  }

  return {
    currentQuery,
    lastResult,
    isExecuting,
    error,
    history,
    recentQueries,
    executeQuery,
    setQuery,
    clearResult,
    clearHistory,
    removeFromHistory,
  };
});
