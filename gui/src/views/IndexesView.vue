<template>
  <div class="p-6 space-y-6">
    <!-- Create Index Section -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <i class="fas fa-plus-circle text-success"></i>
          Create Index
        </h3>
      </div>

      <form @submit.prevent="createIndex" class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div>
          <label class="block text-sm text-text-secondary mb-1">Label</label>
          <select v-model="newIndex.label" class="input" required>
            <option value="">Select label...</option>
            <option v-for="label in labels" :key="label" :value="label">{{ label }}</option>
          </select>
        </div>
        <div>
          <label class="block text-sm text-text-secondary mb-1">Property</label>
          <input
            v-model="newIndex.property"
            type="text"
            class="input"
            placeholder="property_name"
            required
          />
        </div>
        <div>
          <label class="block text-sm text-text-secondary mb-1">Type</label>
          <select v-model="newIndex.type" class="input">
            <option value="btree">B-Tree</option>
            <option value="hash">Hash</option>
            <option value="fulltext">Full-Text</option>
          </select>
        </div>
        <div class="flex items-end">
          <button type="submit" class="btn btn-primary w-full" :disabled="isCreating">
            <i :class="['fas mr-1', isCreating ? 'fa-spinner fa-spin' : 'fa-plus']"></i>
            Create Index
          </button>
        </div>
      </form>
    </div>

    <!-- Existing Indexes -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <i class="fas fa-list-ol text-accent"></i>
          Indexes
        </h3>
        <button @click="refreshIndexes" class="btn btn-secondary text-xs">
          <i :class="['fas mr-1', isLoading ? 'fa-spinner fa-spin' : 'fa-sync']"></i>
          Refresh
        </button>
      </div>

      <div v-if="indexes.length > 0" class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Label</th>
              <th>Properties</th>
              <th>Type</th>
              <th>State</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="index in indexes" :key="index.name">
              <td class="font-mono text-sm">{{ index.name }}</td>
              <td>
                <span class="px-2 py-0.5 bg-info/20 text-info rounded text-xs">
                  {{ index.label }}
                </span>
              </td>
              <td>
                <div class="flex flex-wrap gap-1">
                  <span
                    v-for="prop in index.properties"
                    :key="prop"
                    class="px-2 py-0.5 bg-bg-tertiary rounded text-xs"
                  >
                    {{ prop }}
                  </span>
                </div>
              </td>
              <td class="text-sm text-text-secondary">{{ index.type }}</td>
              <td>
                <span :class="['px-2 py-0.5 rounded text-xs', getStateClasses(index.state)]">
                  {{ index.state }}
                </span>
              </td>
              <td>
                <button
                  @click="dropIndex(index.name)"
                  class="p-1 text-text-muted hover:text-error"
                  title="Drop index"
                >
                  <i class="fas fa-trash"></i>
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-else-if="!isLoading" class="text-center text-text-muted py-8">
        <i class="fas fa-list-ol text-2xl mb-2"></i>
        <p>No indexes found</p>
      </div>
    </div>

    <!-- Constraints Section -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <i class="fas fa-lock text-warning"></i>
          Constraints
        </h3>
      </div>

      <div v-if="constraints.length > 0" class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Label</th>
              <th>Properties</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="constraint in constraints" :key="constraint.name">
              <td class="font-mono text-sm">{{ constraint.name }}</td>
              <td class="text-sm">{{ constraint.type }}</td>
              <td>
                <span class="px-2 py-0.5 bg-info/20 text-info rounded text-xs">
                  {{ constraint.label }}
                </span>
              </td>
              <td>
                <div class="flex flex-wrap gap-1">
                  <span
                    v-for="prop in constraint.properties"
                    :key="prop"
                    class="px-2 py-0.5 bg-bg-tertiary rounded text-xs"
                  >
                    {{ prop }}
                  </span>
                </div>
              </td>
              <td>
                <button
                  @click="dropConstraint(constraint.name)"
                  class="p-1 text-text-muted hover:text-error"
                  title="Drop constraint"
                >
                  <i class="fas fa-trash"></i>
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-else class="text-center text-text-muted py-8">
        <i class="fas fa-lock text-2xl mb-2"></i>
        <p>No constraints found</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue';
