<template>
  <div class="p-6 space-y-6">
    <!-- Stats Cards -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      <StatsCard
        title="Nodes"
        :value="stats?.nodeCount || 0"
        icon="fas fa-circle"
        color="text-info"
      />
      <StatsCard
        title="Relationships"
        :value="stats?.relationshipCount || 0"
        icon="fas fa-arrow-right"
        color="text-success"
      />
      <StatsCard
        title="Labels"
        :value="stats?.labelCount || 0"
        icon="fas fa-tag"
        color="text-warning"
      />
      <StatsCard
        title="Indexes"
        :value="stats?.indexCount || 0"
        icon="fas fa-list-ol"
        color="text-accent"
      />
    </div>

    <!-- Server Health -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-heartbeat text-error"></i>
        Server Health
      </h3>
      <div v-if="health" class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="bg-bg-tertiary rounded-lg p-4">
          <div class="text-sm text-text-secondary mb-1">Status</div>
          <div class="flex items-center gap-2">
            <span :class="['w-3 h-3 rounded-full', health.status === 'healthy' ? 'bg-success' : 'bg-error']"></span>
            <span class="text-lg font-semibold capitalize">{{ health.status }}</span>
          </div>
        </div>
        <div class="bg-bg-tertiary rounded-lg p-4">
          <div class="text-sm text-text-secondary mb-1">Memory Usage</div>
          <div class="text-lg font-semibold">{{ formatBytes(health.memory?.used || 0) }} / {{ formatBytes(health.memory?.total || 0) }}</div>
          <div class="w-full bg-bg-hover rounded-full h-2 mt-2">
            <div
              class="bg-info h-2 rounded-full transition-all"
              :style="{ width: `${health.memory?.percentage || 0}%` }"
            ></div>
          </div>
        </div>
        <div class="bg-bg-tertiary rounded-lg p-4">
          <div class="text-sm text-text-secondary mb-1">Uptime</div>
          <div class="text-lg font-semibold">{{ formatUptime(health.uptime || 0) }}</div>
        </div>
      </div>
      <div v-else class="text-text-muted text-center py-8">
        <i class="fas fa-spinner fa-spin text-2xl mb-2"></i>
        <p>Loading server health...</p>
      </div>
    </div>

    <!-- Quick Actions -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div class="card">
        <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
          <i class="fas fa-bolt text-warning"></i>
          Quick Actions
        </h3>
        <div class="space-y-2">
          <router-link to="/query" class="flex items-center gap-3 p-3 bg-bg-tertiary rounded-lg hover:bg-bg-hover transition-colors">
            <i class="fas fa-terminal text-accent"></i>
            <span>Open Query Editor</span>
          </router-link>
          <router-link to="/graph" class="flex items-center gap-3 p-3 bg-bg-tertiary rounded-lg hover:bg-bg-hover transition-colors">
            <i class="fas fa-project-diagram text-success"></i>
            <span>Explore Graph</span>
          </router-link>
          <router-link to="/schema" class="flex items-center gap-3 p-3 bg-bg-tertiary rounded-lg hover:bg-bg-hover transition-colors">
            <i class="fas fa-sitemap text-info"></i>
            <span>View Schema</span>
          </router-link>
        </div>
      </div>

      <div class="card">
        <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
          <i class="fas fa-history text-info"></i>
          Recent Queries
        </h3>
        <div v-if="recentQueries.length > 0" class="space-y-2">
          <div
            v-for="query in recentQueries.slice(0, 5)"
            :key="query.id"
            class="p-3 bg-bg-tertiary rounded-lg cursor-pointer hover:bg-bg-hover transition-colors"
            @click="runQuery(query.query)"
          >
            <div class="font-mono text-sm text-text-primary truncate">{{ query.query }}</div>
            <div class="flex items-center gap-3 mt-1 text-xs text-text-muted">
              <span>{{ formatTime(query.timestamp) }}</span>
              <span>{{ query.rowCount }} rows</span>
              <span>{{ query.executionTime }}ms</span>
            </div>
          </div>
        </div>
        <div v-else class="text-text-muted text-center py-4">
          <i class="fas fa-inbox text-2xl mb-2"></i>
          <p>No recent queries</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useQueryStore } from '@/stores/query';
import StatsCard from '@/components/StatsCard.vue';
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

function runQuery(query: string): void {
  queryStore.setQuery(query);
  router.push('/query');
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
