<template>
  <div class="space-y-6">
    <div>
      <h1 class="text-xl sm:text-2xl font-bold text-neutral-900 dark:text-white">Dashboard</h1>
      <p class="text-sm sm:text-base text-neutral-600 dark:text-neutral-400 mt-1">Welcome to Nexus Graph Database</p>
    </div>

    <!-- Stats Cards -->
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 sm:gap-6">
      <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
        <div class="text-sm font-medium text-neutral-500 dark:text-neutral-400">Nodes</div>
        <div class="text-2xl font-semibold text-neutral-900 dark:text-white mt-1">{{ stats?.nodeCount || 0 }}</div>
        <div class="text-xs text-neutral-500 dark:text-neutral-400 mt-1">Total nodes in database</div>
      </div>
      <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
        <div class="text-sm font-medium text-neutral-500 dark:text-neutral-400">Relationships</div>
        <div class="text-2xl font-semibold text-neutral-900 dark:text-white mt-1">{{ stats?.relationshipCount || 0 }}</div>
        <div class="text-xs text-neutral-500 dark:text-neutral-400 mt-1">Total relationships</div>
      </div>
      <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
        <div class="text-sm font-medium text-neutral-500 dark:text-neutral-400">Labels</div>
        <div class="text-2xl font-semibold text-neutral-900 dark:text-white mt-1">{{ stats?.labelCount || 0 }}</div>
        <div class="text-xs text-neutral-500 dark:text-neutral-400 mt-1">Node labels</div>
      </div>
      <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
        <div class="text-sm font-medium text-neutral-500 dark:text-neutral-400">Indexes</div>
        <div class="text-2xl font-semibold text-neutral-900 dark:text-white mt-1">{{ stats?.indexCount || 0 }}</div>
        <div class="text-xs text-neutral-500 dark:text-neutral-400 mt-1">Active indexes</div>
      </div>
    </div>

    <!-- Server Health -->
    <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
      <h2 class="text-lg font-semibold text-neutral-900 dark:text-white mb-4">Server Health</h2>
      <div v-if="health" class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="bg-neutral-50 dark:bg-neutral-800/50 rounded-lg p-4">
          <div class="text-sm text-neutral-500 dark:text-neutral-400 mb-1">Status</div>
          <div class="flex items-center gap-2">
            <span :class="['w-3 h-3 rounded-full', health.status === 'healthy' ? 'bg-green-500' : 'bg-red-500']"></span>
            <span class="text-lg font-semibold text-neutral-900 dark:text-white capitalize">{{ health.status }}</span>
          </div>
        </div>
        <div class="bg-neutral-50 dark:bg-neutral-800/50 rounded-lg p-4">
          <div class="text-sm text-neutral-500 dark:text-neutral-400 mb-1">Memory Usage</div>
          <div class="text-lg font-semibold text-neutral-900 dark:text-white">{{ formatBytes(health.memory?.used || 0) }} / {{ formatBytes(health.memory?.total || 0) }}</div>
          <div class="w-full bg-neutral-200 dark:bg-neutral-700 rounded-full h-2 mt-2">
            <div
              class="bg-neutral-600 dark:bg-neutral-400 h-2 rounded-full transition-all"
              :style="{ width: `${health.memory?.percentage || 0}%` }"
            ></div>
          </div>
        </div>
        <div class="bg-neutral-50 dark:bg-neutral-800/50 rounded-lg p-4">
          <div class="text-sm text-neutral-500 dark:text-neutral-400 mb-1">Uptime</div>
          <div class="text-lg font-semibold text-neutral-900 dark:text-white">{{ formatUptime(health.uptime || 0) }}</div>
        </div>
      </div>
      <div v-else class="text-neutral-500 dark:text-neutral-400 text-center py-8">
        <p>Loading server health...</p>
      </div>
    </div>

    <!-- Quick Actions & Recent Queries -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
        <h2 class="text-lg font-semibold text-neutral-900 dark:text-white mb-4">Quick Actions</h2>
        <div class="space-y-2">
          <router-link to="/query" class="flex items-center gap-3 p-3 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors">
            <div class="w-8 h-8 bg-neutral-200 dark:bg-neutral-700 rounded-lg flex items-center justify-center">
              <svg class="w-4 h-4 text-neutral-600 dark:text-neutral-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
            </div>
            <span class="text-neutral-900 dark:text-white">Open Query Editor</span>
          </router-link>
          <router-link to="/graph" class="flex items-center gap-3 p-3 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors">
            <div class="w-8 h-8 bg-neutral-200 dark:bg-neutral-700 rounded-lg flex items-center justify-center">
              <svg class="w-4 h-4 text-neutral-600 dark:text-neutral-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
            </div>
            <span class="text-neutral-900 dark:text-white">Explore Graph</span>
          </router-link>
          <router-link to="/schema" class="flex items-center gap-3 p-3 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors">
            <div class="w-8 h-8 bg-neutral-200 dark:bg-neutral-700 rounded-lg flex items-center justify-center">
              <svg class="w-4 h-4 text-neutral-600 dark:text-neutral-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
              </svg>
            </div>
            <span class="text-neutral-900 dark:text-white">View Schema</span>
          </router-link>
        </div>
      </div>

      <div class="bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800/50 rounded-lg p-6">
        <h2 class="text-lg font-semibold text-neutral-900 dark:text-white mb-4">Recent Queries</h2>
        <div v-if="recentQueries.length > 0" class="space-y-2">
          <div
            v-for="query in recentQueries.slice(0, 5)"
            :key="query.id"
            class="p-3 bg-neutral-50 dark:bg-neutral-800/50 rounded-lg cursor-pointer hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors"
            @click="runQuery(query.query)"
          >
            <div class="font-mono text-sm text-neutral-900 dark:text-white truncate">{{ query.query }}</div>
            <div class="flex items-center gap-3 mt-1 text-xs text-neutral-500 dark:text-neutral-400">
              <span>{{ formatTime(query.timestamp) }}</span>
              <span>{{ query.rowCount }} rows</span>
              <span>{{ query.executionTime }}ms</span>
            </div>
          </div>
        </div>
        <div v-else class="text-neutral-500 dark:text-neutral-400 text-center py-4">
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
