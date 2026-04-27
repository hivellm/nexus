/**
 * Register Monaco's `nexus-dark` / `nexus-light` themes against the
 * Monaco instance owned by `@monaco-editor/react`'s `loader`. Must
 * run before the first <Editor /> mounts — `main.tsx` invokes this
 * once at startup so the editor never paints with the default `vs`
 * (white) theme during the brief window between Monaco load and
 * our `beforeMount` callback.
 *
 * Without this priming, the React wrapper passes the theme name
 * `nexus-dark` into `monaco.editor.create()` while the global
 * theme service still has only `vs` / `vs-dark` registered, so the
 * editor silently falls back to `vs` and shows a white background.
 */
import { loader } from '@monaco-editor/react';

let registered: Promise<void> | null = null;

export function registerNexusMonacoThemes(): Promise<void> {
  if (registered) return registered;
  registered = loader.init().then((monaco) => {
    monaco.editor.defineTheme('nexus-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '5a6169', fontStyle: 'italic' },
        { token: 'keyword', foreground: '00d4ff', fontStyle: 'bold' },
        { token: 'string', foreground: '10b981' },
        { token: 'number', foreground: 'a78bfa' },
        { token: 'operator', foreground: 'f59e0b' },
        { token: 'identifier', foreground: 'e6ebf2' },
      ],
      colors: {
        'editor.background': '#0e1114',
        'editor.foreground': '#e6ebf2',
        'editorLineNumber.foreground': '#3b4049',
        'editorLineNumber.activeForeground': '#8a9199',
        'editor.lineHighlightBackground': '#141820',
        'editor.selectionBackground': '#1f3a4d',
        'editorCursor.foreground': '#00d4ff',
        'editorIndentGuide.background1': '#1a1f28',
      },
    });

    monaco.editor.defineTheme('nexus-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '64748b', fontStyle: 'italic' },
        { token: 'keyword', foreground: '0284c7', fontStyle: 'bold' },
        { token: 'string', foreground: '047857' },
        { token: 'number', foreground: '7c3aed' },
        { token: 'operator', foreground: 'd97706' },
      ],
      colors: {
        'editor.background': '#ffffff',
        'editor.foreground': '#0f172a',
        'editor.lineHighlightBackground': '#f1f5f9',
      },
    });

    // Apply the active nexus theme as the global default so the
    // very first paint of any editor uses it instead of `vs`.
    const html = document.documentElement.getAttribute('data-theme');
    monaco.editor.setTheme(html === 'light' ? 'nexus-light' : 'nexus-dark');
  });
  return registered;
}
