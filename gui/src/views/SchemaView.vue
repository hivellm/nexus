<template>
  <div class="p-6 space-y-6">
    <!-- Labels Section -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold flex items-center gap-2">
          <i class="fas fa-tag text-info"></i>
          Node Labels
        </h3>
        <button @click="refreshSchema" class="btn btn-secondary text-xs">
          <i :class="['fas mr-1', isLoading ? 'fa-spinner fa-spin' : 'fa-sync']"></i>
          Refresh
        </button>
      </div>

      <div v-if="labels.length > 0" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <div
          v-for="label in labels"
          :key="label.name"
          class="bg-bg-tertiary rounded-lg p-4 cursor-pointer hover:bg-bg-hover transition-colors"
          @click="selectedLabel = label"
        >
          <div class="flex items-center justify-between mb-2">
            <span class="font-medium text-text-primary">{{ label.name }}</span>
            <span class="text-xs text-text-muted">{{ label.count }} nodes</span>
          </div>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="prop in label.properties?.slice(0, 3)"
              :key="prop.name"
              class="px-2 py-0.5 bg-bg-hover rounded text-xs text-text-secondary"
            >
              {{ prop.name }}
            </span>
            <span v-if="(label.properties?.length || 0) > 3" class="text-xs text-text-muted">
              +{{ (label.properties?.length || 0) - 3 }} more
            </span>
          </div>
        </div>
      </div>
      <div v-else class="text-center text-text-muted py-8">
        <i class="fas fa-tag text-2xl mb-2"></i>
        <p>No labels found</p>
      </div>
    </div>

    <!-- Relationship Types Section -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-arrow-right text-success"></i>
        Relationship Types
      </h3>

      <div v-if="relationshipTypes.length > 0" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <div
          v-for="relType in relationshipTypes"
          :key="relType.name"
          class="bg-bg-tertiary rounded-lg p-4"
        >
          <div class="flex items-center justify-between mb-2">
            <span class="font-medium text-text-primary">{{ relType.name }}</span>
            <span class="text-xs text-text-muted">{{ relType.count }} rels</span>
          </div>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="prop in relType.properties?.slice(0, 3)"
              :key="prop.name"
              class="px-2 py-0.5 bg-bg-hover rounded text-xs text-text-secondary"
            >
              {{ prop.name }}
            </span>
          </div>
        </div>
      </div>
      <div v-else class="text-center text-text-muted py-8">
        <i class="fas fa-arrow-right text-2xl mb-2"></i>
        <p>No relationship types found</p>
      </div>
    </div>

    <!-- Property Keys Section -->
    <div class="card">
      <h3 class="text-lg font-semibold mb-4 flex items-center gap-2">
        <i class="fas fa-key text-warning"></i>
        Property Keys
      </h3>

      <div v-if="propertyKeys.length > 0" class="flex flex-wrap gap-2">
        <span
          v-for="key in propertyKeys"
          :key="key"
          class="px-3 py-1 bg-bg-tertiary rounded-full text-sm text-text-secondary"
        >
          {{ key }}
        </span>
      </div>
      <div v-else class="text-center text-text-muted py-8">
        <i class="fas fa-key text-2xl mb-2"></i>
        <p>No property keys found</p>
      </div>
    </div>

    <!-- Label Detail Modal -->
    <div v-if="selectedLabel" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="selectedLabel = null">
      <div class="bg-bg-elevated rounded-lg p-6 w-full max-w-lg max-h-[80vh] overflow-y-auto">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold">{{ selectedLabel.name }}</h3>
          <button @click="selectedLabel = null" class="text-text-muted hover:text-text-primary">
            <i class="fas fa-times"></i>
          </button>
        </div>

        <div class="space-y-4">
          <div>
            <div class="text-sm text-text-muted mb-1">Node Count</div>
            <div class="text-2xl font-semibold">{{ selectedLabel.count }}</div>
          </div>

          <div>
            <div class="text-sm text-text-muted mb-2">Properties</div>
            <table class="table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Type</th>
                  <th>Indexed</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="prop in selectedLabel.properties" :key="prop.name">
                  <td>{{ prop.name }}</td>
                  <td><span class="text-xs text-text-muted">{{ prop.type }}</span></td>
                  <td>
                    <i v-if="prop.indexed" class="fas fa-check text-success"></i>
                    <i v-else class="fas fa-times text-text-muted"></i>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="flex gap-2">
            <button @click="queryLabel(selectedLabel.name)" class="btn btn-primary flex-1">
              <i class="fas fa-search mr-1"></i>
              Query Nodes
            </button>
            <button @click="visualizeLabel(selectedLabel.name)" class="btn btn-secondary flex-1">
              <i class="fas fa-project-diagram mr-1"></i>
              Visualize
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import { useQueryStore } from '@/stores/query';
import type { LabelInfo, RelationshipTypeInfo } from '@/types';

const router = useRouter();
const serversStore = useServersStore();
const queryStore = useQueryStore();

const labels = ref<LabelInfo[]>([]);
const relationshipTypes = ref<RelationshipTypeInfo[]>([]);
const propertyKeys = ref<string[]>([]);
const isLoading = ref(false);
const selectedLabel = ref<LabelInfo | null>(null);

async function refreshSchema(): Promise<void> {
  const client = serversStore.activeClient;
  if (!client) return;

  isLoading.value = true;

  try {
    const [labelsRes, relTypesRes, propKeysRes] = await Promise.all([
      client.getLabels(),
      client.getRelationshipTypes(),
      client.getPropertyKeys(),
    ]);

    if (labelsRes.success && labelsRes.data) {
      labels.value = labelsRes.data;
    }
    if (relTypesRes.success && relTypesRes.data) {
      relationshipTypes.value = relTypesRes.data;
    }
    if (propKeysRes.success && propKeysRes.data) {
      propertyKeys.value = propKeysRes.data;
    }
  } catch (error) {
    console.error('Failed to load schema:', error);
  } finally {
    isLoading.value = false;
  }
}

function queryLabel(label: string): void {
  queryStore.setQuery(`MATCH (n:${label}) RETURN n LIMIT 100`);
  router.push('/query');
  selectedLabel.value = null;
}

function visualizeLabel(label: string): void {
  router.push({ path: '/graph', query: { label } });
  selectedLabel.value = null;
}

onMounted(() => {
  refreshSchema();
});
</script>
