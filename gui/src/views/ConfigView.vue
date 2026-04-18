<template>
  <div class="p-6 space-y-6">
    <!-- Server Configuration -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-server text-info"></i>
        Server Configuration
      </h3>

      <div class="space-y-4">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div class="bg-bg-tertiary rounded-lg p-4">
            <div class="text-sm text-text-secondary mb-1">Server Address</div>
            <div class="font-mono">{{ serverConfig.host }}:{{ serverConfig.port }}</div>
          </div>
          <div class="bg-bg-tertiary rounded-lg p-4">
            <div class="text-sm text-text-secondary mb-1">Database Path</div>
            <div class="font-mono text-sm truncate">{{ serverConfig.dbPath || 'N/A' }}</div>
          </div>
          <div class="bg-bg-tertiary rounded-lg p-4">
            <div class="text-sm text-text-secondary mb-1">Max Connections</div>
            <div class="font-semibold">{{ serverConfig.maxConnections }}</div>
          </div>
          <div class="bg-bg-tertiary rounded-lg p-4">
            <div class="text-sm text-text-secondary mb-1">Query Timeout</div>
            <div class="font-semibold">{{ serverConfig.queryTimeout }}ms</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Application Settings -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-cog text-accent"></i>
        Application Settings
      </h3>

      <div class="space-y-4">
        <div class="flex items-center justify-between py-2 border-b border-border">
          <div>
            <div class="font-medium">Theme</div>
            <div class="text-sm text-text-muted">Choose your preferred color scheme</div>
          </div>
          <select v-model="settings.theme" class="input w-32" @change="updateTheme">
            <option value="dark">Dark</option>
            <option value="light">Light</option>
            <option value="system">System</option>
          </select>
        </div>

        <div class="flex items-center justify-between py-2 border-b border-border">
          <div>
            <div class="font-medium">Query Result Limit</div>
            <div class="text-sm text-text-muted">Default LIMIT for queries</div>
          </div>
          <select v-model.number="settings.defaultLimit" class="input w-32">
            <option :value="25">25</option>
            <option :value="50">50</option>
            <option :value="100">100</option>
            <option :value="250">250</option>
            <option :value="500">500</option>
          </select>
        </div>

        <div class="flex items-center justify-between py-2 border-b border-border">
          <div>
            <div class="font-medium">Auto-refresh Dashboard</div>
            <div class="text-sm text-text-muted">Automatically refresh dashboard stats</div>
          </div>
          <div class="flex items-center gap-2">
            <input
              v-model="settings.autoRefresh"
              type="checkbox"
              class="rounded bg-bg-tertiary border-border text-accent"
            />
            <select v-model.number="settings.refreshInterval" class="input w-24" :disabled="!settings.autoRefresh">
              <option :value="10">10s</option>
              <option :value="30">30s</option>
              <option :value="60">60s</option>
            </select>
          </div>
        </div>

        <div class="flex items-center justify-between py-2 border-b border-border">
          <div>
            <div class="font-medium">Query History</div>
            <div class="text-sm text-text-muted">Number of queries to keep in history</div>
          </div>
          <select v-model.number="settings.historyLimit" class="input w-32">
            <option :value="50">50</option>
            <option :value="100">100</option>
            <option :value="200">200</option>
            <option :value="500">500</option>
          </select>
        </div>

        <div class="flex items-center justify-between py-2">
          <div>
            <div class="font-medium">Graph Physics</div>
            <div class="text-sm text-text-muted">Enable physics simulation in graph view</div>
          </div>
          <input
            v-model="settings.graphPhysics"
            type="checkbox"
            class="rounded bg-bg-tertiary border-border text-accent"
          />
        </div>
      </div>

      <div class="flex justify-end mt-6">
        <button @click="saveSettings" class="btn btn-primary">
          <i class="fas fa-save mr-1"></i>
          Save Settings
        </button>
      </div>
    </div>

    <!-- Connections -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <i class="fas fa-plug text-success"></i>
          Saved Connections
        </h3>
        <button @click="showConnectionModal = true" class="btn btn-primary text-xs">
          <i class="fas fa-plus mr-1"></i>
          Add Connection
        </button>
      </div>

      <div v-if="servers.length > 0" class="space-y-2">
        <div
          v-for="server in servers"
          :key="server.id"
          class="flex items-center justify-between p-3 bg-bg-tertiary rounded-lg"
        >
          <div class="flex items-center gap-3">
            <span
              :class="[
                'w-3 h-3 rounded-full',
                server.status === 'connected' ? 'bg-success' : 'bg-text-muted'
              ]"
            ></span>
            <div>
              <div class="font-medium">{{ server.name }}</div>
              <div class="text-sm text-text-muted">{{ server.host || server.url }}{{ server.port ? ':' + server.port : '' }}</div>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <button
              v-if="server.status !== 'connected'"
              @click="connectToServer(server.id)"
              class="btn btn-secondary text-xs"
            >
              Connect
            </button>
            <button
              v-else
              @click="disconnectFromServer(server.id)"
              class="btn btn-secondary text-xs"
            >
              Disconnect
            </button>
            <button
              @click="editServer(server)"
              class="p-1 text-text-muted hover:text-text-primary"
            >
              <i class="fas fa-edit"></i>
            </button>
            <button
              @click="deleteServer(server.id)"
              class="p-1 text-text-muted hover:text-error"
            >
              <i class="fas fa-trash"></i>
            </button>
          </div>
        </div>
      </div>
      <div v-else class="text-center text-text-muted py-8">
        <i class="fas fa-plug text-2xl mb-2"></i>
        <p>No saved connections</p>
      </div>
    </div>

    <!-- About -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-info-circle text-warning"></i>
        About Nexus
      </h3>

      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="bg-bg-tertiary rounded-lg p-4 text-center">
          <div class="text-2xl font-bold text-accent">{{ version }}</div>
          <div class="text-sm text-text-muted">Version</div>
        </div>
        <div class="bg-bg-tertiary rounded-lg p-4 text-center">
          <div class="text-2xl font-bold text-info">Rust</div>
          <div class="text-sm text-text-muted">Backend</div>
        </div>
        <div class="bg-bg-tertiary rounded-lg p-4 text-center">
          <div class="text-2xl font-bold text-success">Vue 3</div>
          <div class="text-sm text-text-muted">Frontend</div>
        </div>
        <div class="bg-bg-tertiary rounded-lg p-4 text-center">
          <div class="text-2xl font-bold text-warning">Electron</div>
          <div class="text-sm text-text-muted">Platform</div>
        </div>
      </div>

      <div class="mt-4 text-center text-text-muted text-sm">
        <p>Nexus Graph Database - A high-performance graph database with Cypher support</p>
        <p class="mt-1">
          <a href="https://github.com/your-repo/nexus" class="text-accent hover:underline" target="_blank">
            <i class="fab fa-github mr-1"></i>
            GitHub Repository
          </a>
        </p>
      </div>
    </div>

    <!-- Connection Modal -->
    <ServerConnectionModal
      v-if="showConnectionModal"
      :editing-server="editingServer"
      @close="closeConnectionModal"
      @saved="onConnectionSaved"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue';
