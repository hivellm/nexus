<template>
  <div class="flex flex-col h-full">
    <!-- Query Editor -->
    <div class="flex-shrink-0 border-b border-border">
      <div class="p-4">
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm font-medium text-text-secondary">Cypher Query</h3>
          <div class="flex items-center gap-2">
            <button
              @click="clearQuery"
              class="btn btn-secondary text-xs"
              :disabled="!currentQuery"
            >
              <i class="fas fa-eraser mr-1"></i>
              Clear
            </button>
            <button
              @click="executeQuery"
              class="btn btn-primary"
              :disabled="isExecuting || !currentQuery.trim()"
            >
              <i :class="['fas mr-1', isExecuting ? 'fa-spinner fa-spin' : 'fa-play']"></i>
              {{ isExecuting ? 'Executing...' : 'Run Query' }}
            </button>
          </div>
        </div>
        <div class="monaco-editor-container h-32">
          <textarea
            v-model="currentQuery"
            class="w-full h-full p-3 bg-bg-tertiary text-text-primary font-mono text-sm resize-none focus:outline-none"
            placeholder="Enter your Cypher query here...

Example: MATCH (n) RETURN n LIMIT 25"
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
      <div v-if="error" class="mb-4 p-4 bg-error/10 border border-error/20 rounded-lg">
        <div class="flex items-center gap-2 text-error">
          <i class="fas fa-exclamation-circle"></i>
          <span class="font-medium">Query Error</span>
        </div>
        <p class="mt-1 text-sm text-text-secondary">{{ error }}</p>
      </div>

      <!-- Results header -->
      <div v-if="lastResult" class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-2">
            <button
              @click="viewMode = 'table'"
              :class="['px-3 py-1 rounded text-sm', viewMode === 'table' ? 'bg-accent text-white' : 'bg-bg-tertiary text-text-secondary']"
            >
              <i class="fas fa-table mr-1"></i>
              Table
            </button>
            <button
              @click="viewMode = 'json'"
              :class="['px-3 py-1 rounded text-sm', viewMode === 'json' ? 'bg-accent text-white' : 'bg-bg-tertiary text-text-secondary']"
            >
              <i class="fas fa-code mr-1"></i>
              JSON
            </button>
          </div>
          <span class="text-sm text-text-muted">
            {{ lastResult.rowCount }} rows in {{ lastResult.executionTime }}ms
          </span>
        </div>
        <div class="flex items-center gap-2">
          <button @click="exportResults('json')" class="btn btn-secondary text-xs">
            <i class="fas fa-download mr-1"></i>
            Export JSON
          </button>
          <button @click="exportResults('csv')" class="btn btn-secondary text-xs">
            <i class="fas fa-file-csv mr-1"></i>
            Export CSV
          </button>
        </div>
      </div>

      <!-- Table view -->
      <div v-if="lastResult && viewMode === 'table'" class="flex-1 overflow-auto">
        <table class="table">
          <thead>
            <tr>
              <th v-for="column in lastResult.columns" :key="column">{{ column }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in lastResult.rows" :key="index">
              <td v-for="column in lastResult.columns" :key="column">
                <span v-if="isObject(row[column])" class="font-mono text-xs">
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
        <pre class="p-4 bg-bg-tertiary rounded-lg font-mono text-sm text-text-primary overflow-auto">{{ JSON.stringify(lastResult.rows, null, 2) }}</pre>
      </div>

      <!-- Empty state -->
      <div v-if="!lastResult && !error" class="flex-1 flex items-center justify-center">
        <div class="text-center text-text-muted">
          <i class="fas fa-terminal text-4xl mb-4"></i>
          <p>Enter a Cypher query and click Run to see results</p>
        </div>
      </div>
    </div>

    <!-- Query History Sidebar -->
    <div v-if="showHistory" class="fixed right-0 top-0 bottom-0 w-80 bg-bg-secondary border-l border-border p-4 z-50 shadow-lg">
      <div class="flex items-center justify-between mb-4">
        <h3 class="font-semibold">Query History</h3>
        <button @click="showHistory = false" class="text-text-muted hover:text-text-primary">
          <i class="fas fa-times"></i>
        </button>
      </div>
      <div class="space-y-2 overflow-y-auto h-full pb-20">
        <div
          v-for="item in history"
          :key="item.id"
          class="p-3 bg-bg-tertiary rounded-lg cursor-pointer hover:bg-bg-hover"
          @click="loadFromHistory(item.query)"
        >
          <div class="font-mono text-xs text-text-primary truncate">{{ item.query }}</div>
          <div class="flex items-center gap-2 mt-1 text-xs">
            <span :class="item.success ? 'text-success' : 'text-error'">
              <i :class="item.success ? 'fas fa-check' : 'fas fa-times'"></i>
            </span>
            <span class="text-text-muted">{{ item.rowCount }} rows</span>
            <span class="text-text-muted">{{ item.executionTime }}ms</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { useQueryStore } from '@/stores/query';
import { useNotificationsStore } from '@/stores/notifications';
import { ipcBridge } from '@/services/ipc';

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

function loadFromHistory(query: string): void {
  queryStore.setQuery(query);
  showHistory.value = false;
}

function isObject(value: any): boolean {
  return value !== null && typeof value === 'object';
}

async function exportResults(format: 'json' | 'csv'): Promise<void> {
  if (!lastResult.value) return;

  let content: string;
  let filename: string;

  if (format === 'json') {
    content = JSON.stringify(lastResult.value.rows, null, 2);
    filename = 'query-results.json';
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
  }

  try {
    const filePath = await ipcBridge.saveFile({
      title: 'Export Results',
      defaultPath: filename,
      filters: format === 'json'
        ? [{ name: 'JSON Files', extensions: ['json'] }]
        : [{ name: 'CSV Files', extensions: ['csv'] }],
    });

    if (filePath) {
      await ipcBridge.writeFile(filePath, content);
      notifications.success('Export complete', `Results saved to ${filePath}`);
    }
  } catch (e: any) {
    notifications.error('Export failed', e.message);
  }
}
</script>
