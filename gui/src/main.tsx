import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from './app/App';
import { bindThemeToHtml } from './stores/layoutStore';
import './styles/tokens.css';
import './styles/globals.css';

// Mirror the persisted theme choice onto <html data-theme="...">
// before the first render so CSS variables in tokens.css resolve
// to the right palette without a flash of unstyled content.
bindThemeToHtml();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('#root element not found in index.html');

createRoot(rootEl).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
