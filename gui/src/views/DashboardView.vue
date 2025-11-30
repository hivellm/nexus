<template>
  <div class="p-8">
    <!-- Stats Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
      <StatCard
        icon="fas fa-circle-nodes"
        :value="formatNumber(stats?.nodeCount || 0)"
        label="Nodes"
        variant="primary"
      />

      <StatCard
        icon="fas fa-arrows-left-right"
        :value="formatNumber(stats?.relationshipCount || 0)"
        label="Relationships"
        variant="success"
      />

      <StatCard
        icon="fas fa-tags"
        :value="formatNumber(stats?.labelCount || 0)"
        label="Labels"
        variant="info"
      />

      <StatCard
        icon="fas fa-list"
        :value="formatNumber(stats?.indexCount || 0)"
        label="Indexes"
        variant="secondary"
      />
    </div>

    <!-- Quick Actions -->
    <div class="bg-bg-secondary border border-border rounded-xl p-6 mb-8">
      <h2 class="text-xl font-semibold text-text-primary mb-6">Quick Actions</h2>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <button @click="$router.push('/query')" class="bg-bg-tertiary border border-border rounded-lg p-6 text-left hover:bg-bg-hover hover:border-border-light transition-colors group">
          <i class="fas fa-terminal text-2xl text-text-secondary group-hover:text-text-primary mb-3 block"></i>
          <h3 class="text-lg font-medium text-text-primary mb-2">Query Editor</h3>
          <p class="text-sm text-text-secondary">Run Cypher queries</p>
        </button>

        <button @click="$router.push('/graph')" class="bg-bg-tertiary border border-border rounded-lg p-6 text-left hover:bg-bg-hover hover:border-border-light transition-colors group">
          <i class="fas fa-project-diagram text-2xl text-text-secondary group-hover:text-text-primary mb-3 block"></i>
          <h3 class="text-lg font-medium text-text-primary mb-2">Explore Graph</h3>
          <p class="text-sm text-text-secondary">Visualize relationships</p>
        </button>

        <button @click="$router.push('/schema')" class="bg-bg-tertiary border border-border rounded-lg p-6 text-left hover:bg-bg-hover hover:border-border-light transition-colors group">
          <i class="fas fa-sitemap text-2xl text-text-secondary group-hover:text-text-primary mb-3 block"></i>
          <h3 class="text-lg font-medium text-text-primary mb-2">View Schema</h3>
          <p class="text-sm text-text-secondary">Labels and types</p>
        </button>

        <button @click="refreshData" class="bg-bg-tertiary border border-border rounded-lg p-6 text-left hover:bg-bg-hover hover:border-border-light transition-colors group">
          <i class="fas fa-sync text-2xl text-text-secondary group-hover:text-text-primary mb-3 block"></i>
          <h3 class="text-lg font-medium text-text-primary mb-2">Refresh Data</h3>
          <p class="text-sm text-text-secondary">Reload statistics</p>
        </button>
      </div>
    </div>

    <!-- Server Health -->
    <div class="bg-bg-secondary border border-border rounded-xl p-6 mb-8">
      <h2 class="text-xl font-semibold text-text-primary mb-6">Server Health</h2>
      <div v-if="health" class="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div class="bg-bg-tertiary border border-border rounded-lg p-4">
          <div class="text-sm text-text-secondary mb-2">Status</div>
          <div class="flex items-center gap-2">
            <span :class="['w-3 h-3 rounded-full', health.status === 'healthy' ? 'bg-success' : 'bg-error']"></span>
            <span class="text-lg font-semibold text-text-primary capitalize">{{ health.status }}</span>
          </div>
        </div>
        <div class="bg-bg-tertiary border border-border rounded-lg p-4">
          <div class="text-sm text-text-secondary mb-2">Memory Usage</div>
          <div class="text-lg font-semibold text-text-primary">{{ formatBytes(health.memory?.used || 0) }} / {{ formatBytes(health.memory?.total || 0) }}</div>
          <div class="w-full bg-bg-primary rounded-full h-2 mt-2">
            <div
              class="bg-info h-2 rounded-full transition-all"
              :style="{ width: `${health.memory?.percentage || 0}%` }"
            ></div>
          </div>
        </div>
        <div class="bg-bg-tertiary border border-border rounded-lg p-4">
          <div class="text-sm text-text-secondary mb-2">Uptime</div>
          <div class="text-lg font-semibold text-text-primary">{{ formatUptime(health.uptime || 0) }}</div>
        </div>
      </div>
      <div v-else class="text-text-secondary text-center py-8">
        <i class="fas fa-spinner fa-spin text-2xl mb-3 block"></i>
        <p class="text-sm">Loading server health...</p>
      </div>
    </div>

    <!-- Recent Queries -->
    <div class="bg-bg-secondary border border-border rounded-xl p-6">
      <h2 class="text-xl font-semibold text-text-primary mb-6">Recent Queries</h2>
      <div v-if="recentQueries.length > 0" class="space-y-2">
        <div
          v-for="query in recentQueries.slice(0, 5)"
          :key="query.id"
          class="p-4 bg-bg-tertiary border border-border rounded-lg cursor-pointer hover:bg-bg-hover hover:border-border-light transition-colors"
          @click="runQuery(query.query)"
        >
          <div class="font-mono text-sm text-text-primary truncate">{{ query.query }}</div>
          <div class="flex items-center gap-4 mt-2 text-xs text-text-secondary">
            <span><i class="fas fa-clock mr-1"></i>{{ formatTime(query.timestamp) }}</span>
            <span><i class="fas fa-table mr-1"></i>{{ query.rowCount }} rows</span>
            <span><i class="fas fa-bolt mr-1"></i>{{ query.executionTime }}ms</span>
          </div>
        </div>
      </div>
      <div v-else class="text-text-secondary text-center py-8">
        <i class="fas fa-history text-2xl mb-3 block"></i>
        <p class="text-sm">No recent queries</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useQueryStore } from '@/stores/query';
import StatCard from '@/components/StatCard.vue';
import type { DatabaseStats, ServerHealth } from '@/types';

const router = useRouter();
const serversStore = useServersStore();
const queryStore = useQueryStore();

const stats = ref<DatabaseStats | null>(null);
const health = ref<ServerHealth | null>(null);
let refreshInterval: ReturnType<typeof setInterval> | null = null;

const recentQueries = computed(() => queryStore.recentQueries);

async function loadData(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  try {
    const [statsResponse, healthResponse] = await Promise.all([
      client.getStats(),
      client.healthCheck(),
    ]);

    if (statsResponse.success && statsResponse.data) {
      stats.value = statsResponse.data;
    }
    if (healthResponse.success && healthResponse.data) {
      health.value = healthResponse.data;
    }
  } catch (error) {
    console.error('Failed to load dashboard data:', error);
  }
}

async function refreshData(): Promise<void> {
  await loadData();
}

function runQuery(query: string): void {
  queryStore.setQuery(query);
  router.push('/query');
}

function formatNumber(num: number): string {
  return new Intl.NumberFormat().format(num);
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function formatTime(date: Date): string {
  const now = new Date();
  const diff = now.getTime() - new Date(date).getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);

  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return new Date(date).toLocaleDateString();
}

onMounted(() => {
  loadData();
  refreshInterval = setInterval(loadData, 30000);
});

onUnmounted(() => {
  if (refreshInterval) {
    clearInterval(refreshInterval);
  }
});
</script>
