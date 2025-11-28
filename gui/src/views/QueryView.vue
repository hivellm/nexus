<template>
  <div class="flex flex-col h-full">
    <!-- Query Editor -->
    <div class="flex-shrink-0 border-b border-neutral-200 dark:border-neutral-800">
      <div class="p-4">
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm font-medium text-neutral-600 dark:text-neutral-400">Cypher Query</h3>
          <div class="flex items-center gap-2">
            <button
              @click="showSavedQueries = true"
              class="px-3 py-1.5 text-xs font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors"
              title="Saved Queries"
            >
              Saved
            </button>
            <button
              @click="showHistory = true"
              class="px-3 py-1.5 text-xs font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors"
              title="Query History"
            >
              History
            </button>
            <button
              @click="saveCurrentQuery"
              class="px-3 py-1.5 text-xs font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors disabled:opacity-50"
              :disabled="!currentQuery.trim()"
              title="Save Current Query"
            >
              Save
            </button>
            <button
              @click="clearQuery"
              class="px-3 py-1.5 text-xs font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors disabled:opacity-50"
              :disabled="!currentQuery"
            >
              Clear
            </button>
            <button
              @click="executeQuery"
              class="px-4 py-1.5 text-sm font-medium text-white bg-neutral-900 dark:bg-neutral-100 dark:text-neutral-900 rounded-lg hover:bg-neutral-800 dark:hover:bg-neutral-200 transition-colors disabled:opacity-50"
              :disabled="isExecuting || !currentQuery.trim()"
            >
              {{ isExecuting ? 'Executing...' : 'Run Query' }}
            </button>
          </div>
        </div>
        <div class="h-40 rounded-lg overflow-hidden border border-neutral-200 dark:border-neutral-700">
          <MonacoEditor
            v-model="currentQuery"
            :theme="editorTheme"
            @execute="executeQuery"
          />
        </div>
        <div class="flex items-center justify-between mt-2 text-xs text-neutral-500 dark:text-neutral-400">
          <span>Press Ctrl+Enter to execute</span>
          <span v-if="lastExecutionTime">Last execution: {{ lastExecutionTime }}ms</span>
        </div>
      </div>
    </div>

    <!-- Results -->
    <div class="flex-1 flex flex-col min-h-0 p-4">
      <!-- Error message -->
      <div v-if="error" class="mb-4 p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
        <div class="flex items-center gap-2 text-red-800 dark:text-red-300">
          <span class="font-medium">Query Error</span>
        </div>
        <p class="mt-1 text-sm text-red-700 dark:text-red-400">{{ error }}</p>
      </div>

      <!-- Results header -->
      <div v-if="lastResult" class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-2">
            <button
              @click="viewMode = 'table'"
              :class="['px-3 py-1 rounded text-sm transition-colors', viewMode === 'table' ? 'bg-neutral-900 dark:bg-neutral-100 text-white dark:text-neutral-900' : 'bg-neutral-100 dark:bg-neutral-800 text-neutral-700 dark:text-neutral-300']"
            >
              Table
            </button>
            <button
              @click="viewMode = 'json'"
              :class="['px-3 py-1 rounded text-sm transition-colors', viewMode === 'json' ? 'bg-neutral-900 dark:bg-neutral-100 text-white dark:text-neutral-900' : 'bg-neutral-100 dark:bg-neutral-800 text-neutral-700 dark:text-neutral-300']"
            >
              JSON
            </button>
          </div>
          <span class="text-sm text-neutral-500 dark:text-neutral-400">
            {{ lastResult.rowCount }} rows in {{ lastResult.executionTime }}ms
          </span>
        </div>
        <div class="flex items-center gap-2">
          <button @click="exportResults('json')" class="px-3 py-1.5 text-xs font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors">
            Export JSON
          </button>
          <button @click="exportResults('csv')" class="px-3 py-1.5 text-xs font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors">
            Export CSV
          </button>
        </div>
      </div>

      <!-- Table view -->
      <div v-if="lastResult && viewMode === 'table'" class="flex-1 overflow-auto">
        <table class="min-w-full divide-y divide-neutral-200 dark:divide-neutral-700">
          <thead class="bg-neutral-50 dark:bg-neutral-800/50">
            <tr>
              <th v-for="column in lastResult.columns" :key="column" class="px-4 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">{{ column }}</th>
            </tr>
          </thead>
          <tbody class="bg-white dark:bg-neutral-900 divide-y divide-neutral-200 dark:divide-neutral-800/50">
            <tr v-for="(row, index) in lastResult.rows" :key="index" class="hover:bg-neutral-50 dark:hover:bg-neutral-800/50">
              <td v-for="column in lastResult.columns" :key="column" class="px-4 py-3 text-sm text-neutral-900 dark:text-neutral-100">
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
        <pre class="p-4 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg font-mono text-sm text-neutral-900 dark:text-neutral-100 overflow-auto">{{ JSON.stringify(lastResult.rows, null, 2) }}</pre>
      </div>

      <!-- Empty state -->
      <div v-if="!lastResult && !error" class="flex-1 flex items-center justify-center">
        <div class="text-center text-neutral-500 dark:text-neutral-400">
          <p>Enter a Cypher query and click Run to see results</p>
        </div>
      </div>
    </div>

    <!-- Query History Sidebar -->
    <div v-if="showHistory" class="fixed right-0 top-0 bottom-0 w-96 bg-white dark:bg-neutral-900 border-l border-neutral-200 dark:border-neutral-800 p-4 z-50 shadow-lg">
      <div class="flex items-center justify-between mb-4">
        <h3 class="font-semibold text-neutral-900 dark:text-white">Query History</h3>
        <div class="flex items-center gap-2">
          <button
            @click="clearHistory"
            class="text-xs text-neutral-500 hover:text-red-600 dark:hover:text-red-400"
            title="Clear History"
          >
            Clear
          </button>
          <button @click="showHistory = false" class="text-neutral-500 hover:text-neutral-900 dark:hover:text-white">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
      <div class="space-y-2 overflow-y-auto h-full pb-20">
        <div
          v-for="item in history"
          :key="item.id"
          class="p-3 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg cursor-pointer hover:bg-neutral-100 dark:hover:bg-neutral-800 group"
          @click="loadFromHistory(item.query)"
        >
          <div class="font-mono text-xs text-neutral-900 dark:text-white truncate">{{ item.query }}</div>
          <div class="flex items-center justify-between mt-1 text-xs">
            <div class="flex items-center gap-2 text-neutral-500 dark:text-neutral-400">
              <span :class="item.success ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'">
                {{ item.success ? '✓' : '✗' }}
              </span>
              <span>{{ item.rowCount }} rows</span>
              <span>{{ item.executionTime }}ms</span>
            </div>
            <button
              @click.stop="saveQueryFromHistory(item)"
              class="opacity-0 group-hover:opacity-100 text-neutral-500 hover:text-neutral-900 dark:hover:text-white"
              title="Save this query"
            >
              Save
            </button>
          </div>
        </div>
        <div v-if="history.length === 0" class="text-center text-neutral-500 dark:text-neutral-400 py-8">
          <p>No query history yet</p>
        </div>
      </div>
    </div>

    <!-- Saved Queries Sidebar -->
    <div v-if="showSavedQueries" class="fixed right-0 top-0 bottom-0 w-96 bg-white dark:bg-neutral-900 border-l border-neutral-200 dark:border-neutral-800 p-4 z-50 shadow-lg">
      <div class="flex items-center justify-between mb-4">
        <h3 class="font-semibold text-neutral-900 dark:text-white">Saved Queries</h3>
        <button @click="showSavedQueries = false" class="text-neutral-500 hover:text-neutral-900 dark:hover:text-white">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
      <div class="space-y-2 overflow-y-auto h-full pb-20">
        <div
          v-for="saved in savedQueries"
          :key="saved.id"
          class="p-3 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg group"
        >
          <div class="flex items-center justify-between mb-1">
            <span class="font-medium text-sm text-neutral-900 dark:text-white">{{ saved.name }}</span>
            <div class="flex items-center gap-1">
              <button
                @click="loadSavedQuery(saved)"
                class="text-xs text-neutral-500 hover:text-neutral-900 dark:hover:text-white px-2 py-1"
                title="Load query"
              >
                Load
              </button>
              <button
                @click="deleteSavedQuery(saved.id)"
                class="text-xs text-neutral-500 hover:text-red-600 dark:hover:text-red-400 px-2 py-1"
                title="Delete query"
              >
                Delete
              </button>
            </div>
          </div>
          <div class="font-mono text-xs text-neutral-600 dark:text-neutral-400 truncate">{{ saved.query }}</div>
          <div class="text-xs text-neutral-500 dark:text-neutral-400 mt-1">
            {{ formatDate(saved.createdAt) }}
          </div>
        </div>
        <div v-if="savedQueries.length === 0" class="text-center text-neutral-500 dark:text-neutral-400 py-8">
          <p>No saved queries yet</p>
          <p class="text-xs mt-1">Click "Save" to bookmark a query</p>
        </div>
      </div>
    </div>

    <!-- Save Query Modal -->
    <div v-if="showSaveModal" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div class="bg-white dark:bg-neutral-900 rounded-lg p-6 w-full max-w-md mx-4 border border-neutral-200 dark:border-neutral-800">
        <h3 class="text-lg font-semibold text-neutral-900 dark:text-white mb-4">Save Query</h3>
        <div class="space-y-4">
          <div>
            <label class="block text-sm text-neutral-600 dark:text-neutral-400 mb-1">Query Name</label>
            <input
              v-model="saveQueryName"
              type="text"
              class="w-full px-3 py-2 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg focus:outline-none focus:ring-2 focus:ring-neutral-500 text-neutral-900 dark:text-white"
              placeholder="e.g., Get all users"
              @keydown.enter="confirmSaveQuery"
            />
          </div>
          <div>
            <label class="block text-sm text-neutral-600 dark:text-neutral-400 mb-1">Query</label>
            <div class="font-mono text-xs text-neutral-600 dark:text-neutral-400 bg-neutral-50 dark:bg-neutral-800/50 p-3 rounded-lg max-h-32 overflow-auto">
              {{ queryToSave }}
            </div>
          </div>
        </div>
        <div class="flex justify-end gap-2 mt-6">
          <button @click="cancelSaveQuery" class="px-4 py-2 text-sm font-medium text-neutral-700 dark:text-neutral-300 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg hover:bg-neutral-50 dark:hover:bg-neutral-700 transition-colors">Cancel</button>
          <button
            @click="confirmSaveQuery"
            class="px-4 py-2 text-sm font-medium text-white bg-neutral-900 dark:bg-neutral-100 dark:text-neutral-900 rounded-lg hover:bg-neutral-800 dark:hover:bg-neutral-200 transition-colors disabled:opacity-50"
            :disabled="!saveQueryName.trim()"
          >
            Save
          </button>
        </div>
      </div>
    </div>

    <!-- Backdrop for sidebars -->
    <div
      v-if="showHistory || showSavedQueries"
      class="fixed inset-0 bg-black/20 z-40"
      @click="showHistory = false; showSavedQueries = false"
    ></div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { useQueryStore } from '@/stores/query';
