import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Theme } from '@/types';

const THEME_KEY = 'nexus-desktop-theme';

export const useThemeStore = defineStore('theme', () => {
  const theme = ref<Theme>('dark');

  function loadTheme(): void {
    const saved = localStorage.getItem(THEME_KEY) as Theme | null;
    if (saved && (saved === 'dark' || saved === 'light')) {
      theme.value = saved;
    } else {
      // Check system preference
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      theme.value = prefersDark ? 'dark' : 'light';
    }
    applyTheme();
  }

  function setTheme(newTheme: Theme): void {
    theme.value = newTheme;
    localStorage.setItem(THEME_KEY, newTheme);
    applyTheme();
  }

  function toggleTheme(): void {
    setTheme(theme.value === 'dark' ? 'light' : 'dark');
  }

  function applyTheme(): void {
    document.documentElement.setAttribute('data-theme', theme.value);
  }

  // Watch for system theme changes
  if (typeof window !== 'undefined') {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
      if (!localStorage.getItem(THEME_KEY)) {
        theme.value = e.matches ? 'dark' : 'light';
        applyTheme();
      }
    });
  }

  return {
    theme,
    loadTheme,
    setTheme,
    toggleTheme,
  };
});
