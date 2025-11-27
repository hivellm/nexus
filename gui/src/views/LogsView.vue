<template>
  <div class="p-6 space-y-6">
    <!-- Filters -->
    <div class="card">
      <div class="flex items-center gap-4">
        <div class="flex-1">
          <input
            v-model="searchQuery"
            type="text"
            class="input"
            placeholder="Search logs..."
            @keydown.enter="refresh"
          />
        </div>
        <select v-model="levelFilter" class="input w-32">
          <option value="">All Levels</option>
          <option value="error">Error</option>
          <option value="warn">Warning</option>
          <option value="info">Info</option>
          <option value="debug">Debug</option>
        </select>
        <button @click="refresh" class="btn btn-primary">
          <i class="fas fa-search mr-1"></i>
          Search
        </button>
        <button @click="clearLogs" class="btn btn-secondary">
          <i class="fas fa-trash mr-1"></i>
          Clear
        </button>
      </div>
    </div>

    <!-- Log Stream -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <i class="fas fa-stream text-info"></i>
          Server Logs
          <span v-if="logs.length > 0" class="text-sm font-normal text-text-muted">
            ({{ logs.length }} entries)
          </span>
        </h3>
        <div class="flex items-center gap-2">
          <label class="flex items-center gap-2 text-sm text-text-secondary">
            <input
              v-model="autoScroll"
              type="checkbox"
              class="rounded bg-bg-tertiary border-border text-accent"
            />
            Auto-scroll
          </label>
          <button @click="refresh" class="btn btn-secondary text-xs">
            <i :class="['fas mr-1', isLoading ? 'fa-spinner fa-spin' : 'fa-sync']"></i>
            Refresh
          </button>
        </div>
      </div>

      <div
        ref="logContainer"
        class="bg-bg-tertiary rounded-lg p-4 font-mono text-sm h-[calc(100vh-300px)] overflow-y-auto"
      >
        <div v-if="logs.length > 0" class="space-y-1">
          <div
            v-for="(log, index) in filteredLogs"
            :key="index"
            :class="['py-1 px-2 rounded', getLogClasses(log.level)]"
          >
            <span class="text-text-muted mr-2">{{ formatTimestamp(log.timestamp) }}</span>
            <span :class="getLevelClasses(log.level)" class="mr-2 uppercase font-semibold text-xs">
              [{{ log.level }}]
            </span>
            <span class="text-text-primary">{{ log.message }}</span>
            <span v-if="log.source" class="text-text-muted ml-2">- {{ log.source }}</span>
          </div>
        </div>
        <div v-else class="text-center text-text-muted py-8">
          <i class="fas fa-scroll text-2xl mb-2"></i>
          <p>No logs available</p>
        </div>
      </div>
    </div>

    <!-- Query History -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-history text-success"></i>
        Query History
      </h3>

      <div v-if="queryHistory.length > 0" class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr>
              <th>Timestamp</th>
              <th>Query</th>
              <th>Duration</th>
              <th>Rows</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(query, index) in queryHistory" :key="index">
              <td class="text-text-muted text-xs">{{ formatTimestamp(query.timestamp) }}</td>
              <td class="font-mono text-xs max-w-md truncate">{{ query.query }}</td>
              <td class="text-sm">{{ query.duration }}ms</td>
              <td class="text-sm">{{ query.rowCount }}</td>
              <td>
                <span :class="['px-2 py-0.5 rounded text-xs', query.success ? 'bg-success/20 text-success' : 'bg-error/20 text-error']">
                  {{ query.success ? 'OK' : 'Error' }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="text-center text-text-muted py-8">
        <i class="fas fa-history text-2xl mb-2"></i>
        <p>No query history</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue';
import { useServersStore } from '@/stores/servers';

interface LogEntry {
  timestamp: Date;
  level: 'error' | 'warn' | 'info' | 'debug';
  message: string;
  source?: string;
}

interface QueryHistoryEntry {
  timestamp: Date;
  query: string;
  duration: number;
  rowCount: number;
  success: boolean;
}

const serversStore = useServersStore();

const logs = ref<LogEntry[]>([]);
const queryHistory = ref<QueryHistoryEntry[]>([]);
const searchQuery = ref('');
const levelFilter = ref('');
const isLoading = ref(false);
const autoScroll = ref(true);
const logContainer = ref<HTMLElement | null>(null);

let refreshInterval: ReturnType<typeof setInterval> | null = null;

const filteredLogs = computed(() => {
  return logs.value.filter(log => {
    if (levelFilter.value && log.level !== levelFilter.value) return false;
    if (searchQuery.value && !log.message.toLowerCase().includes(searchQuery.value.toLowerCase())) return false;
    return true;
  });
});

async function refresh(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  isLoading.value = true;

  try {
    // Get logs from server
    const response = await client.getLogs?.();
    if (response?.success && response.data) {
      logs.value = response.data.map((log: any) => ({
        timestamp: new Date(log.timestamp),
        level: log.level || 'info',
        message: log.message,
        source: log.source,
      }));
    }

    // Get query history
    const historyResponse = await client.getQueryHistory?.();
    if (historyResponse?.success && historyResponse.data) {
      queryHistory.value = historyResponse.data.map((entry: any) => ({
        timestamp: new Date(entry.timestamp),
        query: entry.query,
        duration: entry.duration,
        rowCount: entry.rowCount,
        success: entry.success,
      }));
    }
  } catch (error) {
    console.error('Failed to load logs:', error);
    // Generate some mock logs for demo
    generateMockLogs();
  } finally {
    isLoading.value = false;
  }
}

function generateMockLogs(): void {
  logs.value = [
    { timestamp: new Date(), level: 'info', message: 'Server started on port 7687', source: 'main' },
    { timestamp: new Date(Date.now() - 60000), level: 'info', message: 'Connection accepted from 127.0.0.1', source: 'network' },
    { timestamp: new Date(Date.now() - 120000), level: 'debug', message: 'Query executed: MATCH (n) RETURN n LIMIT 10', source: 'query' },
    { timestamp: new Date(Date.now() - 180000), level: 'info', message: 'Index created on :Person(name)', source: 'storage' },
    { timestamp: new Date(Date.now() - 240000), level: 'warn', message: 'Memory usage above 80%', source: 'monitor' },
  ];
}

function clearLogs(): void {
  logs.value = [];
  queryHistory.value = [];
}

function formatTimestamp(date: Date): string {
  return new Date(date).toLocaleTimeString('en-US', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

function getLogClasses(level: string): string {
  switch (level) {
    case 'error':
      return 'bg-error/5';
    case 'warn':
      return 'bg-warning/5';
    default:
      return '';
  }
}

function getLevelClasses(level: string): string {
  switch (level) {
    case 'error':
      return 'text-error';
    case 'warn':
      return 'text-warning';
    case 'info':
      return 'text-info';
    case 'debug':
      return 'text-text-muted';
    default:
      return 'text-text-secondary';
  }
}

watch(filteredLogs, async () => {
  if (autoScroll.value && logContainer.value) {
    await nextTick();
    logContainer.value.scrollTop = logContainer.value.scrollHeight;
  }
});

onMounted(() => {
  refresh();
  refreshInterval = setInterval(refresh, 5000);
});

onUnmounted(() => {
  if (refreshInterval) {
    clearInterval(refreshInterval);
  }
});
</script>
