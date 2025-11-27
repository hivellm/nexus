<template>
  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="flex-shrink-0 p-4 border-b border-border bg-bg-secondary">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-2">
            <input
              v-model="query"
              type="text"
              class="input w-96"
              placeholder="MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 100"
              @keydown.enter="loadGraph"
            />
            <button
              @click="loadGraph"
              class="btn btn-primary"
              :disabled="isLoading"
            >
              <i :class="['fas mr-1', isLoading ? 'fa-spinner fa-spin' : 'fa-search']"></i>
              Load
            </button>
          </div>
          <div class="flex items-center gap-2 text-sm text-text-muted">
            <span>Limit:</span>
            <select v-model="nodeLimit" class="input w-20">
              <option :value="50">50</option>
              <option :value="100">100</option>
              <option :value="250">250</option>
              <option :value="500">500</option>
            </select>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <button @click="fitGraph" class="btn btn-secondary text-xs" title="Fit to screen">
            <i class="fas fa-expand"></i>
          </button>
          <button @click="resetZoom" class="btn btn-secondary text-xs" title="Reset zoom">
            <i class="fas fa-compress"></i>
          </button>
          <button @click="togglePhysics" class="btn btn-secondary text-xs" :title="physicsEnabled ? 'Disable physics' : 'Enable physics'">
            <i :class="['fas', physicsEnabled ? 'fa-pause' : 'fa-play']"></i>
          </button>
        </div>
      </div>
    </div>

    <!-- Graph Container -->
    <div class="flex-1 flex min-h-0">
      <!-- Graph -->
      <div class="flex-1 relative">
        <div ref="graphContainer" class="graph-container absolute inset-0"></div>

        <!-- Loading overlay -->
        <div v-if="isLoading" class="absolute inset-0 bg-bg-primary/80 flex items-center justify-center">
          <div class="text-center">
            <i class="fas fa-spinner fa-spin text-4xl text-accent mb-2"></i>
            <p class="text-text-secondary">Loading graph...</p>
          </div>
        </div>

        <!-- Empty state -->
        <div v-if="!isLoading && nodes.length === 0" class="absolute inset-0 flex items-center justify-center">
          <div class="text-center text-text-muted">
            <i class="fas fa-project-diagram text-4xl mb-4"></i>
            <p>Enter a query to visualize the graph</p>
            <p class="text-sm mt-2">Example: MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 100</p>
          </div>
        </div>

        <!-- Stats overlay -->
        <div v-if="nodes.length > 0" class="absolute bottom-4 left-4 bg-bg-elevated border border-border rounded-lg p-3 text-sm">
          <div class="flex items-center gap-4">
            <span><i class="fas fa-circle text-info mr-1"></i>{{ nodes.length }} nodes</span>
            <span><i class="fas fa-arrow-right text-success mr-1"></i>{{ edges.length }} relationships</span>
          </div>
        </div>
      </div>

      <!-- Properties Panel -->
      <div v-if="selectedNode || selectedEdge" class="w-80 border-l border-border bg-bg-secondary p-4 overflow-y-auto">
        <div class="flex items-center justify-between mb-4">
          <h3 class="font-semibold">
            {{ selectedNode ? 'Node Properties' : 'Relationship Properties' }}
          </h3>
          <button @click="clearSelection" class="text-text-muted hover:text-text-primary">
            <i class="fas fa-times"></i>
          </button>
        </div>

        <div v-if="selectedNode">
          <div class="mb-4">
            <div class="text-xs text-text-muted mb-1">ID</div>
            <div class="font-mono text-sm">{{ selectedNode.id }}</div>
          </div>
          <div class="mb-4">
            <div class="text-xs text-text-muted mb-1">Labels</div>
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
          <div>
            <div class="text-xs text-text-muted mb-2">Properties</div>
            <div class="space-y-2">
              <div v-for="(value, key) in selectedNode.properties" :key="key" class="bg-bg-tertiary rounded p-2">
                <div class="text-xs text-text-muted">{{ key }}</div>
                <div class="text-sm font-mono break-all">{{ formatValue(value) }}</div>
              </div>
            </div>
          </div>
        </div>

        <div v-if="selectedEdge">
          <div class="mb-4">
            <div class="text-xs text-text-muted mb-1">Type</div>
            <span class="px-2 py-0.5 bg-success/20 text-success rounded text-xs">
              {{ selectedEdge.type }}
            </span>
          </div>
          <div class="mb-4">
            <div class="text-xs text-text-muted mb-1">From → To</div>
            <div class="font-mono text-sm">{{ selectedEdge.from }} → {{ selectedEdge.to }}</div>
          </div>
          <div>
            <div class="text-xs text-text-muted mb-2">Properties</div>
            <div class="space-y-2">
              <div v-for="(value, key) in selectedEdge.properties" :key="key" class="bg-bg-tertiary rounded p-2">
                <div class="text-xs text-text-muted">{{ key }}</div>
                <div class="text-sm font-mono break-all">{{ formatValue(value) }}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { Network, DataSet } from 'vis-network/standalone';
