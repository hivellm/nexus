<template>
  <div class="flex flex-col h-full">
    <!-- Query Editor -->
    <div class="flex-shrink-0 border-b border-border bg-bg-secondary">
      <div class="p-4">
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm font-medium text-text-secondary">Cypher Query</h3>
          <div class="flex items-center gap-2">
            <button
              @click="showHistory = true"
              class="px-3 py-1.5 text-xs font-medium text-text-secondary bg-bg-tertiary border border-border rounded-lg hover:bg-bg-hover hover:text-text-primary transition-colors"
              title="Query History"
            >
              <i class="fas fa-history mr-1"></i>History
            </button>
            <button
              @click="clearQuery"
              class="px-3 py-1.5 text-xs font-medium text-text-secondary bg-bg-tertiary border border-border rounded-lg hover:bg-bg-hover hover:text-text-primary transition-colors disabled:opacity-50"
              :disabled="!currentQuery"
            >
              <i class="fas fa-trash mr-1"></i>Clear
            </button>
            <button
              @click="executeQuery"
              class="px-4 py-1.5 text-sm font-medium text-text-primary bg-bg-tertiary border border-border rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-50"
              :disabled="isExecuting || !currentQuery.trim()"
            >
              <i :class="['fas mr-1', isExecuting ? 'fa-spinner fa-spin' : 'fa-play']"></i>
              {{ isExecuting ? 'Executing...' : 'Run Query' }}
            </button>
          </div>
        </div>
        <div class="h-40 rounded-lg overflow-hidden border border-border bg-bg-tertiary">
          <textarea
            v-model="currentQuery"
            class="w-full h-full bg-transparent text-text-primary font-mono text-sm p-3 resize-none focus:outline-none placeholder-text-muted"
            placeholder="MATCH (n) RETURN n LIMIT 25"
            @keydown.ctrl.enter="executeQuery"
          ></textarea>
        </div>
        <div class="flex items-center justify-between mt-2 text-xs text-text-muted">
          <span>Press Ctrl+Enter to execute</span>
          <span v-if="lastExecutionTime">Last execution: {{ lastExecutionTime }}ms</span>
        </div>
      </div>
    </div>

    <!-- Results -->
    <div class="flex-1 flex flex-col min-h-0 p-4">
      <!-- Error message -->
      <div v-if="error" class="mb-4 p-4 bg-error/10 border border-error rounded-lg">
        <div class="flex items-center gap-2 text-error">
          <i class="fas fa-exclamation-circle"></i>
          <span class="font-medium">Query Error</span>
        </div>
        <p class="mt-1 text-sm text-error/80">{{ error }}</p>
      </div>

      <!-- Results header -->
      <div v-if="lastResult" class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-2">
            <button
              @click="viewMode = 'table'"
              :class="['px-3 py-1 rounded text-sm transition-colors', viewMode === 'table' ? 'bg-bg-tertiary text-text-primary border border-border' : 'bg-transparent text-text-secondary hover:text-text-primary']"
            >
              <i class="fas fa-table mr-1"></i>Table
            </button>
            <button
              @click="viewMode = 'json'"
              :class="['px-3 py-1 rounded text-sm transition-colors', viewMode === 'json' ? 'bg-bg-tertiary text-text-primary border border-border' : 'bg-transparent text-text-secondary hover:text-text-primary']"
            >
              <i class="fas fa-code mr-1"></i>JSON
            </button>
          </div>
          <span class="text-sm text-text-muted">
            {{ lastResult.rowCount }} rows in {{ lastResult.executionTime }}ms
          </span>
        </div>
        <div class="flex items-center gap-2">
          <button @click="exportResults('json')" class="px-3 py-1.5 text-xs font-medium text-text-secondary bg-bg-tertiary border border-border rounded-lg hover:bg-bg-hover hover:text-text-primary transition-colors">
            <i class="fas fa-download mr-1"></i>Export JSON
          </button>
          <button @click="exportResults('csv')" class="px-3 py-1.5 text-xs font-medium text-text-secondary bg-bg-tertiary border border-border rounded-lg hover:bg-bg-hover hover:text-text-primary transition-colors">
            <i class="fas fa-file-csv mr-1"></i>Export CSV
          </button>
        </div>
      </div>

      <!-- Table view -->
      <div v-if="lastResult && viewMode === 'table'" class="flex-1 overflow-auto bg-bg-secondary border border-border rounded-lg">
        <table class="min-w-full divide-y divide-border">
          <thead class="bg-bg-tertiary sticky top-0">
            <tr>
              <th v-for="column in lastResult.columns" :key="column" class="px-4 py-3 text-left text-xs font-medium text-text-muted uppercase tracking-wider">{{ column }}</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            <tr v-for="(row, index) in lastResult.rows" :key="index" class="hover:bg-bg-hover transition-colors">
              <td v-for="column in lastResult.columns" :key="column" class="px-4 py-3 text-sm text-text-primary">
                <span v-if="isObject(row[column])" class="font-mono text-xs text-text-secondary">
                  {{ JSON.stringify(row[column]).substring(0, 100) }}{{ JSON.stringify(row[column]).length > 100 ? '...' : '' }}
                </span>
                <span v-else>{{ row[column] }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- JSON view -->
      <div v-if="lastResult && viewMode === 'json'" class="flex-1 overflow-auto">
        <pre class="p-4 bg-bg-tertiary rounded-lg font-mono text-sm text-text-primary overflow-auto h-full border border-border">{{ JSON.stringify(lastResult.rows, null, 2) }}</pre>
      </div>

      <!-- Empty state -->
      <div v-if="!lastResult && !error" class="flex-1 flex items-center justify-center">
        <div class="text-center text-text-muted">
          <i class="fas fa-terminal text-4xl mb-4 block"></i>
          <p>Enter a Cypher query and click Run to see results</p>
        </div>
      </div>
    </div>

    <!-- Query History Sidebar -->
    <div v-if="showHistory" class="fixed right-0 top-0 bottom-0 w-96 bg-bg-secondary border-l border-border p-4 z-50 shadow-lg">
      <div class="flex items-center justify-between mb-4">
        <h3 class="font-semibold text-text-primary">Query History</h3>
        <div class="flex items-center gap-2">
          <button
            @click="clearHistory"
            class="text-xs text-text-muted hover:text-error"
            title="Clear History"
          >
            Clear
          </button>
          <button @click="showHistory = false" class="text-text-muted hover:text-text-primary">
            <i class="fas fa-times"></i>
          </button>
        </div>
      </div>
      <div class="space-y-2 overflow-y-auto h-full pb-20">
        <div
          v-for="item in history"
          :key="item.id"
          class="p-3 bg-bg-tertiary border border-border rounded-lg cursor-pointer hover:bg-bg-hover transition-colors group"
          @click="loadFromHistory(item.query)"
        >
          <div class="font-mono text-xs text-text-primary truncate">{{ item.query }}</div>
          <div class="flex items-center justify-between mt-1 text-xs">
            <div class="flex items-center gap-2 text-text-muted">
              <span :class="item.success ? 'text-success' : 'text-error'">
                <i :class="['fas', item.success ? 'fa-check' : 'fa-times']"></i>
              </span>
              <span>{{ item.rowCount }} rows</span>
              <span>{{ item.executionTime }}ms</span>
            </div>
          </div>
        </div>
        <div v-if="history.length === 0" class="text-center text-text-muted py-8">
          <i class="fas fa-history text-2xl mb-3 block"></i>
          <p>No query history yet</p>
        </div>
      </div>
    </div>

    <!-- Backdrop for sidebars -->
    <div
      v-if="showHistory"
      class="fixed inset-0 bg-black/20 z-40"
      @click="showHistory = false"
    ></div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { useQueryStore } from '@/stores/query';
import { useNotificationsStore } from '@/stores/notifications';

const queryStore = useQueryStore();
const notifications = useNotificationsStore();

const viewMode = ref<'table' | 'json'>('table');
const showHistory = ref(false);

const currentQuery = computed({
  get: () => queryStore.currentQuery,
  set: (value) => queryStore.setQuery(value),
});

const lastResult = computed(() => queryStore.lastResult);
const isExecuting = computed(() => queryStore.isExecuting);
const error = computed(() => queryStore.error);
const history = computed(() => queryStore.history);
const lastExecutionTime = computed(() => lastResult.value?.executionTime);

async function executeQuery(): Promise<void> {
  const result = await queryStore.executeQuery();
  if (result) {
    notifications.success('Query executed', `${result.rowCount} rows returned`);
  }
}

function clearQuery(): void {
  queryStore.setQuery('');
  queryStore.clearResult();
}

function clearHistory(): void {
  queryStore.clearHistory();
  notifications.info('History cleared', 'Query history has been cleared');
}

function loadFromHistory(query: string): void {
  queryStore.setQuery(query);
  showHistory.value = false;
}

function isObject(value: any): boolean {
  return value !== null && typeof value === 'object';
}

function exportResults(format: 'json' | 'csv'): void {
  if (!lastResult.value) return;

  let content: string;
  let filename: string;
  let mimeType: string;

  if (format === 'json') {
    content = JSON.stringify(lastResult.value.rows, null, 2);
    filename = 'query-results.json';
    mimeType = 'application/json';
  } else {
    // CSV export
    const headers = lastResult.value.columns.join(',');
    const rows = lastResult.value.rows.map((row) =>
      lastResult.value!.columns.map((col) => {
        const value = row[col];
        if (typeof value === 'string') {
          return `"${value.replace(/"/g, '""')}"`;
        }
        if (typeof value === 'object') {
          return `"${JSON.stringify(value).replace(/"/g, '""')}"`;
        }
        return value;
      }).join(',')
    );
    content = [headers, ...rows].join('\n');
    filename = 'query-results.csv';
    mimeType = 'text/csv';
  }

  // Create and trigger download
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);

  notifications.success('Export complete', `Results saved as ${filename}`);
}

onMounted(() => {
  // No additional setup needed
});
</script>
