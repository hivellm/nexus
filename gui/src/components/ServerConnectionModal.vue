<template>
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="$emit('close')">
    <div class="bg-bg-elevated rounded-lg p-6 w-full max-w-md">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold">
          {{ editingServer ? 'Edit Connection' : 'New Connection' }}
        </h3>
        <button @click="$emit('close')" class="text-text-muted hover:text-text-primary">
          <i class="fas fa-times"></i>
        </button>
      </div>

      <form @submit.prevent="save" class="space-y-4">
        <div>
          <label class="block text-sm text-text-secondary mb-1">Connection Name</label>
          <input
            v-model="form.name"
            type="text"
            class="input"
            placeholder="My Nexus Server"
            required
          />
        </div>

        <div>
          <label class="block text-sm text-text-secondary mb-1">Host</label>
          <input
            v-model="form.host"
            type="text"
            class="input"
            placeholder="localhost"
            required
          />
        </div>

        <div>
          <label class="block text-sm text-text-secondary mb-1">Port</label>
          <input
            v-model.number="form.port"
            type="number"
            class="input"
            placeholder="7687"
            required
          />
        </div>

        <div class="flex items-center gap-2">
          <input
            v-model="form.ssl"
            type="checkbox"
            id="ssl-checkbox"
            class="rounded bg-bg-tertiary border-border text-accent focus:ring-accent"
          />
          <label for="ssl-checkbox" class="text-sm text-text-secondary">Use SSL/TLS</label>
        </div>

        <div v-if="testResult" :class="['p-3 rounded-lg text-sm', testResult.success ? 'bg-success/10 text-success' : 'bg-error/10 text-error']">
          <div class="flex items-center gap-2">
            <i :class="['fas', testResult.success ? 'fa-check-circle' : 'fa-exclamation-circle']"></i>
            <span>{{ testResult.message }}</span>
          </div>
        </div>

        <div class="flex items-center gap-2 pt-2">
          <button
            type="button"
            @click="testConnection"
            class="btn btn-secondary flex-1"
            :disabled="isTesting"
          >
            <i :class="['fas mr-1', isTesting ? 'fa-spinner fa-spin' : 'fa-plug']"></i>
            Test
          </button>
          <button type="submit" class="btn btn-primary flex-1" :disabled="isSaving">
            <i :class="['fas mr-1', isSaving ? 'fa-spinner fa-spin' : 'fa-save']"></i>
            Save
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue';
import { useServersStore } from '@/stores/servers';
import type { Server } from '@/types';

const props = defineProps<{
  editingServer?: Server | null;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'saved'): void;
}>();

const serversStore = useServersStore();

const form = reactive({
  name: '',
  host: 'localhost',
  port: 7687,
  ssl: false,
});

const isTesting = ref(false);
const isSaving = ref(false);
const testResult = ref<{ success: boolean; message: string } | null>(null);

onMounted(() => {
  if (props.editingServer) {
    form.name = props.editingServer.name;
    form.host = props.editingServer.host || 'localhost';
    form.port = props.editingServer.port || 7687;
    form.ssl = props.editingServer.ssl || false;
  }
});

async function testConnection(): Promise<void> {
  isTesting.value = true;
  testResult.value = null;

  try {
    const client = serversStore.createClient({
      id: 'test',
      name: form.name,
      host: form.host,
      port: form.port,
      ssl: form.ssl,
      connected: false,
      status: 'disconnected',
    });

    const result = await client.healthCheck();

    if (result.success) {
      testResult.value = {
        success: true,
        message: `Connected! Server is ${result.data?.status || 'healthy'}`,
      };
    } else {
      testResult.value = {
        success: false,
        message: result.error || 'Connection failed',
      };
    }
  } catch (error: any) {
    testResult.value = {
      success: false,
      message: error.message || 'Connection failed',
    };
  } finally {
    isTesting.value = false;
  }
}

async function save(): Promise<void> {
  isSaving.value = true;

  try {
    if (props.editingServer) {
      serversStore.updateServer(props.editingServer.id, {
        name: form.name,
        host: form.host,
        port: form.port,
        ssl: form.ssl,
      });
    } else {
      serversStore.addServer({
        name: form.name,
        host: form.host,
        port: form.port,
        ssl: form.ssl,
      });
    }
    emit('saved');
    emit('close');
  } catch (error: any) {
    testResult.value = {
      success: false,
      message: error.message || 'Failed to save connection',
    };
  } finally {
    isSaving.value = false;
  }
}
</script>
