<template>
  <div id="app" class="flex flex-col h-screen bg-bg-primary text-text-primary">
    <!-- Top Bar -->
    <div class="h-12 bg-bg-secondary border-b border-border flex items-center justify-between px-4 flex-shrink-0">
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2 text-sm font-medium">
          <i class="fas fa-project-diagram text-accent"></i>
          <span>Nexus</span>
        </div>
        <div class="h-4 w-px bg-border"></div>
        <div class="flex items-center gap-2">
          <span :class="['w-2 h-2 rounded-full', isConnected ? 'bg-success' : 'bg-text-muted']"></span>
          <span class="text-xs text-text-secondary">{{ isConnected ? 'Connected' : 'Disconnected' }}</span>
        </div>
      </div>
      <div class="flex items-center gap-2">
        <div class="flex items-center gap-1">
          <router-link
            v-for="item in menuItems"
            :key="item.path"
            :to="item.path"
            class="px-3 py-1.5 text-xs rounded hover:bg-bg-hover transition-colors"
            :class="{ 'bg-bg-hover text-accent': isActive(item.path) }"
          >
            {{ item.label }}
          </router-link>
        </div>
        <div class="h-4 w-px bg-border mx-2"></div>
        <button @click="toggleTheme" class="p-1.5 hover:bg-bg-hover rounded transition-colors">
          <i :class="theme === 'dark' ? 'fas fa-sun text-warning' : 'fas fa-moon text-info'"></i>
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 overflow-hidden">
      <router-view />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRoute } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useThemeStore } from '@/stores/theme';
import { ipcBridge } from '@/services/ipc';

const route = useRoute();
const serversStore = useServersStore();
const themeStore = useThemeStore();

const isConnected = computed(() => serversStore.activeServer?.connected || false);
const theme = computed(() => themeStore.theme);

const menuItems = [
  { path: '/', label: 'Dashboard' },
  { path: '/query', label: 'Query' },
  { path: '/graph', label: 'Graph' },
  { path: '/schema', label: 'Schema' },
  { path: '/data', label: 'Data' },
  { path: '/indexes', label: 'Indexes' },
  { path: '/vector-search', label: 'Vector' },
  { path: '/config', label: 'Config' },
];

function isActive(path: string): boolean {
  return route.path === path;
}

function toggleTheme(): void {
  themeStore.toggleTheme();
}

onMounted(() => {
  themeStore.loadTheme();
});
</script>
