<template>
  <div id="app" class="flex flex-col h-screen bg-bg-primary text-text-primary">
    <!-- Custom Titlebar -->
    <div class="h-8 bg-bg-secondary border-b border-border flex items-center justify-between px-4 drag-region flex-shrink-0">
      <div class="flex items-center gap-2 text-xs text-text-secondary">
        <i class="fas fa-project-diagram"></i>
        <span>Nexus GUI</span>
      </div>
      <div class="flex items-center gap-1 no-drag">
        <button @click="minimizeWindow" class="p-1 w-8 h-8 hover:bg-bg-hover transition-colors rounded">
          <i class="fas fa-window-minimize text-xs"></i>
        </button>
        <button @click="maximizeWindow" class="p-1 w-8 h-8 hover:bg-bg-hover transition-colors rounded">
          <i class="fas fa-window-maximize text-xs"></i>
        </button>
        <button @click="closeWindow" class="p-1 w-8 h-8 hover:bg-red-600 hover:text-white transition-colors rounded">
          <i class="fas fa-times text-xs"></i>
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div class="flex flex-1 min-h-0">
      <!-- Sidebar -->
      <aside class="w-64 bg-bg-secondary border-r border-border flex flex-col">

        <!-- Connection Selector -->
        <div class="h-14 flex items-center px-4 border-b border-border flex-shrink-0">
          <div class="connection-dropdown relative w-full" :class="{ 'z-50': isDropdownOpen }">
            <button
              @click="serverList.length === 0 ? openConnectionManager() : toggleDropdown()"
              class="w-full flex items-center justify-between gap-3 p-3 rounded-lg bg-bg-tertiary hover:bg-bg-hover transition-colors cursor-pointer"
            >
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 whitespace-nowrap overflow-hidden text-ellipsis">
                  <span class="text-sm font-medium text-text-primary">{{ activeServer?.name || 'Select Connection' }}</span>
                  <span class="text-text-muted">•</span>
                  <span :class="['w-2 h-2 rounded-full', activeServer?.status === 'online' ? 'bg-success' : 'bg-text-muted']"></span>
                  <span class="text-xs text-text-secondary">{{ activeServer?.status || 'No connections' }}</span>
                </div>
              </div>
              <i class="fas fa-chevron-down text-xs text-text-muted transition-transform" :class="{ 'rotate-180': isDropdownOpen }"></i>
            </button>

            <div v-if="isDropdownOpen" class="absolute top-full left-0 right-0 mt-1 bg-bg-elevated border border-border rounded-lg shadow-lg z-50">
              <div v-if="serverList.length === 0" class="p-4 text-center text-text-secondary">
                <i class="fas fa-exclamation-circle mb-2 block"></i>
                <span class="text-sm">No connections available</span>
              </div>
              <div v-else>
                <div
                  v-for="server in serverList"
                  :key="server.id"
                  :class="['flex items-center justify-between p-3 hover:bg-bg-hover cursor-pointer transition-colors', { 'bg-bg-hover': activeServerId === server.id }]"
                  @click="selectConnection(server.id)"
                >
                  <div class="flex-1 min-w-0">
                    <div class="text-sm font-medium text-text-primary truncate">{{ server.name }}</div>
                    <div class="flex items-center gap-2 text-xs text-text-secondary">
                      <span :class="['w-1.5 h-1.5 rounded-full', server.status === 'online' ? 'bg-success' : 'bg-text-muted']"></span>
                      {{ server.host || server.url }}:{{ server.port }}
                    </div>
                  </div>
                  <div v-if="activeServerId === server.id" class="text-success">
                    <i class="fas fa-check text-sm"></i>
                  </div>
                </div>
              </div>

              <div class="border-t border-border p-3">
                <button @click="openConnectionManager" class="w-full flex items-center gap-2 px-3 py-2 text-sm text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors">
                  <i class="fas fa-cog"></i>
                  Manage Connections
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Database Selector -->
        <div v-if="isConnected" class="h-12 flex items-center px-4 border-b border-border flex-shrink-0">
          <div class="database-dropdown relative w-full" :class="{ 'z-40': isDatabaseDropdownOpen }">
            <button
              @click="toggleDatabaseDropdown"
              class="w-full flex items-center justify-between gap-3 p-2 rounded-lg bg-bg-tertiary hover:bg-bg-hover transition-colors cursor-pointer"
              :disabled="isLoadingDatabases"
            >
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 whitespace-nowrap overflow-hidden text-ellipsis">
                  <i class="fas fa-layer-group text-xs text-text-muted"></i>
                  <span class="text-xs font-medium text-text-primary">{{ currentDatabase }}</span>
                </div>
              </div>
              <i v-if="isLoadingDatabases" class="fas fa-spinner fa-spin text-xs text-text-muted"></i>
              <i v-else class="fas fa-chevron-down text-xs text-text-muted transition-transform" :class="{ 'rotate-180': isDatabaseDropdownOpen }"></i>
            </button>

            <div v-if="isDatabaseDropdownOpen" class="absolute top-full left-0 right-0 mt-1 bg-bg-elevated border border-border rounded-lg shadow-lg z-40">
              <div v-if="databases.length === 0" class="p-3 text-center text-text-secondary">
                <span class="text-xs">No databases available</span>
              </div>
              <div v-else class="max-h-48 overflow-y-auto">
                <div
                  v-for="db in databases"
                  :key="db.name"
                  :class="['flex items-center justify-between p-2 hover:bg-bg-hover cursor-pointer transition-colors', { 'bg-bg-hover': currentDatabase === db.name }]"
                  @click="selectDatabase(db.name)"
                >
                  <div class="flex-1 min-w-0">
                    <div class="text-xs font-medium text-text-primary truncate">{{ db.name }}</div>
                  </div>
                  <div v-if="currentDatabase === db.name" class="text-success">
                    <i class="fas fa-check text-xs"></i>
                  </div>
                </div>
              </div>

              <div class="border-t border-border p-2">
                <button @click="openDatabaseManager" class="w-full flex items-center gap-2 px-2 py-1.5 text-xs text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors">
                  <i class="fas fa-cog"></i>
                  Manage Databases
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Navigation -->
        <nav class="flex-1 overflow-y-auto p-4">
          <router-link
            v-for="item in menuItems"
            :key="item.path"
            :to="item.path"
            :class="[
              'flex items-center gap-3 py-2 text-text-secondary hover:text-text-primary transition-colors cursor-pointer text-sm',
              { 'text-text-primary': isActive(item.path) }
            ]"
          >
            <i :class="[item.icon, 'w-4 text-center']"></i>
            <span>{{ item.label }}</span>
          </router-link>
        </nav>
      </aside>

      <!-- Main Content -->
      <main class="flex-1 flex flex-col">
        <header class="h-14 border-b border-border flex items-center justify-between px-6 bg-bg-secondary">
          <div class="flex items-center gap-3">
            <i :class="[pageIcon, 'text-text-secondary']"></i>
            <span class="text-lg font-semibold text-text-primary">{{ pageTitle }}</span>
          </div>
          <div class="flex items-center gap-2">
            <!-- Global connection status -->
            <div class="flex items-center gap-2 ml-4 pl-4 border-l border-border">
              <span :class="['w-2 h-2 rounded-full', isConnected ? 'bg-success' : 'bg-text-muted']"></span>
              <span class="text-xs text-text-secondary">{{ isConnected ? 'Connected' : 'Disconnected' }}</span>
            </div>
          </div>
        </header>

        <div class="flex-1 overflow-y-auto">
          <router-view />
        </div>
      </main>
    </div>

    <!-- Notification Center -->
    <NotificationCenter />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useDatabasesStore } from '@/stores/databases';
