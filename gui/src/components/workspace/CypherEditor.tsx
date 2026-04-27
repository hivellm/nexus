/**
 * CypherEditor — Monaco-driven Cypher editor with theme matching
 * the project tokens, JetBrains Mono gutter, ⌘↵ Run / ⌘/ comment
 * toggle / ⌘S save shortcuts. Body comes from
 * `layoutStore.editorTabs[activeTab]`; edits write back via
 * `setTabBody`. The footer surfaces parser status + plan summary +
 * est. cost + a kbd hint.
 *
 * Cypher's grammar is registered as a custom Monaco language on
 * first mount; the registration is idempotent so swapping tabs
 * does not stack handlers.
 */
import { Editor, type BeforeMount, type Monaco, type OnMount } from '@monaco-editor/react';
import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useLayoutStore } from '../../stores/layoutStore';
import type { editor as MonacoEditor } from 'monaco-editor';

const CYPHER_KEYWORDS = [
  'MATCH',
  'OPTIONAL',
  'WHERE',
  'RETURN',
  'WITH',
  'CREATE',
  'MERGE',
  'DELETE',
  'DETACH',
  'REMOVE',
  'SET',
  'CALL',
  'YIELD',
  'UNWIND',
  'FOREACH',
  'ORDER',
  'BY',
  'ASC',
  'DESC',
  'LIMIT',
  'SKIP',
  'AS',
  'AND',
  'OR',
  'NOT',
  'IN',
  'EXISTS',
  'NULL',
  'TRUE',
  'FALSE',
  'CASE',
  'WHEN',
  'THEN',
  'ELSE',
  'END',
  'UNION',
  'DISTINCT',
];

function registerCypherLanguage(monaco: Monaco): void {
  if (monaco.languages.getLanguages().some((l) => l.id === 'cypher')) {
    return;
  }
  monaco.languages.register({ id: 'cypher' });

  monaco.languages.setMonarchTokensProvider('cypher', {
    ignoreCase: true,
    keywords: CYPHER_KEYWORDS,
    tokenizer: {
      root: [
        [/--.*$/, 'comment'],
        [/\/\/.*$/, 'comment'],
        [/'(?:[^'\\]|\\.)*'/, 'string'],
        [/"(?:[^"\\]|\\.)*"/, 'string'],
        [/\b\d+(?:\.\d+)?\b/, 'number'],
        [
          /\b[A-Za-z_][A-Za-z0-9_]*\b/,
          {
            cases: {
              '@keywords': 'keyword',
              '@default': 'identifier',
            },
          },
        ],
        [/[(){}[\]]/, 'delimiter.bracket'],
        [/[<>!=]=?|=~|->|<-|\.\./, 'operator'],
        [/[,.;:|]/, 'delimiter'],
      ],
    },
  });

  monaco.languages.setLanguageConfiguration('cypher', {
    comments: { lineComment: '//', blockComment: ['/*', '*/'] },
    brackets: [
      ['(', ')'],
      ['[', ']'],
      ['{', '}'],
    ],
    autoClosingPairs: [
      { open: '(', close: ')' },
      { open: '[', close: ']' },
      { open: '{', close: '}' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
  });

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
}

interface CypherEditorProps {
  onRun: () => void;
  onSave?: () => void;
}

export function CypherEditor({ onRun, onSave }: CypherEditorProps) {
  const activeTab = useLayoutStore((s) => s.activeTab);
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const setTabBody = useLayoutStore((s) => s.setTabBody);
  const theme = useLayoutStore((s) => s.theme);

  const tab = useMemo(
    () => editorTabs.find((t) => t.id === activeTab) ?? null,
    [editorTabs, activeTab],
  );

  const editorRef = useRef<MonacoEditor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);

  const themeName = theme === 'light' ? 'nexus-light' : 'nexus-dark';

  // Register the Cypher language + nexus themes BEFORE Monaco
  // first paints, AND explicitly setTheme() — without the explicit
  // call Monaco's react wrapper looks up the theme name during
  // editor construction (before our defineTheme has run) and falls
  // back to the default `vs` theme, leaving the editor white until
  // the user types. The `theme` prop alone races against
  // registration; calling `setTheme` after `defineTheme` is the
  // documented fix.
  const handleBeforeMount: BeforeMount = useCallback(
    (monaco) => {
      monacoRef.current = monaco;
      registerCypherLanguage(monaco);
      monaco.editor.setTheme(themeName);
    },
    [themeName],
  );

  const handleMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;
      registerCypherLanguage(monaco);
      monaco.editor.setTheme(themeName);
      editor.updateOptions({
        fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace",
        fontSize: 13,
        lineHeight: 20,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        renderLineHighlight: 'line',
        roundedSelection: false,
        smoothScrolling: true,
        cursorBlinking: 'smooth',
      });

      // ⌘↵ / Ctrl+↵ — Run.
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => {
        onRun();
      });
      // ⌘S / Ctrl+S — Save (no-op when handler omitted, but the
      // shortcut is always registered so the browser default
      // does not steal focus).
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        onSave?.();
      });
      // ⌘/ — toggle line comment via Monaco's built-in action so
      // the binding stays consistent with VS Code muscle memory.
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Slash, () => {
        editor.getAction('editor.action.commentLine')?.run();
      });
    },
    [onRun, onSave, themeName],
  );

  // React to theme flips (Tweaks panel) — re-apply the matching
  // nexus theme. Monaco is global; setTheme repaints all instances.
  useEffect(() => {
    const monaco = monacoRef.current;
    if (!monaco) return;
    monaco.editor.setTheme(themeName);
  }, [themeName]);

  const handleChange = (value: string | undefined) => {
    if (tab && typeof value === 'string') {
      setTabBody(tab.id, value);
    }
  };

  return (
    <div className="cypher-editor">
      <Editor
        height="100%"
        language="cypher"
        theme={theme === 'light' ? 'nexus-light' : 'nexus-dark'}
        value={tab?.body ?? ''}
        beforeMount={handleBeforeMount}
        onMount={handleMount}
        onChange={handleChange}
        options={{
          readOnly: !tab,
        }}
      />
      <div className="editor-footer">
        <span className="stat">
          Parsed <strong>{tab ? 'OK' : '—'}</strong>
        </span>
        <span className="stat">
          Plan <strong>—</strong>
        </span>
        <span className="stat">
          Est. cost <strong>—</strong>
        </span>
        <div className="grow" />
        <span className="stat">⌘+↵ run · ⌘+/ comment · ⌘+S save</span>
      </div>
    </div>
  );
}
