<template>
  <div class="p-6">
    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <h2 class="text-xl font-semibold text-text-primary">Database Management</h2>
        <p class="text-sm text-text-secondary mt-1">Create, manage, and switch between databases</p>
      </div>
      <button
        @click="showCreateDialog = true"
        class="flex items-center gap-2 px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent/90 transition-colors"
      >
        <i class="fas fa-plus"></i>
        <span>Create Database</span>
      </button>
    </div>

    <!-- Error Message -->
    <div v-if="error" class="mb-4 p-4 bg-error/20 border border-error/50 rounded-lg">
      <div class="flex items-center gap-2 text-error">
        <i class="fas fa-exclamation-circle"></i>
        <span>{{ error }}</span>
      </div>
    </div>

    <!-- Loading State -->
    <div v-if="isLoading" class="flex items-center justify-center py-12">
      <i class="fas fa-spinner fa-spin text-2xl text-text-muted"></i>
    </div>

    <!-- Database Grid -->
    <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      <div
        v-for="db in databases"
        :key="db.name"
        :class="[
          'bg-bg-secondary border rounded-lg p-4 transition-all cursor-pointer hover:shadow-lg',
          currentDatabase === db.name ? 'border-accent shadow-accent/20' : 'border-border hover:border-text-muted'
        ]"
        @click="handleDatabaseClick(db.name)"
      >
        <div class="flex items-start justify-between mb-3">
          <div class="flex items-center gap-2">
            <i class="fas fa-database text-lg" :class="currentDatabase === db.name ? 'text-accent' : 'text-text-muted'"></i>
            <h3 class="font-semibold text-text-primary">{{ db.name }}</h3>
          </div>
          <div class="flex items-center gap-1">
            <span v-if="currentDatabase === db.name" class="px-2 py-0.5 text-xs bg-accent/20 text-accent rounded">Active</span>
            <span v-if="db.name === defaultDatabase" class="px-2 py-0.5 text-xs bg-text-muted/20 text-text-secondary rounded">Default</span>
          </div>
        </div>

        <div class="space-y-2 text-sm">
          <div class="flex justify-between">
            <span class="text-text-secondary">Nodes</span>
            <span class="text-text-primary font-medium">{{ formatNumber(db.nodeCount) }}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-text-secondary">Relationships</span>
            <span class="text-text-primary font-medium">{{ formatNumber(db.relationshipCount) }}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-text-secondary">Storage</span>
            <span class="text-text-primary font-medium">{{ formatStorageSize(db.storageSize) }}</span>
          </div>
          <div class="flex justify-between">
            <span class="text-text-secondary">Created</span>
            <span class="text-text-primary font-medium">{{ formatDate(db.createdAt) }}</span>
          </div>
        </div>

        <div class="flex items-center gap-2 mt-4 pt-3 border-t border-border">
          <button
            v-if="currentDatabase !== db.name"
            @click.stop="switchToDatabase(db.name)"
            class="flex-1 flex items-center justify-center gap-2 px-3 py-1.5 text-sm bg-bg-tertiary hover:bg-bg-hover text-text-primary rounded transition-colors"
          >
            <i class="fas fa-exchange-alt"></i>
            Switch
          </button>
          <button
            v-if="db.name !== defaultDatabase && currentDatabase !== db.name"
            @click.stop="confirmDropDatabase(db.name)"
            class="flex items-center justify-center gap-2 px-3 py-1.5 text-sm bg-error/10 hover:bg-error/20 text-error rounded transition-colors"
          >
            <i class="fas fa-trash"></i>
          </button>
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="!isLoading && databases.length === 0" class="text-center py-12">
      <i class="fas fa-database text-4xl text-text-muted mb-4"></i>
      <h3 class="text-lg font-medium text-text-primary mb-2">No Databases Found</h3>
      <p class="text-text-secondary mb-4">Create your first database to get started</p>
      <button
        @click="showCreateDialog = true"
        class="inline-flex items-center gap-2 px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent/90 transition-colors"
      >
        <i class="fas fa-plus"></i>
        <span>Create Database</span>
      </button>
    </div>

    <!-- Create Database Dialog -->
    <div v-if="showCreateDialog" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div class="bg-bg-secondary border border-border rounded-lg shadow-xl w-full max-w-md mx-4">
        <div class="flex items-center justify-between p-4 border-b border-border">
          <h3 class="text-lg font-semibold text-text-primary">Create Database</h3>
          <button @click="closeCreateDialog" class="text-text-muted hover:text-text-primary transition-colors">
            <i class="fas fa-times"></i>
          </button>
        </div>
        <div class="p-4">
          <label class="block text-sm font-medium text-text-secondary mb-2">Database Name</label>
          <input
            v-model="newDatabaseName"
            type="text"
            placeholder="Enter database name"
            class="w-full px-3 py-2 bg-bg-tertiary border border-border rounded-lg text-text-primary placeholder-text-muted focus:outline-none focus:border-accent"
            @keyup.enter="createDatabase"
          />
          <p class="text-xs text-text-muted mt-2">Allowed: alphanumeric, underscores, and hyphens</p>
          <div v-if="createError" class="mt-2 text-sm text-error">{{ createError }}</div>
        </div>
        <div class="flex justify-end gap-3 p-4 border-t border-border">
          <button
            @click="closeCreateDialog"
            class="px-4 py-2 text-text-secondary hover:text-text-primary transition-colors"
          >
            Cancel
          </button>
          <button
            @click="createDatabase"
            :disabled="!newDatabaseName.trim() || isCreating"
            class="px-4 py-2 bg-accent text-white rounded-lg hover:bg-accent/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <i v-if="isCreating" class="fas fa-spinner fa-spin mr-2"></i>
            Create
          </button>
        </div>
      </div>
    </div>

    <!-- Drop Database Confirmation Dialog -->
    <div v-if="showDropDialog" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div class="bg-bg-secondary border border-border rounded-lg shadow-xl w-full max-w-md mx-4">
        <div class="flex items-center justify-between p-4 border-b border-border">
          <h3 class="text-lg font-semibold text-error">Drop Database</h3>
          <button @click="closeDropDialog" class="text-text-muted hover:text-text-primary transition-colors">
            <i class="fas fa-times"></i>
          </button>
        </div>
        <div class="p-4">
          <div class="flex items-start gap-3">
            <i class="fas fa-exclamation-triangle text-2xl text-error"></i>
            <div>
              <p class="text-text-primary">Are you sure you want to drop the database <strong>{{ databaseToDrop }}</strong>?</p>
              <p class="text-sm text-text-secondary mt-2">This action cannot be undone. All data in this database will be permanently deleted.</p>
            </div>
          </div>
        </div>
        <div class="flex justify-end gap-3 p-4 border-t border-border">
          <button
            @click="closeDropDialog"
            class="px-4 py-2 text-text-secondary hover:text-text-primary transition-colors"
          >
            Cancel
          </button>
          <button
            @click="dropDatabase"
            :disabled="isDropping"
            class="px-4 py-2 bg-error text-white rounded-lg hover:bg-error/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <i v-if="isDropping" class="fas fa-spinner fa-spin mr-2"></i>
            Drop Database
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { useDatabasesStore } from '@/stores/databases';
import { useNotificationsStore } from '@/stores/notifications';