import NotificationCenter from '@/components/NotificationCenter.vue';

const route = useRoute();
const router = useRouter();
const serversStore = useServersStore();
const databasesStore = useDatabasesStore();

const serverList = computed(() => serversStore.serverList);
const activeServer = computed(() => serversStore.activeServer);
const activeServerId = computed(() => serversStore.activeServerId);
const isConnected = computed(() => activeServer.value?.status === 'online');

const isDropdownOpen = ref(false);
const isDatabaseDropdownOpen = ref(false);

// Database computed properties
const databases = computed(() => databasesStore.databases);
const currentDatabase = computed(() => databasesStore.currentDatabase);
const isLoadingDatabases = computed(() => databasesStore.isLoading);

const menuItems = [
  { path: '/', label: 'Dashboard', icon: 'fas fa-tachometer-alt' },
  { path: '/query', label: 'Query', icon: 'fas fa-terminal' },
  { path: '/graph', label: 'Graph', icon: 'fas fa-project-diagram' },
  { path: '/schema', label: 'Schema', icon: 'fas fa-sitemap' },
  { path: '/databases', label: 'Databases', icon: 'fas fa-layer-group' },
  { path: '/data', label: 'Data', icon: 'fas fa-database' },
  { path: '/indexes', label: 'Indexes', icon: 'fas fa-list' },
  { path: '/vector-search', label: 'Vector Search', icon: 'fas fa-search' },
  { path: '/logs', label: 'Logs', icon: 'fas fa-file-alt' },
  { path: '/config', label: 'Configuration', icon: 'fas fa-cog' },
];

