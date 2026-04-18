<template>
  <div class="fixed bottom-4 right-4 z-50 space-y-2 w-80">
    <TransitionGroup name="notification">
      <div
        v-for="notification in notifications"
        :key="notification.id"
        :class="[
          'p-4 rounded-lg shadow-lg border',
          getNotificationClasses(notification.type)
        ]"
      >
        <div class="flex items-start gap-3">
          <i :class="['fas', getNotificationIcon(notification.type)]"></i>
          <div class="flex-1 min-w-0">
            <div class="font-medium text-sm">{{ notification.title }}</div>
            <div v-if="notification.message" class="text-xs mt-1 opacity-80">
              {{ notification.message }}
            </div>
          </div>
          <button
            @click="dismiss(notification.id)"
            class="opacity-60 hover:opacity-100 transition-opacity"
          >
            <i class="fas fa-times text-sm"></i>
          </button>
        </div>
      </div>
    </TransitionGroup>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useNotificationsStore } from '@/stores/notifications';

const notificationsStore = useNotificationsStore();

const notifications = computed(() => notificationsStore.notifications);

function dismiss(id: string): void {
  notificationsStore.remove(id);
}

function getNotificationClasses(type: string): string {
  switch (type) {
    case 'success':
      return 'bg-success/10 border-success/20 text-success';
    case 'error':
      return 'bg-error/10 border-error/20 text-error';
    case 'warning':
      return 'bg-warning/10 border-warning/20 text-warning';
    default:
      return 'bg-info/10 border-info/20 text-info';
  }
}

function getNotificationIcon(type: string): string {
  switch (type) {
    case 'success':
      return 'fa-check-circle';
    case 'error':
      return 'fa-exclamation-circle';
    case 'warning':
      return 'fa-exclamation-triangle';
    default:
      return 'fa-info-circle';
  }
}
</script>

<style scoped>
.notification-enter-active,
.notification-leave-active {
  transition: all 0.3s ease;
}

.notification-enter-from {
  opacity: 0;
  transform: translateX(100%);
}

.notification-leave-to {
  opacity: 0;
  transform: translateX(100%);
}

.notification-move {
  transition: transform 0.3s ease;
}
</style>