const databasesStore = useDatabasesStore();
const notificationsStore = useNotificationsStore();

const databases = computed(() => databasesStore.databases);
const currentDatabase = computed(() => databasesStore.currentDatabase);
const defaultDatabase = computed(() => databasesStore.defaultDatabase);
const isLoading = computed(() => databasesStore.isLoading);
const error = computed(() => databasesStore.error);

// Create dialog state
const showCreateDialog = ref(false);
const newDatabaseName = ref('');
const isCreating = ref(false);
const createError = ref('');

// Drop dialog state
const showDropDialog = ref(false);
const databaseToDrop = ref('');
const isDropping = ref(false);

onMounted(async () => {
  await databasesStore.fetchDatabases();
});

function formatNumber(num: number): string {
  return new Intl.NumberFormat().format(num);
}

function formatStorageSize(bytes: number): string {
  return databasesStore.formatStorageSize(bytes);
}

function formatDate(timestamp: number): string {
  if (!timestamp) return 'N/A';
  return new Date(timestamp * 1000).toLocaleDateString();
}

async function handleDatabaseClick(name: string): Promise<void> {
  if (name !== currentDatabase.value) {
    await switchToDatabase(name);
  }
}

async function switchToDatabase(name: string): Promise<void> {
  const success = await databasesStore.switchDatabase(name);
  if (success) {
    notificationsStore.addNotification({
      type: 'success',
      title: 'Database Switched',
      message: `Now using database: ${name}`,
    });
  } else {
    notificationsStore.addNotification({
      type: 'error',
      title: 'Switch Failed',
      message: databasesStore.error || 'Failed to switch database',
    });
  }
}

function closeCreateDialog(): void {
  showCreateDialog.value = false;
  newDatabaseName.value = '';
  createError.value = '';
}

async function createDatabase(): Promise<void> {
  if (!newDatabaseName.value.trim()) return;

  // Validate name
  const nameRegex = /^[a-zA-Z0-9_-]+$/;
  if (!nameRegex.test(newDatabaseName.value)) {
    createError.value = 'Invalid name. Use only alphanumeric, underscores, and hyphens.';
    return;
  }

  isCreating.value = true;
  createError.value = '';

  const success = await databasesStore.createDatabase(newDatabaseName.value);

  isCreating.value = false;

  if (success) {
    notificationsStore.addNotification({
      type: 'success',
      title: 'Database Created',
      message: `Database "${newDatabaseName.value}" created successfully`,
    });
    closeCreateDialog();
  } else {
    createError.value = databasesStore.error || 'Failed to create database';
  }
}

function confirmDropDatabase(name: string): void {
  databaseToDrop.value = name;
  showDropDialog.value = true;
}

function closeDropDialog(): void {
  showDropDialog.value = false;
  databaseToDrop.value = '';
}

async function dropDatabase(): Promise<void> {
  if (!databaseToDrop.value) return;

  isDropping.value = true;

  const success = await databasesStore.dropDatabase(databaseToDrop.value);

  isDropping.value = false;

  if (success) {
    notificationsStore.addNotification({
      type: 'success',
      title: 'Database Dropped',
      message: `Database "${databaseToDrop.value}" has been deleted`,
    });
    closeDropDialog();
  } else {
    notificationsStore.addNotification({
      type: 'error',
      title: 'Drop Failed',
      message: databasesStore.error || 'Failed to drop database',
    });
  }
}
</script>
