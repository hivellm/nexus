import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { Notification } from '@/types';

export const useNotificationsStore = defineStore('notifications', () => {
  const notifications = ref<Notification[]>([]);

  const unreadCount = computed(() => notifications.value.filter((n) => !n.read).length);

  function addNotification(
    type: Notification['type'],
    title: string,
    message: string
  ): string {
    const id = `notif-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const notification: Notification = {
      id,
      type,
      title,
      message,
      timestamp: new Date(),
      read: false,
    };

    notifications.value.unshift(notification);

    // Keep only last 50 notifications
    if (notifications.value.length > 50) {
      notifications.value = notifications.value.slice(0, 50);
    }

    // Auto-remove success notifications after 5 seconds
    if (type === 'success') {
      setTimeout(() => {
        removeNotification(id);
      }, 5000);
    }

    return id;
  }

  function success(title: string, message: string = ''): string {
    return addNotification('success', title, message);
  }

  function error(title: string, message: string = ''): string {
    return addNotification('error', title, message);
  }

  function warning(title: string, message: string = ''): string {
    return addNotification('warning', title, message);
  }

  function info(title: string, message: string = ''): string {
    return addNotification('info', title, message);
  }

  function markAsRead(id: string): void {
    const notification = notifications.value.find((n) => n.id === id);
    if (notification) {
      notification.read = true;
    }
  }

  function markAllAsRead(): void {
    notifications.value.forEach((n) => {
      n.read = true;
    });
  }

  function removeNotification(id: string): void {
    notifications.value = notifications.value.filter((n) => n.id !== id);
  }

  function clearAll(): void {
    notifications.value = [];
  }

  return {
    notifications,
    unreadCount,
    addNotification,
    success,
    error,
    warning,
    info,
    markAsRead,
    markAllAsRead,
    removeNotification,
    remove: removeNotification,
    clearAll,
  };
});