const pageTitle = computed(() => {
  const titles: Record<string, string> = {
    '/': 'Dashboard',
    '/query': 'Query Editor',
    '/graph': 'Graph Visualization',
    '/schema': 'Schema',
    '/databases': 'Databases',
    '/data': 'Data Management',
    '/indexes': 'Indexes',
    '/vector-search': 'Vector Search',
    '/logs': 'Logs',
    '/config': 'Configuration',
  };
  return titles[route.path] || 'Nexus GUI';
});

const pageIcon = computed(() => {
  const item = menuItems.find(m => m.path === route.path);
  return item?.icon || 'fas fa-project-diagram';
});

function isActive(path: string): boolean {
  return route.path === path;
}

function toggleDropdown(): void {
  isDropdownOpen.value = !isDropdownOpen.value;
}

async function selectConnection(serverId: string): Promise<void> {
  isDropdownOpen.value = false;
  serversStore.setActiveServer(serverId);
}

function openConnectionManager(): void {
  isDropdownOpen.value = false;
  router.push('/config');
}

function toggleDatabaseDropdown(): void {
  isDatabaseDropdownOpen.value = !isDatabaseDropdownOpen.value;
}

async function selectDatabase(name: string): Promise<void> {
  isDatabaseDropdownOpen.value = false;
  await databasesStore.switchDatabase(name);
}

function openDatabaseManager(): void {
  isDatabaseDropdownOpen.value = false;
  router.push('/databases');
}

function minimizeWindow(): void {
  window.electronAPI?.windowControl('minimize');
}

function maximizeWindow(): void {
  window.electronAPI?.windowControl('maximize');
}

function closeWindow(): void {
  window.electronAPI?.windowControl('close');
}

onMounted(async () => {
  // Auto-connect to active server on mount
  if (serversStore.activeServerId) {
    await serversStore.connectServer(serversStore.activeServerId);
    // Fetch databases after connecting
    if (isConnected.value) {
      await databasesStore.fetchDatabases();
    }
  }

  // Close dropdown when clicking outside
  document.addEventListener('click', handleClickOutside);
});

// Watch for connection status changes to fetch databases
watch(isConnected, async (connected) => {
  if (connected) {
    await databasesStore.fetchDatabases();
  }
});

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
});

function handleClickOutside(event: MouseEvent): void {
  const target = event.target as HTMLElement;
  if (!target.closest('.connection-dropdown')) {
    isDropdownOpen.value = false;
  }
  if (!target.closest('.database-dropdown')) {
    isDatabaseDropdownOpen.value = false;
  }
}
</script>

<style scoped>
.drag-region {
  -webkit-app-region: drag;
}
.no-drag {
  -webkit-app-region: no-drag;
}
</style>
