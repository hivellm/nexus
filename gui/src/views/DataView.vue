<template>
  <div class="p-6 space-y-6">
    <!-- Search/Filter Bar -->
    <div class="card">
      <div class="flex items-center gap-4">
        <div class="flex-1">
          <input
            v-model="searchQuery"
            type="text"
            class="input"
            placeholder="Search nodes by property value..."
            @keydown.enter="search"
          />
        </div>
        <select v-model="selectedLabel" class="input w-48">
          <option value="">All Labels</option>
          <option v-for="label in labels" :key="label" :value="label">{{ label }}</option>
        </select>
        <button @click="search" class="btn btn-primary">
          <i class="fas fa-search mr-1"></i>
          Search
        </button>
      </div>
    </div>

    <!-- Results Table -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold">
          {{ selectedLabel ? `Nodes: ${selectedLabel}` : 'All Nodes' }}
          <span v-if="totalCount > 0" class="text-sm font-normal text-text-muted ml-2">
            ({{ totalCount }} total)
          </span>
        </h3>
        <div class="flex items-center gap-2">
          <button @click="refresh" class="btn btn-secondary text-xs">
            <i :class="['fas mr-1', isLoading ? 'fa-spinner fa-spin' : 'fa-sync']"></i>
            Refresh
          </button>
        </div>
      </div>

      <div v-if="nodes.length > 0" class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Labels</th>
              <th v-for="prop in visibleProperties" :key="prop">{{ prop }}</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="node in nodes" :key="node.id">
              <td class="font-mono text-xs">{{ node.id }}</td>
              <td>
                <div class="flex flex-wrap gap-1">
                  <span
                    v-for="label in node.labels"
                    :key="label"
                    class="px-2 py-0.5 bg-info/20 text-info rounded text-xs"
                  >
                    {{ label }}
                  </span>
                </div>
              </td>
              <td v-for="prop in visibleProperties" :key="prop">
                <span class="text-sm">{{ formatValue(node.properties[prop]) }}</span>
              </td>
              <td>
                <div class="flex items-center gap-1">
                  <button
                    @click="viewNode(node)"
                    class="p-1 text-text-muted hover:text-accent"
                    title="View details"
                  >
                    <i class="fas fa-eye"></i>
                  </button>
                  <button
                    @click="queryRelationships(node)"
                    class="p-1 text-text-muted hover:text-success"
                    title="View relationships"
                  >
                    <i class="fas fa-project-diagram"></i>
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-else-if="!isLoading" class="text-center text-text-muted py-8">
        <i class="fas fa-database text-2xl mb-2"></i>
        <p>No nodes found</p>
      </div>

      <!-- Pagination -->
      <div v-if="totalCount > pageSize" class="flex items-center justify-between mt-4 pt-4 border-t border-border">
        <div class="text-sm text-text-muted">
          Showing {{ (currentPage - 1) * pageSize + 1 }} - {{ Math.min(currentPage * pageSize, totalCount) }} of {{ totalCount }}
        </div>
        <div class="flex items-center gap-2">
          <button
            @click="currentPage--"
            :disabled="currentPage === 1"
            class="btn btn-secondary text-xs"
          >
            <i class="fas fa-chevron-left"></i>
          </button>
          <span class="text-sm">Page {{ currentPage }} of {{ totalPages }}</span>
          <button
            @click="currentPage++"
            :disabled="currentPage === totalPages"
            class="btn btn-secondary text-xs"
          >
            <i class="fas fa-chevron-right"></i>
          </button>
        </div>
      </div>
    </div>

    <!-- Node Detail Modal -->
    <div v-if="selectedNode" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="selectedNode = null">
      <div class="bg-bg-elevated rounded-lg p-6 w-full max-w-2xl max-h-[80vh] overflow-y-auto">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">Node Details</h3>
          <button @click="selectedNode = null" class="text-text-muted hover:text-text-primary">
            <i class="fas fa-times"></i>
          </button>
        </div>

        <div class="space-y-4">
          <div class="grid grid-cols-2 gap-4">
            <div>
              <div class="text-sm text-text-muted mb-1">ID</div>
              <div class="font-mono">{{ selectedNode.id }}</div>
            </div>
            <div>
              <div class="text-sm text-text-muted mb-1">Labels</div>
              <div class="flex flex-wrap gap-1">
                <span
                  v-for="label in selectedNode.labels"
                  :key="label"
                  class="px-2 py-0.5 bg-info/20 text-info rounded text-xs"
                >
                  {{ label }}
                </span>
              </div>
            </div>
          </div>

          <div>
            <div class="text-sm text-text-muted mb-2">Properties</div>
            <pre class="p-4 bg-bg-tertiary rounded-lg font-mono text-sm overflow-x-auto">{{ JSON.stringify(selectedNode.properties, null, 2) }}</pre>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useQueryStore } from '@/stores/query';
