<template>
  <div class="flex h-screen bg-white dark:bg-neutral-950 text-neutral-900 dark:text-neutral-50">
    <!-- Sidebar - Hidden on mobile, shown on desktop -->
    <aside class="hidden lg:flex w-64 h-full bg-white dark:bg-neutral-900 border-r border-neutral-200 dark:border-neutral-800/50 flex-col">
      <!-- Logo/Header -->
      <div class="h-16 flex items-center justify-between px-6 border-b border-neutral-200 dark:border-neutral-800">
        <div>
          <h1 class="text-xl font-semibold text-neutral-900 dark:text-white leading-none">Nexus</h1>
          <p class="text-xs text-neutral-500 dark:text-neutral-400 mt-1 leading-none">Graph Database</p>
        </div>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 overflow-y-auto p-4">
        <ul class="space-y-1">
          <li v-for="item in menuItems" :key="item.path">
            <router-link
              :to="item.path"
              :class="[
                'flex items-center px-3 py-2 rounded-lg text-sm font-medium transition-colors',
                isActive(item.path)
                  ? 'bg-neutral-100 dark:bg-neutral-800 text-neutral-900 dark:text-neutral-100'
                  : 'text-neutral-700 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800/50'
              ]"
            >
              <span class="ml-3">{{ item.label }}</span>
            </router-link>
          </li>
        </ul>
      </nav>

      <!-- Theme Toggle -->
      <div class="p-4 border-t border-neutral-200 dark:border-neutral-800">
        <button
          @click="toggleTheme"
          class="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm font-medium text-neutral-700 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors"
        >
          <span>Theme</span>
          <svg v-if="isDarkMode" class="w-4 h-4 text-neutral-600 dark:text-neutral-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
          </svg>
          <svg v-else class="w-4 h-4 text-neutral-600 dark:text-neutral-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
          </svg>
        </button>
      </div>
    </aside>

    <!-- Mobile Sidebar Overlay -->
    <div v-if="sidebarOpen" class="fixed inset-0 bg-black/50 z-40 lg:hidden" @click="sidebarOpen = false" />

    <!-- Mobile Sidebar -->
    <aside v-if="sidebarOpen" class="fixed inset-y-0 left-0 z-50 w-64 bg-white dark:bg-neutral-900 border-r border-neutral-200 dark:border-neutral-800/50 flex flex-col lg:hidden">
      <!-- Logo/Header -->
      <div class="h-16 flex items-center justify-between px-6 border-b border-neutral-200 dark:border-neutral-800">
        <div>
          <h1 class="text-xl font-semibold text-neutral-900 dark:text-white leading-none">Nexus</h1>
          <p class="text-xs text-neutral-500 dark:text-neutral-400 mt-1 leading-none">Graph Database</p>
        </div>
        <button @click="sidebarOpen = false" class="lg:hidden p-2 text-neutral-500 hover:text-neutral-700 dark:hover:text-neutral-300">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
      <nav class="flex-1 overflow-y-auto p-4">
        <ul class="space-y-1">
          <li v-for="item in menuItems" :key="item.path">
            <router-link
              :to="item.path"
              @click="sidebarOpen = false"
              :class="[
                'flex items-center px-3 py-2 rounded-lg text-sm font-medium transition-colors',
                isActive(item.path)
                  ? 'bg-neutral-100 dark:bg-neutral-800 text-neutral-900 dark:text-neutral-100'
                  : 'text-neutral-700 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-neutral-800/50'
              ]"
            >
              <span class="ml-3">{{ item.label }}</span>
            </router-link>
          </li>
        </ul>
      </nav>
    </aside>

    <!-- Main Content Area -->
    <div class="flex-1 flex flex-col overflow-hidden min-w-0">
      <!-- Header -->
      <header class="h-16 bg-white dark:bg-neutral-900 border-b border-neutral-200 dark:border-neutral-800/50 flex items-center justify-between px-3 sm:px-4 md:px-6">
        <div class="flex items-center gap-3">
          <!-- Mobile Menu Button -->
          <button
            @click="sidebarOpen = !sidebarOpen"
            class="lg:hidden p-2 text-neutral-500 hover:text-neutral-700 dark:hover:text-neutral-300 rounded-lg hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
            </svg>
          </button>
          <h2 class="text-base sm:text-lg font-semibold text-neutral-900 dark:text-white leading-none">
            {{ pageTitle }}
          </h2>
        </div>
        <div class="flex items-center gap-4">
          <!-- Add header actions here -->
        </div>
      </header>

      <!-- Main Content -->
      <main class="flex-1 overflow-y-auto bg-white dark:bg-neutral-950">
        <div class="max-w-7xl mx-auto px-3 sm:px-4 md:px-6 lg:px-8 py-4 sm:py-6 lg:py-8">
          <router-view />
        </div>
      </main>
    </div>

    <!-- Notification Center -->
    <NotificationCenter />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { useThemeStore } from '@/stores/theme';
import NotificationCenter from '@/components/NotificationCenter.vue';

const route = useRoute();
const themeStore = useThemeStore();

const isDarkMode = computed(() => themeStore.theme === 'dark');

const sidebarOpen = ref(false);

const menuItems = [
  { path: '/', label: 'Dashboard' },
  { path: '/query', label: 'Query' },
  { path: '/graph', label: 'Graph' },
  { path: '/schema', label: 'Schema' },
  { path: '/data', label: 'Data' },
  { path: '/indexes', label: 'Indexes' },
  { path: '/vector-search', label: 'Vector Search' },
  { path: '/logs', label: 'Logs' },
  { path: '/config', label: 'Configuration' },
];

const pageTitle = computed(() => {
  const path = route.path;
  if (path === '/' || path === '/dashboard') return 'Dashboard';
  const item = menuItems.find(m => m.path === path);
  return item?.label || 'Nexus';
});

function isActive(path: string): boolean {
  return route.path === path;
}

function toggleTheme(): void {
  themeStore.toggleTheme();
}

onMounted(() => {
  themeStore.loadTheme();
});
</script>
