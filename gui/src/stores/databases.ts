import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { DatabaseInfo } from '@/types';
import { useServersStore } from './servers';

const CURRENT_DB_KEY = 'nexus-desktop-current-database';

export const useDatabasesStore = defineStore('databases', () => {
  const databases = ref<DatabaseInfo[]>([]);
  const currentDatabase = ref<string>('neo4j');
  const defaultDatabase = ref<string>('neo4j');
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  // Load current database from localStorage
  function loadFromStorage(): void {
    try {
      const stored = localStorage.getItem(CURRENT_DB_KEY);
      if (stored) {
        currentDatabase.value = stored;
      }
    } catch (e) {
      console.error('Failed to load current database from storage:', e);
    }
  }

  function saveToStorage(): void {
    try {
      localStorage.setItem(CURRENT_DB_KEY, currentDatabase.value);
    } catch (e) {
      console.error('Failed to save current database to storage:', e);
    }
  }

  loadFromStorage();

  const databaseNames = computed(() => databases.value.map(db => db.name));

  const currentDatabaseInfo = computed(() =>
    databases.value.find(db => db.name === currentDatabase.value)
  );

  async function fetchDatabases(): Promise<void> {
    const serversStore = useServersStore();
    const client = serversStore.activeClient;

    if (!client) {
      error.value = 'No active server connection';
      return;
    }

    isLoading.value = true;
    error.value = null;

    try {
      const response = await client.listDatabases();
      if (response.success && response.data) {
        databases.value = response.data.databases;
        defaultDatabase.value = response.data.defaultDatabase;

        // If current database doesn't exist in the list, reset to default
        if (!databaseNames.value.includes(currentDatabase.value)) {
          currentDatabase.value = defaultDatabase.value;
          saveToStorage();
        }
      } else {
        error.value = response.error || 'Failed to fetch databases';
      }
    } catch (e: any) {
      error.value = e.message || 'Failed to fetch databases';
    } finally {
      isLoading.value = false;
    }
  }

  async function switchDatabase(name: string): Promise<boolean> {
    const serversStore = useServersStore();
    const client = serversStore.activeClient;

    if (!client) {
      error.value = 'No active server connection';
      return false;
    }

    isLoading.value = true;
    error.value = null;

    try {
      const response = await client.switchDatabase(name);
      if (response.success && response.data?.success) {
        currentDatabase.value = name;
        saveToStorage();
        return true;
      } else {
        error.value = response.error || response.data?.message || 'Failed to switch database';
        return false;
      }
    } catch (e: any) {
      error.value = e.message || 'Failed to switch database';
      return false;
    } finally {
      isLoading.value = false;
    }
  }

  async function createDatabase(name: string): Promise<boolean> {
    const serversStore = useServersStore();
    const client = serversStore.activeClient;

    if (!client) {
      error.value = 'No active server connection';
      return false;
    }

    isLoading.value = true;
    error.value = null;

    try {
      const response = await client.createDatabase(name);
      if (response.success && response.data?.success) {
        await fetchDatabases();
        return true;
      } else {
        error.value = response.error || response.data?.message || 'Failed to create database';
        return false;
      }
    } catch (e: any) {
      error.value = e.message || 'Failed to create database';
      return false;
    } finally {
      isLoading.value = false;
    }
  }

  async function dropDatabase(name: string): Promise<boolean> {
    const serversStore = useServersStore();
    const client = serversStore.activeClient;

    if (!client) {
      error.value = 'No active server connection';
      return false;
    }

    // Prevent dropping default database or current database
    if (name === defaultDatabase.value) {
      error.value = 'Cannot drop the default database';
      return false;
    }

    if (name === currentDatabase.value) {
      error.value = 'Cannot drop the currently active database. Switch to a different database first.';
      return false;
    }

    isLoading.value = true;
    error.value = null;

    try {
      const response = await client.dropDatabase(name);
      if (response.success && response.data?.success) {
        await fetchDatabases();
        return true;
      } else {
        error.value = response.error || response.data?.message || 'Failed to drop database';
        return false;
      }
    } catch (e: any) {
      error.value = e.message || 'Failed to drop database';
      return false;
    } finally {
      isLoading.value = false;
    }
  }

  async function refreshCurrentDatabase(): Promise<void> {
    const serversStore = useServersStore();
    const client = serversStore.activeClient;

    if (!client) return;

    try {
      const response = await client.getCurrentDatabase();
      if (response.success && response.data) {
        currentDatabase.value = response.data;
        saveToStorage();
      }
    } catch (e) {
      console.error('Failed to refresh current database:', e);
    }
  }

  function formatStorageSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  return {
    databases,
    currentDatabase,
    defaultDatabase,
    isLoading,
    error,
    databaseNames,
    currentDatabaseInfo,
    fetchDatabases,
    switchDatabase,
    createDatabase,
    dropDatabase,
    refreshCurrentDatabase,
    formatStorageSize,
  };
});
