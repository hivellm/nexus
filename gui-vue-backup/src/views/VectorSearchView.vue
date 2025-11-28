<template>
  <div class="h-full flex flex-col p-6">
    <!-- Header -->
    <div class="mb-6">
      <h1 class="text-2xl font-bold text-text-primary">Vector Search (KNN)</h1>
      <p class="text-text-secondary mt-1">Search for similar nodes using vector embeddings</p>
    </div>

    <!-- Search Form -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
      <!-- Embedding Input -->
      <div class="lg:col-span-2 card p-4">
        <h3 class="text-sm font-medium text-text-secondary mb-3">Embedding Vector</h3>
        <div class="space-y-3">
          <div class="flex items-center gap-2">
            <select
              v-model="inputMode"
              class="px-3 py-2 bg-bg-tertiary border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-accent"
            >
              <option value="text">Text Input</option>
              <option value="json">JSON Array</option>
              <option value="file">Load from File</option>
            </select>
            <span class="text-xs text-text-muted">
              {{ inputMode === 'text' ? 'Enter comma-separated values' : inputMode === 'json' ? 'Enter JSON array' : 'Load embedding from file' }}
            </span>
          </div>

          <!-- Text Input -->
          <textarea
            v-if="inputMode === 'text'"
            v-model="embeddingText"
            rows="4"
            class="w-full p-3 bg-bg-tertiary border border-border rounded-lg font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            placeholder="0.1, 0.2, 0.3, -0.4, 0.5..."
          ></textarea>

          <!-- JSON Input -->
          <textarea
            v-if="inputMode === 'json'"
            v-model="embeddingJson"
            rows="4"
            class="w-full p-3 bg-bg-tertiary border border-border rounded-lg font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
            placeholder="[0.1, 0.2, 0.3, -0.4, 0.5]"
          ></textarea>

          <!-- File Input -->
          <div v-if="inputMode === 'file'" class="space-y-2">
            <button
              @click="loadFromFile"
              class="btn btn-secondary"
            >
              <i class="fas fa-file-upload mr-2"></i>
              Load Embedding File
            </button>
            <p v-if="loadedFileName" class="text-sm text-text-muted">
              <i class="fas fa-file mr-1"></i> {{ loadedFileName }}
            </p>
          </div>

          <!-- Parsed Info -->
          <div v-if="parsedEmbedding.length > 0" class="text-xs text-text-muted">
            <i class="fas fa-check-circle text-success mr-1"></i>
            Vector dimensions: {{ parsedEmbedding.length }}
          </div>
          <div v-if="parseError" class="text-xs text-error">
            <i class="fas fa-exclamation-circle mr-1"></i>
            {{ parseError }}
          </div>
        </div>
      </div>

      <!-- Search Options -->
      <div class="card p-4">
        <h3 class="text-sm font-medium text-text-secondary mb-3">Search Options</h3>
        <div class="space-y-4">
          <div>
            <label class="block text-sm text-text-secondary mb-1">Number of Results (K)</label>
            <input
              v-model.number="kValue"
              type="number"
              min="1"
              max="100"
              class="w-full px-3 py-2 bg-bg-tertiary border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-accent"
            />
          </div>

          <div>
            <label class="block text-sm text-text-secondary mb-1">Filter by Label (Optional)</label>
            <select
              v-model="selectedLabel"
              class="w-full px-3 py-2 bg-bg-tertiary border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-accent"
            >
              <option value="">All Labels</option>
              <option v-for="label in availableLabels" :key="label" :value="label">
                {{ label }}
              </option>
            </select>
          </div>

          <button
            @click="executeSearch"
            class="w-full btn btn-primary"
            :disabled="isSearching || parsedEmbedding.length === 0"
          >
            <i :class="['fas mr-2', isSearching ? 'fa-spinner fa-spin' : 'fa-search']"></i>
            {{ isSearching ? 'Searching...' : 'Search Similar Nodes' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Results -->
    <div class="flex-1 overflow-hidden">
      <!-- Error -->
      <div v-if="error" class="mb-4 p-4 bg-error/10 border border-error/20 rounded-lg">
        <div class="flex items-center gap-2 text-error">
          <i class="fas fa-exclamation-circle"></i>
          <span class="font-medium">Search Error</span>
        </div>
        <p class="mt-1 text-sm text-text-secondary">{{ error }}</p>
      </div>

      <!-- Results Table -->
      <div v-if="results.length > 0" class="card h-full flex flex-col">
        <div class="p-4 border-b border-border flex items-center justify-between">
          <div class="flex items-center gap-4">
            <h3 class="font-semibold">Search Results</h3>
            <span class="text-sm text-text-muted">{{ results.length }} similar nodes found</span>
          </div>
          <button @click="exportResults" class="btn btn-secondary text-xs">
            <i class="fas fa-download mr-1"></i>
            Export
          </button>
        </div>
        <div class="flex-1 overflow-auto p-4">
          <table class="table">
            <thead>
              <tr>
                <th>Rank</th>
                <th>Node ID</th>
                <th>Labels</th>
                <th>Similarity</th>
                <th>Properties</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(result, index) in results" :key="result.id">
                <td class="text-center font-mono">{{ index + 1 }}</td>
                <td class="font-mono text-sm">{{ result.id }}</td>
                <td>
                  <span
                    v-for="label in result.labels"
                    :key="label"
                    class="inline-block px-2 py-0.5 bg-accent/20 text-accent rounded text-xs mr-1"
                  >
                    {{ label }}
                  </span>
                </td>
                <td>
                  <div class="flex items-center gap-2">
                    <div class="w-20 h-2 bg-bg-tertiary rounded-full overflow-hidden">
                      <div
                        class="h-full bg-accent rounded-full"
                        :style="{ width: `${result.similarity * 100}%` }"
                      ></div>
                    </div>
                    <span class="text-sm font-mono">{{ (result.similarity * 100).toFixed(1) }}%</span>
                  </div>
                </td>
                <td>
                  <button
                    @click="showProperties(result)"
                    class="text-accent hover:underline text-sm"
                  >
                    View Properties
                  </button>
                </td>
                <td>
                  <button
                    @click="viewInGraph(result)"
                    class="text-text-muted hover:text-accent text-sm"
                    title="View in Graph"
                  >
                    <i class="fas fa-project-diagram"></i>
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Empty State -->
      <div v-if="!results.length && !error && !isSearching" class="h-full flex items-center justify-center">
        <div class="text-center text-text-muted">
          <i class="fas fa-vector-square text-4xl mb-4"></i>
          <p class="text-lg mb-2">Vector Similarity Search</p>
          <p class="text-sm">Enter an embedding vector to find similar nodes in the graph</p>
        </div>
      </div>
    </div>

    <!-- Properties Modal -->
    <div v-if="selectedNode" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div class="bg-bg-secondary rounded-lg p-6 w-full max-w-2xl mx-4 max-h-[80vh] overflow-auto">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">Node Properties</h3>
          <button @click="selectedNode = null" class="text-text-muted hover:text-text-primary">
            <i class="fas fa-times"></i>
          </button>
        </div>
        <div class="space-y-4">
          <div>
            <span class="text-sm text-text-secondary">ID:</span>
            <span class="ml-2 font-mono">{{ selectedNode.id }}</span>
          </div>
          <div>
            <span class="text-sm text-text-secondary">Labels:</span>
            <span
              v-for="label in selectedNode.labels"
              :key="label"
              class="ml-2 px-2 py-0.5 bg-accent/20 text-accent rounded text-sm"
            >
              {{ label }}
            </span>
          </div>
          <div>
            <span class="text-sm text-text-secondary">Similarity:</span>
            <span class="ml-2">{{ (selectedNode.similarity * 100).toFixed(2) }}%</span>
          </div>
          <div>
            <span class="text-sm text-text-secondary block mb-2">Properties:</span>
            <pre class="p-4 bg-bg-tertiary rounded-lg font-mono text-sm overflow-auto max-h-64">{{ JSON.stringify(selectedNode.properties, null, 2) }}</pre>
          </div>
        </div>
        <div class="flex justify-end mt-6">
          <button @click="selectedNode = null" class="btn btn-secondary">Close</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useNotificationsStore } from '@/stores/notifications';
import { ipcBridge } from '@/services/ipc';

interface SearchResult {
  id: string | number;
  labels: string[];
  similarity: number;
  properties: Record<string, any>;
}

const router = useRouter();
const serversStore = useServersStore();
const notifications = useNotificationsStore();

const inputMode = ref<'text' | 'json' | 'file'>('text');
const embeddingText = ref('');
const embeddingJson = ref('');
const loadedFileName = ref('');
const kValue = ref(10);
const selectedLabel = ref('');
const availableLabels = ref<string[]>([]);

const isSearching = ref(false);
const error = ref<string | null>(null);
const results = ref<SearchResult[]>([]);
const selectedNode = ref<SearchResult | null>(null);
const parseError = ref<string | null>(null);

const parsedEmbedding = computed((): number[] => {
  parseError.value = null;
  try {
    if (inputMode.value === 'text' && embeddingText.value.trim()) {
      const values = embeddingText.value.split(',').map(v => {
        const num = parseFloat(v.trim());
        if (isNaN(num)) throw new Error(`Invalid number: ${v}`);
        return num;
      });
      return values;
    } else if (inputMode.value === 'json' && embeddingJson.value.trim()) {
      const parsed = JSON.parse(embeddingJson.value);
      if (!Array.isArray(parsed)) throw new Error('Must be a JSON array');
      if (!parsed.every(v => typeof v === 'number')) throw new Error('All elements must be numbers');
      return parsed;
    }
    return [];
  } catch (e: any) {
    parseError.value = e.message;
    return [];
  }
});

async function loadLabels(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  try {
    const response = await client.getLabels();
    if (response.success && response.data) {
      availableLabels.value = response.data.map(l => l.name || l.label || String(l));
    }
  } catch (e) {
    console.error('Failed to load labels:', e);
  }
}

async function loadFromFile(): Promise<void> {
  try {
    const filePath = await ipcBridge.openFile({
      title: 'Load Embedding File',
      filters: [
        { name: 'JSON Files', extensions: ['json'] },
        { name: 'All Files', extensions: ['*'] },
      ],
    });

    if (filePath) {
      const content = await ipcBridge.readFile(filePath);
      const parsed = JSON.parse(content);

      if (Array.isArray(parsed)) {
        embeddingJson.value = JSON.stringify(parsed);
        inputMode.value = 'json';
      } else if (parsed.embedding && Array.isArray(parsed.embedding)) {
        embeddingJson.value = JSON.stringify(parsed.embedding);
        inputMode.value = 'json';
      } else {
        throw new Error('File must contain a JSON array or object with "embedding" field');
      }

      loadedFileName.value = filePath.split(/[\\/]/).pop() || filePath;
      notifications.success('File loaded', `Loaded ${parsedEmbedding.value.length} dimensions`);
    }
  } catch (e: any) {
    notifications.error('Load failed', e.message);
  }
}

async function executeSearch(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) {
    error.value = 'No server connected';
    return;
  }

  if (parsedEmbedding.value.length === 0) {
    error.value = 'Please enter a valid embedding vector';
    return;
  }

  isSearching.value = true;
  error.value = null;
  results.value = [];

  try {
    const response = await client.knnSearch(
      parsedEmbedding.value,
      kValue.value,
      selectedLabel.value || undefined
    );

    if (response.success && response.data) {
      // Transform results
      const rows = response.data.rows || response.data || [];
      results.value = rows.map((row: any) => ({
        id: row.id || row._nexus_id || row.nodeId,
        labels: row.labels || row._nexus_labels || [],
        similarity: row.similarity || row.score || row.distance || 0,
        properties: row.properties || row,
      }));

      notifications.success('Search complete', `Found ${results.value.length} similar nodes`);
    } else {
      error.value = response.error || 'Search failed';
    }
  } catch (e: any) {
    error.value = e.message || 'Search failed';
  } finally {
    isSearching.value = false;
  }
}

function showProperties(result: SearchResult): void {
  selectedNode.value = result;
}

function viewInGraph(result: SearchResult): void {
  // Navigate to graph view with query to show this node
  router.push({
    path: '/graph',
    query: { nodeId: String(result.id) },
  });
}

async function exportResults(): Promise<void> {
  if (results.value.length === 0) return;

  const content = JSON.stringify(results.value, null, 2);

  try {
    const filePath = await ipcBridge.saveFile({
      title: 'Export Search Results',
      defaultPath: 'knn-results.json',
      filters: [{ name: 'JSON Files', extensions: ['json'] }],
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
  loadLabels();
});
</script>
