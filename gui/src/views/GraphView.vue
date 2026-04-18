<template>
  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="flex-shrink-0 p-4 border-b border-border bg-bg-secondary">
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2 flex-1">
          <input
            v-model="query"
            type="text"
            class="flex-1 px-3 py-2 bg-bg-tertiary border border-border rounded-lg text-text-primary placeholder-text-muted focus:outline-none focus:border-border-light transition-colors text-sm font-mono"
            placeholder="MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 100"
            @keydown.ctrl.enter="executeQuery"
            @keydown.enter="executeQuery"
          />
          <button
            @click="executeQuery"
            class="px-4 py-2 text-sm font-medium text-text-primary bg-bg-tertiary border border-border rounded-lg hover:bg-bg-hover transition-colors disabled:opacity-50"
            :disabled="isLoading"
          >
            <i :class="['fas mr-1', isLoading ? 'fa-spinner fa-spin' : 'fa-play']"></i>
            Run
          </button>
        </div>
      </div>
    </div>

    <!-- Results Area -->
    <div v-if="hasResult" class="flex-1 flex flex-col min-h-0">
      <!-- View Tabs -->
      <div class="flex items-center justify-between px-4 py-2 border-b border-border bg-bg-secondary">
        <div class="flex items-center gap-1">
          <button
            @click="viewMode = 'graph'"
            :class="['px-3 py-1.5 text-sm font-medium rounded transition-colors', viewMode === 'graph' ? 'bg-bg-tertiary text-text-primary' : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover']"
          >
            <i class="fas fa-project-diagram mr-1"></i>Graph
          </button>
          <button
            @click="viewMode = 'table'"
            :class="['px-3 py-1.5 text-sm font-medium rounded transition-colors', viewMode === 'table' ? 'bg-bg-tertiary text-text-primary' : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover']"
          >
            <i class="fas fa-table mr-1"></i>Table
          </button>
          <button
            @click="viewMode = 'text'"
            :class="['px-3 py-1.5 text-sm font-medium rounded transition-colors', viewMode === 'text' ? 'bg-bg-tertiary text-text-primary' : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover']"
          >
            <i class="fas fa-align-left mr-1"></i>Text
          </button>
          <button
            @click="viewMode = 'code'"
            :class="['px-3 py-1.5 text-sm font-medium rounded transition-colors', viewMode === 'code' ? 'bg-bg-tertiary text-text-primary' : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover']"
          >
            <i class="fas fa-code mr-1"></i>Code
          </button>
        </div>
        <div class="flex items-center gap-4 text-sm text-text-muted">
          <span>{{ resultStats }}</span>
          <span>{{ executionTime }}ms</span>
        </div>
      </div>

      <!-- Graph View -->
      <div v-show="viewMode === 'graph'" class="flex-1 flex min-h-0">
        <div class="flex-1 relative bg-bg-primary">
          <div ref="graphContainer" class="absolute inset-0"></div>

          <!-- Graph Controls -->
          <div class="absolute top-4 right-4 flex flex-col gap-2">
            <button @click="fitGraph" class="p-2 bg-bg-elevated border border-border rounded-lg hover:bg-bg-hover transition-colors" title="Fit to screen">
              <i class="fas fa-expand text-text-secondary"></i>
            </button>
            <button @click="zoomIn" class="p-2 bg-bg-elevated border border-border rounded-lg hover:bg-bg-hover transition-colors" title="Zoom in">
              <i class="fas fa-plus text-text-secondary"></i>
            </button>
            <button @click="zoomOut" class="p-2 bg-bg-elevated border border-border rounded-lg hover:bg-bg-hover transition-colors" title="Zoom out">
              <i class="fas fa-minus text-text-secondary"></i>
            </button>
            <button @click="togglePhysics" class="p-2 bg-bg-elevated border border-border rounded-lg hover:bg-bg-hover transition-colors" :title="physicsEnabled ? 'Disable physics' : 'Enable physics'">
              <i :class="['fas', physicsEnabled ? 'fa-lock-open' : 'fa-lock', 'text-text-secondary']"></i>
            </button>
          </div>

          <!-- Stats overlay -->
          <div v-if="graphNodes.length > 0" class="absolute bottom-4 left-4 bg-bg-elevated border border-border rounded-lg px-3 py-2 text-xs">
            <span class="text-text-secondary">{{ graphNodes.length }} nodes, {{ graphEdges.length }} relationships</span>
          </div>
        </div>

        <!-- Properties Panel -->
        <div v-if="selectedNode || selectedEdge" class="w-72 border-l border-border bg-bg-secondary p-4 overflow-y-auto">
          <div class="flex items-center justify-between mb-3">
            <h3 class="font-semibold text-text-primary text-sm">
              {{ selectedNode ? 'Node' : 'Relationship' }}
            </h3>
            <button @click="clearSelection" class="text-text-muted hover:text-text-primary transition-colors">
              <i class="fas fa-times text-xs"></i>
            </button>
          </div>

          <div v-if="selectedNode" class="space-y-3">
            <div>
              <div class="text-xs text-text-muted mb-1">ID</div>
              <div class="font-mono text-sm text-text-primary">{{ selectedNode.id }}</div>
            </div>
            <div v-if="selectedNode.labels?.length">
              <div class="text-xs text-text-muted mb-1">Labels</div>
              <div class="flex flex-wrap gap-1">
                <span v-for="label in selectedNode.labels" :key="label" class="px-2 py-0.5 bg-info/20 text-info rounded text-xs">
                  :{{ label }}
                </span>
              </div>
            </div>
            <div v-if="Object.keys(selectedNode.properties || {}).length">
              <div class="text-xs text-text-muted mb-2">Properties</div>
              <div class="space-y-1">
                <div v-for="(value, key) in selectedNode.properties" :key="key" class="bg-bg-tertiary rounded p-2">
                  <div class="text-xs text-text-muted">{{ key }}</div>
                  <div class="text-sm font-mono text-text-primary break-all">{{ formatValue(value) }}</div>
                </div>
              </div>
            </div>
          </div>

          <div v-if="selectedEdge" class="space-y-3">
            <div>
              <div class="text-xs text-text-muted mb-1">Type</div>
              <span class="px-2 py-0.5 bg-success/20 text-success rounded text-xs">:{{ selectedEdge.type }}</span>
            </div>
            <div>
              <div class="text-xs text-text-muted mb-1">From → To</div>
              <div class="font-mono text-sm text-text-primary">{{ selectedEdge.from }} → {{ selectedEdge.to }}</div>
            </div>
            <div v-if="Object.keys(selectedEdge.properties || {}).length">
              <div class="text-xs text-text-muted mb-2">Properties</div>
              <div class="space-y-1">
                <div v-for="(value, key) in selectedEdge.properties" :key="key" class="bg-bg-tertiary rounded p-2">
                  <div class="text-xs text-text-muted">{{ key }}</div>
                  <div class="text-sm font-mono text-text-primary break-all">{{ formatValue(value) }}</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Table View -->
      <div v-show="viewMode === 'table'" class="flex-1 overflow-auto p-4">
        <div class="bg-bg-secondary border border-border rounded-lg overflow-hidden">
          <table class="min-w-full divide-y divide-border">
            <thead class="bg-bg-tertiary">
              <tr>
                <th v-for="column in rawResult?.columns" :key="column" class="px-4 py-3 text-left text-xs font-medium text-text-muted uppercase tracking-wider">
                  {{ column }}
                </th>
              </tr>
            </thead>
            <tbody class="divide-y divide-border">
              <tr v-for="(row, index) in rawResult?.rows" :key="index" class="hover:bg-bg-hover transition-colors">
                <td v-for="column in rawResult?.columns" :key="column" class="px-4 py-3 text-sm text-text-primary">
                  <span v-if="isObject(row[column])" class="font-mono text-xs">
                    <span v-if="row[column]?._nexus_id" class="text-info">
                      ({{ row[column]._nexus_id }})
                    </span>
                    {{ formatTableCell(row[column]) }}
                  </span>
                  <span v-else>{{ row[column] }}</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Text View -->
      <div v-show="viewMode === 'text'" class="flex-1 overflow-auto p-4">
        <div class="bg-bg-secondary border border-border rounded-lg p-4 font-mono text-sm">
          <div v-for="(row, index) in rawResult?.rows" :key="index" class="mb-4 pb-4 border-b border-border last:border-0 last:mb-0 last:pb-0">
            <div v-for="column in rawResult?.columns" :key="column" class="mb-2">
              <span class="text-text-muted">{{ column }}:</span>
              <span class="text-text-primary ml-2">{{ formatTextValue(row[column]) }}</span>
            </div>
          </div>
          <div v-if="!rawResult?.rows?.length" class="text-text-muted text-center py-8">
            No results
          </div>
        </div>
      </div>

      <!-- Code View (JSON) -->
      <div v-show="viewMode === 'code'" class="flex-1 overflow-auto p-4">
        <div class="bg-bg-secondary border border-border rounded-lg overflow-hidden">
          <div class="flex items-center justify-between px-4 py-2 border-b border-border bg-bg-tertiary">
            <span class="text-xs text-text-muted">JSON</span>
            <button @click="copyJson" class="text-xs text-text-secondary hover:text-text-primary transition-colors">
              <i class="fas fa-copy mr-1"></i>Copy
            </button>
          </div>
          <pre class="p-4 font-mono text-sm text-text-primary overflow-auto max-h-full">{{ formattedJson }}</pre>
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-else class="flex-1 flex items-center justify-center">
      <div class="text-center text-text-muted">
        <i class="fas fa-terminal text-4xl mb-4 block"></i>
        <p class="mb-2">Enter a Cypher query and press Run</p>
        <p class="text-sm">Example: MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 25</p>
      </div>
    </div>

    <!-- Error -->
    <div v-if="error" class="absolute bottom-4 left-4 right-4 p-4 bg-error/10 border border-error rounded-lg">
      <div class="flex items-center gap-2 text-error">
        <i class="fas fa-exclamation-circle"></i>
        <span class="font-medium">Error</span>
      </div>
      <p class="mt-1 text-sm text-error/80">{{ error }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { Network, DataSet } from 'vis-network/standalone';
import { useServersStore } from '@/stores/servers';
import { useNotificationsStore } from '@/stores/notifications';

const serversStore = useServersStore();
const notifications = useNotificationsStore();

// State
const query = ref('MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 25');
const viewMode = ref<'graph' | 'table' | 'text' | 'code'>('graph');
const isLoading = ref(false);
const error = ref<string | null>(null);
const executionTime = ref(0);
const physicsEnabled = ref(true);

// Results
const rawResult = ref<{ columns: string[]; rows: any[] } | null>(null);
const graphNodes = ref<any[]>([]);
const graphEdges = ref<any[]>([]);

// Selection
const selectedNode = ref<any | null>(null);
const selectedEdge = ref<any | null>(null);

// Graph
const graphContainer = ref<HTMLElement | null>(null);
let network: Network | null = null;
let nodesDataSet: DataSet<any> | null = null;
let edgesDataSet: DataSet<any> | null = null;

const labelColors: Record<string, string> = {};
const colorPalette = [
  '#4C8BF5', '#F5A623', '#7ED321', '#D0021B', '#9013FE',
  '#50E3C2', '#E91E63', '#00BCD4', '#FF5722', '#795548'
];

const hasResult = computed(() => rawResult.value !== null);

const resultStats = computed(() => {
  if (!rawResult.value) return '';
  const rows = rawResult.value.rows.length;
  return `${rows} row${rows !== 1 ? 's' : ''}`;
});

const formattedJson = computed(() => {
  if (!rawResult.value) return '';
  return JSON.stringify(rawResult.value.rows, null, 2);
});

function getColorForLabel(label: string): string {
  if (!labelColors[label]) {
    const index = Object.keys(labelColors).length % colorPalette.length;
    labelColors[label] = colorPalette[index];
  }
  return labelColors[label];
}

function initGraph(): void {
  if (!graphContainer.value) return;

  nodesDataSet = new DataSet();
  edgesDataSet = new DataSet();

  const options = {
    nodes: {
      shape: 'dot',
      size: 25,
      font: {
        size: 12,
        color: '#ffffff',
        face: 'Inter, system-ui, sans-serif',
      },
      borderWidth: 2,
      shadow: {
        enabled: true,
        color: 'rgba(0,0,0,0.3)',
        size: 5,
      },
    },
    edges: {
      arrows: {
        to: { enabled: true, scaleFactor: 0.8 },
      },
      color: {
        color: '#4a5568',
        highlight: '#667eea',
        hover: '#667eea',
      },
      font: {
        size: 11,
        color: '#a0aec0',
        strokeWidth: 0,
        background: 'rgba(26, 32, 44, 0.8)',
      },
      smooth: {
        enabled: true,
        type: 'continuous',
        roundness: 0.5,
      },
      width: 2,
    },
    physics: {
      enabled: physicsEnabled.value,
      stabilization: {
        enabled: true,
        iterations: 150,
        fit: true,
      },
      barnesHut: {
        gravitationalConstant: -3000,
        springLength: 150,
        springConstant: 0.04,
        damping: 0.09,
      },
    },
    interaction: {
      hover: true,
      tooltipDelay: 100,
      zoomView: true,
      dragView: true,
    },
  };

  network = new Network(
    graphContainer.value,
    { nodes: nodesDataSet, edges: edgesDataSet },
    options
  );

  network.on('click', (params: any) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      selectedNode.value = graphNodes.value.find(n => n.id === nodeId) || null;
      selectedEdge.value = null;
    } else if (params.edges.length > 0) {
      const edgeId = params.edges[0];
      const edge = edgesDataSet?.get(edgeId);
      if (edge) {
        selectedEdge.value = edge;
        selectedNode.value = null;
      }
    } else {
      clearSelection();
    }
  });
}

