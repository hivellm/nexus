<template>
  <div class="card">
    <div class="flex items-center justify-between">
      <div>
        <div class="text-sm text-text-secondary mb-1">{{ title }}</div>
        <div class="text-2xl font-semibold text-text-primary">
          {{ formattedValue }}
        </div>
        <div v-if="change !== undefined" class="flex items-center gap-1 mt-1 text-xs">
          <i :class="['fas', changeIcon, changeColor]"></i>
          <span :class="changeColor">{{ Math.abs(change) }}%</span>
          <span class="text-text-muted">vs last period</span>
        </div>
      </div>
      <div :class="['w-12 h-12 rounded-lg flex items-center justify-center bg-bg-tertiary', color]">
        <i :class="[icon, 'text-xl']"></i>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{
  title: string;
  value: number;
  icon: string;
  color?: string;
  change?: number;
}>();

const formattedValue = computed(() => {
  if (props.value >= 1000000) {
    return (props.value / 1000000).toFixed(1) + 'M';
  }
  if (props.value >= 1000) {
    return (props.value / 1000).toFixed(1) + 'K';
  }
  return props.value.toLocaleString();
});

const changeIcon = computed(() => {
  if (props.change === undefined) return '';
  return props.change >= 0 ? 'fa-arrow-up' : 'fa-arrow-down';
});

const changeColor = computed(() => {
  if (props.change === undefined) return '';
  return props.change >= 0 ? 'text-success' : 'text-error';
});
</script>