import { useServersStore } from '@/stores/servers';
import { useThemeStore } from '@/stores/theme';
import { useNotificationsStore } from '@/stores/notifications';
import ServerConnectionModal from '@/components/ServerConnectionModal.vue';
import type { Server } from '@/types';

const serversStore = useServersStore();
const themeStore = useThemeStore();
const notifications = useNotificationsStore();

const version = ref('1.0.0');
const showConnectionModal = ref(false);
const editingServer = ref<Server | null>(null);

const servers = computed(() => serversStore.serverList);

const serverConfig = reactive({
  host: 'localhost',
  port: 7687,
  dbPath: '',
  maxConnections: 100,
  queryTimeout: 30000,
});

const settings = reactive({
  theme: 'dark',
  defaultLimit: 100,
  autoRefresh: true,
  refreshInterval: 30,
  historyLimit: 100,
  graphPhysics: true,
});

async function loadServerConfig(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  try {
    const response = await client.getConfig?.();
    if (response?.success && response.data) {
      Object.assign(serverConfig, response.data);
    }
  } catch (error) {
    console.error('Failed to load server config:', error);
  }
}

function loadSettings(): void {
  const stored = localStorage.getItem('nexus-settings');
  if (stored) {
    try {
      Object.assign(settings, JSON.parse(stored));
    } catch (e) {
      console.error('Failed to parse settings:', e);
    }
  }
  settings.theme = themeStore.theme;
}

function saveSettings(): void {
  localStorage.setItem('nexus-settings', JSON.stringify(settings));
  notifications.success('Settings saved', 'Your preferences have been saved');
}

function updateTheme(): void {
  if (settings.theme === 'dark' || settings.theme === 'light') {
    themeStore.setTheme(settings.theme);
  }
}

async function connectToServer(serverId: string): Promise<void> {
  await serversStore.connect(serverId);
}

async function disconnectFromServer(serverId: string): Promise<void> {
  await serversStore.disconnect(serverId);
}

function editServer(server: Server): void {
  editingServer.value = server;
  showConnectionModal.value = true;
}

async function deleteServer(serverId: string): Promise<void> {
  if (!confirm('Are you sure you want to delete this connection?')) return;
  await serversStore.removeServer(serverId);
  notifications.success('Connection deleted', 'The connection has been removed');
}

function closeConnectionModal(): void {
  showConnectionModal.value = false;
  editingServer.value = null;
}

function onConnectionSaved(): void {
  closeConnectionModal();
  notifications.success('Connection saved', 'The connection has been saved');
}

onMounted(() => {
  loadServerConfig();
  loadSettings();
});
</script>