async function executeQuery(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) {
    notifications.error('No connection', 'Please connect to a server first');
    return;
  }

  isLoading.value = true;
  error.value = null;
  clearSelection();

  try {
    const startTime = Date.now();
    const response = await client.executeCypher(query.value);
    executionTime.value = Date.now() - startTime;

    if (!response.success || !response.data) {
      error.value = response.error || 'Query failed';
      return;
    }

    rawResult.value = {
      columns: response.data.columns,
      rows: response.data.rows,
    };

    // Process graph data
    processGraphData();

    // Update graph visualization
    updateGraph();

  } catch (err: any) {
    error.value = err.message;
  } finally {
    isLoading.value = false;
  }
}

function processGraphData(): void {
  if (!rawResult.value) return;

  const nodes = new Map<string | number, any>();
  const edges: any[] = [];
  const columns = rawResult.value.columns;

  for (const row of rawResult.value.rows) {
    for (let i = 0; i < columns.length; i++) {
      const colName = columns[i];
      const value = row[colName];

      if (!value || typeof value !== 'object') continue;

      // Detect relationships
      const hasAdjacentColumns = columns.length >= 3 && i > 0 && i < columns.length - 1;
      const isRelColumn = colName.toLowerCase() === 'r' || colName.toLowerCase().includes('rel');
      const looksLikeRel = value.type && !value.name && !value.title && !value._nexus_labels;

      const isRel = hasAdjacentColumns && (isRelColumn || looksLikeRel);

      if (isRel) {
        const prevCol = columns[i - 1];
        const nextCol = columns[i + 1];
        const startNode = row[prevCol]?._nexus_id;
        const endNode = row[nextCol]?._nexus_id;

        if (startNode && endNode) {
          edges.push({
            id: value._nexus_id || `rel-${edges.length}`,
            type: value.type || 'RELATED',
            startNode,
            endNode,
            properties: extractProperties(value),
          });
        }
      } else {
        const nodeId = value._nexus_id || value.id;
        if (nodeId && !nodes.has(nodeId)) {
          nodes.set(nodeId, {
            id: nodeId,
            labels: value._nexus_labels || value.labels || [],
            properties: extractProperties(value),
          });
        }
      }
    }
  }

  graphNodes.value = Array.from(nodes.values());
  graphEdges.value = edges;
}