import type { GraphNode } from '@/types';

const router = useRouter();
const serversStore = useServersStore();
const queryStore = useQueryStore();

const nodes = ref<GraphNode[]>([]);
const labels = ref<string[]>([]);
const selectedLabel = ref('');
const searchQuery = ref('');
const isLoading = ref(false);
const currentPage = ref(1);
const pageSize = ref(25);
const totalCount = ref(0);
const selectedNode = ref<GraphNode | null>(null);

const totalPages = computed(() => Math.ceil(totalCount.value / pageSize.value));

const visibleProperties = computed(() => {
  if (nodes.value.length === 0) return [];
  const allProps = new Set<string>();
  nodes.value.forEach(node => {
    Object.keys(node.properties).forEach(key => allProps.add(key));
  });
  return Array.from(allProps).slice(0, 5);
});

async function loadLabels(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  const response = await client.getLabels();
  if (response.success && response.data) {
    labels.value = response.data.map(l => l.name);
  }
}

async function refresh(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  isLoading.value = true;

  try {
    let query = 'MATCH (n';
    if (selectedLabel.value) {
      query += `:${selectedLabel.value}`;
    }
    query += ')';

    if (searchQuery.value) {
      const searchLower = searchQuery.value.toLowerCase();
      query += ` WHERE any(prop IN keys(n) WHERE toLower(toString(n[prop])) CONTAINS '${searchLower}')`;
    }

    query += ' RETURN n';
    query += ` SKIP ${(currentPage.value - 1) * pageSize.value}`;
    query += ` LIMIT ${pageSize.value}`;

    const response = await client.executeCypher(query);
    if (response.success && response.data) {
      nodes.value = response.data.rows.map(row => ({
        id: row.n._nexus_id || row.n.id,
        labels: row.n._nexus_labels || row.n.labels || [],
        properties: extractProperties(row.n),
      }));
    }

    // Get total count
    let countQuery = 'MATCH (n';
    if (selectedLabel.value) {
      countQuery += `:${selectedLabel.value}`;
    }
    countQuery += ') RETURN count(n) AS count';

    const countResponse = await client.executeCypher(countQuery);
    if (countResponse.success && countResponse.data && countResponse.data.rows.length > 0) {
      totalCount.value = countResponse.data.rows[0].count;
    }
  } catch (error) {
    console.error('Failed to load data:', error);
  } finally {
    isLoading.value = false;
  }
}

function extractProperties(node: any): Record<string, any> {
  const props: Record<string, any> = {};
  for (const [key, value] of Object.entries(node)) {
    if (!key.startsWith('_nexus_') && key !== 'labels' && key !== 'id') {
      props[key] = value;
    }
  }
  return props;
}

function search(): void {
  currentPage.value = 1;
  refresh();
}

function viewNode(node: GraphNode): void {
  selectedNode.value = node;
}

function queryRelationships(node: GraphNode): void {
  queryStore.setQuery(`MATCH (n)-[r]-(m) WHERE id(n) = ${node.id} RETURN n, r, m`);
  router.push('/graph');
}

function formatValue(value: any): string {
  if (value === null || value === undefined) return '-';
  if (typeof value === 'object') return JSON.stringify(value);
  const str = String(value);
  return str.length > 50 ? str.substring(0, 50) + '...' : str;
}

watch([selectedLabel, currentPage], () => {
  refresh();
});

onMounted(() => {
  loadLabels();
  refresh();
});
</script>