import { useNotificationsStore } from '@/stores/notifications';
import { useThemeStore } from '@/stores/theme';
import { ipcBridge } from '@/services/ipc';
import MonacoEditor from '@/components/MonacoEditor.vue';

interface SavedQuery {
  id: string;
  name: string;
  query: string;
  createdAt: Date;
}

const SAVED_QUERIES_KEY = 'nexus-desktop-saved-queries';

const queryStore = useQueryStore();
const notifications = useNotificationsStore();
const themeStore = useThemeStore();

const viewMode = ref<'table' | 'json'>('table');
const showHistory = ref(false);
const showSavedQueries = ref(false);
const showSaveModal = ref(false);
const saveQueryName = ref('');
const queryToSave = ref('');
const savedQueries = ref<SavedQuery[]>([]);

const editorTheme = computed(() => themeStore.theme === 'dark' ? 'vs-dark' : 'vs');

const currentQuery = computed({
  get: () => queryStore.currentQuery,
  set: (value) => queryStore.setQuery(value),
});

const lastResult = computed(() => queryStore.lastResult);
const isExecuting = computed(() => queryStore.isExecuting);
const error = computed(() => queryStore.error);
const history = computed(() => queryStore.history);
const lastExecutionTime = computed(() => lastResult.value?.executionTime);