function extractProperties(value: any): Record<string, any> {
  const props: Record<string, any> = {};
  for (const [key, val] of Object.entries(value)) {
    if (!key.startsWith('_nexus_') && key !== 'labels' && key !== 'type' && key !== 'id') {
      props[key] = val;
    }
  }
  return props;
}

function updateGraph(): void {
  if (!nodesDataSet || !edgesDataSet) return;

  nodesDataSet.clear();
  edgesDataSet.clear();

  const visNodes = graphNodes.value.map(node => {
    const label = node.properties?.name || node.properties?.title || `${node.id}`;
    const nodeLabel = node.labels?.[0] || 'Node';
    return {
      id: node.id,
      label: label,
      color: {
        background: getColorForLabel(nodeLabel),
        border: getColorForLabel(nodeLabel),
        highlight: { background: '#667eea', border: '#5a67d8' },
      },
      title: `${nodeLabel}\n${JSON.stringify(node.properties, null, 2)}`,
    };
  });

  const visEdges = graphEdges.value.map((rel, index) => ({
    id: rel.id || `edge-${index}`,
    from: rel.startNode,
    to: rel.endNode,
    label: rel.type,
    type: rel.type,
    properties: rel.properties,
  }));

  nodesDataSet.add(visNodes);
  edgesDataSet.add(visEdges);

  setTimeout(() => fitGraph(), 100);
}

