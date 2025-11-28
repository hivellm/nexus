<template>
  <div id="app" class="flex flex-col h-screen bg-bg-primary text-text-primary">
    <!-- Main Content -->
    <div class="flex flex-1 min-h-0">
      <!-- Sidebar -->
      <aside class="w-56 bg-bg-secondary border-r border-border flex flex-col">
        <!-- Server Selector -->
        <div class="p-3 border-b border-border">
          <div class="connection-dropdown relative w-full" :class="{ 'z-dropdown': isDropdownOpen }">
            <button
              @click="serverList.length === 0 ? openServerModal() : toggleDropdown()"
              class="w-full flex items-center justify-between gap-2 p-2 rounded bg-bg-tertiary hover:bg-bg-hover transition-colors text-sm"
            >
              <div class="flex-1 min-w-0 text-left">
                <div class="text-text-primary truncate">{{ activeServer?.name || 'Select Server' }}</div>
                <div class="text-xs text-text-muted truncate">{{ activeServer?.url || '' }}{{ activeServer?.port ? ':' + activeServer.port : '' }}</div>
              </div>
              <span class="text-xs text-text-muted">▼</span>
            </button>

            <div v-if="isDropdownOpen" class="absolute top-full left-0 right-0 mt-1 bg-bg-elevated border border-border rounded shadow-lg z-dropdown">
              <div v-if="serverList.length === 0" class="p-3 text-center text-text-secondary text-sm">
                No servers available
              </div>
              <div v-else>
                <div
                  v-for="server in serverList"
                  :key="server.id"
                  :class="['flex items-center justify-between p-2 hover:bg-bg-hover cursor-pointer text-sm', { 'bg-bg-hover': activeServerId === server.id }]"
                  @click="selectServer(server.id)"
                >
                  <div class="flex-1 min-w-0">
                    <div class="text-text-primary truncate">{{ server.name }}</div>
                    <div class="text-xs text-text-muted truncate">{{ server.url }}{{ server.port ? ':' + server.port : '' }}</div>
                  </div>
                </div>
              </div>
              <div class="border-t border-border p-2">
                <button @click="openServerModal" class="w-full text-left p-2 text-sm text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded">
                  Manage Servers
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Navigation -->
        <nav class="flex-1 flex flex-col min-h-0 p-2">
          <router-link
            v-for="item in menuItems"
            :key="item.path"
            :to="item.path"
            class="px-3 py-2 text-sm text-text-secondary hover:text-text-primary hover:bg-bg-hover transition-colors rounded"
            :class="{ 'bg-bg-hover text-text-primary': isActive(item.path) }"
          >
            {{ item.label }}
          </router-link>
        </nav>
      </aside>

      <!-- Main Content -->
      <main class="flex-1 flex flex-col min-h-0">
        <div class="flex-1 overflow-auto">
          <router-view />
        </div>
      </main>
    </div>

    <!-- Server Connection Modal -->
    <ServerConnectionModal
      :is-open="showServerModal"
      :server-id="editingServerId"
      @close="handleServerModalClose"
      @saved="handleServerSaved"
    />

    <!-- Notification Center -->
    <NotificationCenter />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useThemeStore } from '@/stores/theme';
import ServerConnectionModal from '@/components/ServerConnectionModal.vue';
import NotificationCenter from '@/components/NotificationCenter.vue';
import { ipcBridge } from '@/services/ipc';

const route = useRoute();
const serversStore = useServersStore();
const themeStore = useThemeStore();

const serverList = computed(() => serversStore.serverList);
const activeServerId = computed(() => serversStore.activeServerId);
const activeServer = computed(() => serversStore.activeServer);
const isConnected = computed(() => activeServer.value?.connected || false);

const isDropdownOpen = ref(false);
const showServerModal = ref(false);
const editingServerId = ref<string | undefined>(undefined);

const menuItems = [
  { path: '/', label: 'Dashboard' },
  { path: '/query', label: 'Query' },
  { path: '/graph', label: 'Graph' },
  { path: '/schema', label: 'Schema' },
  { path: '/data', label: 'Data' },
  { path: '/indexes', label: 'Indexes' },
  { path: '/vector-search', label: 'Vector' },
  { path: '/logs', label: 'Logs' },
  { path: '/config', label: 'Config' },
];

function isActive(path: string): boolean {
  return route.path === path;
}

function toggleDropdown(): void {
  isDropdownOpen.value = !isDropdownOpen.value;
}

async function selectServer(id: string): Promise<void> {
  serversStore.setActiveServer(id);
  await serversStore.connectServer(id);
  isDropdownOpen.value = false;
}

function openServerModal(): void {
  editingServerId.value = undefined;
  showServerModal.value = true;
  isDropdownOpen.value = false;
}

function handleServerSaved(): void {
  showServerModal.value = false;
  editingServerId.value = undefined;
}

function handleServerModalClose(): void {
  showServerModal.value = false;
  editingServerId.value = undefined;
}

function handleClickOutside(event: MouseEvent): void {
  const target = event.target as HTMLElement;
  if (!target.closest('.connection-dropdown')) {
    isDropdownOpen.value = false;
  }
}

onMounted(async () => {
  document.addEventListener('click', handleClickOutside);
  themeStore.loadTheme();

  setTimeout(() => {
    if (activeServerId.value) {
      selectServer(activeServerId.value);
    }
  }, 100);
});

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
});
</script>

<style scoped>
.z-dropdown {
  z-index: 50;
}
</style>