import { useServersStore } from '@/stores/servers';
import { useNotificationsStore } from '@/stores/notifications';
import type { GraphNode, GraphRelationship } from '@/types';

const serversStore = useServersStore();
const notifications = useNotificationsStore();

const graphContainer = ref<HTMLElement | null>(null);
const query = ref('MATCH (n)-[r]->(m) RETURN n, r, m LIMIT 100');
const nodeLimit = ref(100);
const isLoading = ref(false);
const physicsEnabled = ref(true);

const nodes = ref<GraphNode[]>([]);
const edges = ref<GraphRelationship[]>([]);
const selectedNode = ref<GraphNode | null>(null);
const selectedEdge = ref<any | null>(null);

let network: Network | null = null;
let nodesDataSet: DataSet<any> | null = null;
let edgesDataSet: DataSet<any> | null = null;

const labelColors: Record<string, string> = {};
const colorPalette = [
  '#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#8b5cf6',
  '#06b6d4', '#ec4899', '#84cc16', '#f97316', '#6366f1'
];

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
      size: 20,
      font: {
        size: 12,
        color: '#f8fafc',
      },
      borderWidth: 2,
    },
    edges: {
      arrows: 'to',
      color: {
        color: '#64748b',
        highlight: '#3b82f6',
      },
      font: {
        size: 10,
        color: '#94a3b8',
        strokeWidth: 0,
        background: '#1e293b',
      },
      smooth: {
        enabled: true,
        type: 'continuous',
        roundness: 0.5,
      },
    },
    physics: {
      enabled: physicsEnabled.value,
      stabilization: {
        iterations: 100,
      },
      barnesHut: {
        gravitationalConstant: -2000,
        springLength: 150,
      },
    },
    interaction: {
      hover: true,
      tooltipDelay: 200,
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
      selectedNode.value = nodes.value.find(n => n.id === nodeId) || null;
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

async function loadGraph(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) {
    notifications.error('No connection', 'Please connect to a server first');
    return;
  }

  isLoading.value = true;
  selectedNode.value = null;
  selectedEdge.value = null;

  try {
    const response = await client.getGraphData(query.value, nodeLimit.value);

    if (!response.success || !response.data) {
      notifications.error('Query failed', response.error || 'Failed to load graph data');
      return;
    }

    nodes.value = response.data.nodes;
    edges.value = response.data.relationships;

    // Update vis.js datasets
    if (nodesDataSet && edgesDataSet) {
      nodesDataSet.clear();
      edgesDataSet.clear();

      const visNodes = nodes.value.map(node => ({
        id: node.id,
        label: node.properties.name || node.properties.title || `Node ${node.id}`,
        color: getColorForLabel(node.labels[0] || 'default'),
        title: `${node.labels.join(', ')}\n${JSON.stringify(node.properties, null, 2)}`,
      }));

      const visEdges = edges.value.map((rel, index) => ({
        id: `edge-${index}`,
        from: rel.startNode,
        to: rel.endNode,
        label: rel.type,
        type: rel.type,
        properties: rel.properties,
      }));

      nodesDataSet.add(visNodes);
      edgesDataSet.add(visEdges);

      // Fit to view
      setTimeout(() => fitGraph(), 100);
    }

    notifications.success('Graph loaded', `${nodes.value.length} nodes, ${edges.value.length} relationships`);
  } catch (error: any) {
    notifications.error('Error', error.message);
  } finally {
    isLoading.value = false;
  }
}

function fitGraph(): void {
  network?.fit({ animation: true });
}

function resetZoom(): void {
  network?.moveTo({ scale: 1, animation: true });
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

onMounted(() => {
  initGraph();
});

onUnmounted(() => {
  network?.destroy();
});
</script>