function fitGraph(): void {
  network?.fit({ animation: { duration: 500, easingFunction: 'easeInOutQuad' } });
}

function zoomIn(): void {
  const scale = network?.getScale() || 1;
  network?.moveTo({ scale: scale * 1.3, animation: true });
}

function zoomOut(): void {
  const scale = network?.getScale() || 1;
  network?.moveTo({ scale: scale * 0.7, animation: true });
}

function togglePhysics(): void {
  physicsEnabled.value = !physicsEnabled.value;
  network?.setOptions({ physics: { enabled: physicsEnabled.value } });
}

function clearSelection(): void {
  selectedNode.value = null;
  selectedEdge.value = null;
}

function formatValue(value: any): string {
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}

function formatTableCell(value: any): string {
  if (!value) return '';
  const props = extractProperties(value);
  const parts = Object.entries(props).slice(0, 3).map(([k, v]) => `${k}: ${v}`);
  return parts.join(', ') + (Object.keys(props).length > 3 ? '...' : '');
}

function formatTextValue(value: any): string {
  if (typeof value === 'object') {
    return JSON.stringify(value, null, 2);
  }
  return String(value);
}

function isObject(value: any): boolean {
  return value !== null && typeof value === 'object';
}

function copyJson(): void {
  navigator.clipboard.writeText(formattedJson.value);
  notifications.success('Copied', 'JSON copied to clipboard');
}

watch(viewMode, (mode) => {
  if (mode === 'graph') {
    setTimeout(() => {
      if (!network) {
        initGraph();
        updateGraph();
      }
    }, 100);
  }
});

onMounted(() => {
  initGraph();
});

onUnmounted(() => {
  network?.destroy();
});
</script>
