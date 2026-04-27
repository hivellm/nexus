import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from './app/App';
import { bindThemeToHtml } from './stores/layoutStore';
import { registerNexusMonacoThemes } from './styles/monaco-themes';
import './styles/tokens.css';
import './styles/globals.css';

// Mirror the persisted theme choice onto <html data-theme="...">
// before the first render so CSS variables in tokens.css resolve
// to the right palette without a flash of unstyled content.
bindThemeToHtml();

// Prime Monaco with the nexus dark/light themes BEFORE any Editor
// component mounts. The React wrapper otherwise calls
// `monaco.editor.create({ theme: 'nexus-dark' })` while the global
// theme service still has only `vs`/`vs-dark` registered, and
// silently falls back to white `vs`. Fire-and-forget — the loader
// caches its initialised Monaco for subsequent mounts.
void registerNexusMonacoThemes();

// QueryClient defaults — tuned for a desktop GUI talking to a
// single graph server: aggressive retry would mask real outages
// behind exponential backoff, refetch-on-focus would re-poll a
// busy schema query every alt-tab. Per-hook polling intervals
// live in `services/queries.ts`.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => {
        // Network-level errors get one retry; HTTP error responses
        // (NexusApiError carries an HTTP status) surface immediately
        // so the UI can render the error payload without a delay.
        const status = (error as { status?: number } | null)?.status;
        if (typeof status === 'number') return false;
        return failureCount < 1;
      },
      refetchOnWindowFocus: false,
      refetchOnReconnect: true,
      staleTime: 5_000,
      gcTime: 5 * 60_000,
    },
    mutations: {
      retry: 0,
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