// Load saved queries from localStorage
function loadSavedQueries(): void {
  try {
    const stored = localStorage.getItem(SAVED_QUERIES_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      savedQueries.value = parsed.map((q: any) => ({
        ...q,
        createdAt: new Date(q.createdAt),
      }));
    }
  } catch (e) {
    console.error('Failed to load saved queries:', e);
  }
}

function saveSavedQueries(): void {
  try {
    localStorage.setItem(SAVED_QUERIES_KEY, JSON.stringify(savedQueries.value));
  } catch (e) {
    console.error('Failed to save queries:', e);
  }
}

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

function loadSavedQuery(saved: SavedQuery): void {
  queryStore.setQuery(saved.query);
  showSavedQueries.value = false;
  notifications.info('Query loaded', `Loaded "${saved.name}"`);
}

function saveCurrentQuery(): void {
  queryToSave.value = currentQuery.value;
  saveQueryName.value = '';
  showSaveModal.value = true;
}

function saveQueryFromHistory(item: { query: string }): void {
  queryToSave.value = item.query;
  saveQueryName.value = '';
  showSaveModal.value = true;
}

function confirmSaveQuery(): void {
  if (!saveQueryName.value.trim() || !queryToSave.value.trim()) return;

  const newSaved: SavedQuery = {
    id: `saved-${Date.now()}`,
    name: saveQueryName.value.trim(),
    query: queryToSave.value,
    createdAt: new Date(),
  };

  savedQueries.value.unshift(newSaved);
  saveSavedQueries();

  showSaveModal.value = false;
  notifications.success('Query saved', `Saved as "${newSaved.name}"`);
}

function cancelSaveQuery(): void {
  showSaveModal.value = false;
  saveQueryName.value = '';
  queryToSave.value = '';
}

function deleteSavedQuery(id: string): void {
  savedQueries.value = savedQueries.value.filter(q => q.id !== id);
  saveSavedQueries();
  notifications.info('Query deleted', 'Saved query has been removed');
}

function formatDate(date: Date): string {
  return new Intl.DateTimeFormat('en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
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

onMounted(() => {
  loadSavedQueries();
});
</script>