import { useServersStore } from '@/stores/servers';
import { useNotificationsStore } from '@/stores/notifications';

interface IndexInfo {
  name: string;
  label: string;
  properties: string[];
  type: string;
  state: string;
}

interface ConstraintInfo {
  name: string;
  type: string;
  label: string;
  properties: string[];
}

const serversStore = useServersStore();
const notifications = useNotificationsStore();

const indexes = ref<IndexInfo[]>([]);
const constraints = ref<ConstraintInfo[]>([]);
const labels = ref<string[]>([]);
const isLoading = ref(false);
const isCreating = ref(false);

const newIndex = reactive({
  label: '',
  property: '',
  type: 'btree',
});

async function refreshIndexes(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  isLoading.value = true;

  try {
    // Get indexes
    const indexResponse = await client.executeCypher('SHOW INDEXES');
    if (indexResponse.success && indexResponse.data) {
      indexes.value = indexResponse.data.rows.map((row: any) => ({
        name: row.name,
        label: row.labelsOrTypes?.[0] || '',
        properties: row.properties || [],
        type: row.type || 'BTREE',
        state: row.state || 'ONLINE',
      }));
    }

    // Get constraints
    const constraintResponse = await client.executeCypher('SHOW CONSTRAINTS');
    if (constraintResponse.success && constraintResponse.data) {
      constraints.value = constraintResponse.data.rows.map((row: any) => ({
        name: row.name,
        type: row.type || 'UNIQUENESS',
        label: row.labelsOrTypes?.[0] || '',
        properties: row.properties || [],
      }));
    }

    // Get labels
    const labelsResponse = await client.getLabels();
    if (labelsResponse.success && labelsResponse.data) {
      labels.value = labelsResponse.data.map(l => l.name);
    }
  } catch (error) {
    console.error('Failed to load indexes:', error);
  } finally {
    isLoading.value = false;
  }
}

async function createIndex(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  isCreating.value = true;

  try {
    const query = `CREATE INDEX FOR (n:${newIndex.label}) ON (n.${newIndex.property})`;
    const response = await client.executeCypher(query);

    if (response.success) {
      notifications.success('Index created', `Created index on ${newIndex.label}.${newIndex.property}`);
      newIndex.label = '';
      newIndex.property = '';
      refreshIndexes();
    } else {
      notifications.error('Failed to create index', response.error || 'Unknown error');
    }
  } catch (error: any) {
    notifications.error('Error', error.message);
  } finally {
    isCreating.value = false;
  }
}

async function dropIndex(name: string): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  if (!confirm(`Are you sure you want to drop index "${name}"?`)) return;

  try {
    const response = await client.executeCypher(`DROP INDEX ${name}`);
    if (response.success) {
      notifications.success('Index dropped', `Dropped index ${name}`);
      refreshIndexes();
    } else {
      notifications.error('Failed to drop index', response.error || 'Unknown error');
    }
  } catch (error: any) {
    notifications.error('Error', error.message);
  }
}

async function dropConstraint(name: string): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  if (!confirm(`Are you sure you want to drop constraint "${name}"?`)) return;

  try {
    const response = await client.executeCypher(`DROP CONSTRAINT ${name}`);
    if (response.success) {
      notifications.success('Constraint dropped', `Dropped constraint ${name}`);
      refreshIndexes();
    } else {
      notifications.error('Failed to drop constraint', response.error || 'Unknown error');
    }
  } catch (error: any) {
    notifications.error('Error', error.message);
  }
}

function getStateClasses(state: string): string {
  switch (state.toLowerCase()) {
    case 'online':
      return 'bg-success/20 text-success';
    case 'populating':
      return 'bg-warning/20 text-warning';
    case 'failed':
      return 'bg-error/20 text-error';
    default:
      return 'bg-bg-tertiary text-text-secondary';
  }
}

onMounted(() => {
  refreshIndexes();
});
</script>
