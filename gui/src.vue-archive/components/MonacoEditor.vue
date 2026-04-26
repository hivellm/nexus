<template>
  <div ref="editorContainer" class="monaco-editor-wrapper"></div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue';
import * as monaco from 'monaco-editor';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    language?: string;
    theme?: 'vs-dark' | 'vs' | 'hc-black';
    readOnly?: boolean;
    minimap?: boolean;
    lineNumbers?: 'on' | 'off' | 'relative';
  }>(),
  {
    language: 'cypher',
    theme: 'vs-dark',
    readOnly: false,
    minimap: false,
    lineNumbers: 'on',
  }
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'execute'): void;
}>();

const editorContainer = ref<HTMLDivElement | null>(null);
let editor: monaco.editor.IStandaloneCodeEditor | null = null;

// Register Cypher language
function registerCypherLanguage(): void {
  // Check if already registered
  const languages = monaco.languages.getLanguages();
  if (languages.some((lang) => lang.id === 'cypher')) {
    return;
  }

  monaco.languages.register({ id: 'cypher' });

  monaco.languages.setMonarchTokensProvider('cypher', {
    defaultToken: '',
    tokenPostfix: '.cypher',
    ignoreCase: true,

    keywords: [
      'MATCH',
      'OPTIONAL',
      'WHERE',
      'RETURN',
      'CREATE',
      'DELETE',
      'DETACH',
      'SET',
      'REMOVE',
      'MERGE',
      'ON',
      'WITH',
      'AS',
      'UNWIND',
      'UNION',
      'ALL',
      'CALL',
      'YIELD',
      'ORDER',
      'BY',
      'ASC',
      'DESC',
      'ASCENDING',
      'DESCENDING',
      'SKIP',
      'LIMIT',
      'DISTINCT',
      'AND',
      'OR',
      'NOT',
      'XOR',
      'IN',
      'STARTS',
      'ENDS',
      'CONTAINS',
      'IS',
      'NULL',
      'TRUE',
      'FALSE',
      'COUNT',
      'COLLECT',
      'CASE',
      'WHEN',
      'THEN',
      'ELSE',
      'END',
      'EXISTS',
      'FOREACH',
      'LOAD',
      'CSV',
      'FROM',
      'HEADERS',
      'INDEX',
      'CONSTRAINT',
      'UNIQUE',
      'DROP',
      'USING',
      'PERIODIC',
      'COMMIT',
      'EXPLAIN',
      'PROFILE',
    ],

    builtinFunctions: [
      // Aggregation
      'avg',
      'collect',
      'count',
      'max',
      'min',
      'percentileCont',
      'percentileDisc',
      'stDev',
      'stDevP',
      'sum',
      // Scalar
      'coalesce',
      'endNode',
      'head',
      'id',
      'elementId',
      'last',
      'length',
      'size',
      'properties',
      'randomUUID',
      'startNode',
      'timestamp',
      'toBoolean',
      'toFloat',
      'toInteger',
      'type',
      'labels',
      'keys',
      'nodes',
      'relationships',
      'range',
      'tail',
      'reverse',
      // String
      'left',
      'lTrim',
      'replace',
      'reverse',
      'right',
      'rTrim',
      'split',
      'substring',
      'toLower',
      'toString',
      'toUpper',
      'trim',
      // Math
      'abs',
      'ceil',
      'floor',
      'rand',
      'round',
      'sign',
      'sqrt',
      'log',
      'log10',
      'exp',
      'e',
      'pi',
      'sin',
      'cos',
      'tan',
      'asin',
      'acos',
      'atan',
      'atan2',
      'degrees',
      'radians',
      // Temporal
      'date',
      'datetime',
      'time',
      'localtime',
      'localdatetime',
      'duration',
      // Spatial
      'point',
      'distance',
    ],

    operators: [
      '=',
      '>',
      '<',
      '!',
      '~',
      '?',
      ':',
      '==',
      '<=',
      '>=',
      '!=',
      '<>',
      '=~',
      '+',
      '-',
      '*',
      '/',
      '%',
      '^',
      '|',
      '&',
    ],

    symbols: /[=><!~?:&|+\-*\/\^%]+/,

    escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

    tokenizer: {
      root: [
        // Comments
        [/\/\/.*$/, 'comment'],
        [/\/\*/, 'comment', '@comment'],

        // Strings
        [/"([^"\\]|\\.)*$/, 'string.invalid'],
        [/'([^'\\]|\\.)*$/, 'string.invalid'],
        [/"/, 'string', '@string_double'],
        [/'/, 'string', '@string_single'],

        // Numbers
        [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
        [/\d+/, 'number'],

        // Parameters
        [/\$[a-zA-Z_][a-zA-Z0-9_]*/, 'variable.parameter'],

        // Labels and relationship types
        [/:[A-Z][a-zA-Z0-9_]*/, 'type.identifier'],

        // Properties
        [/\.[a-zA-Z_][a-zA-Z0-9_]*/, 'variable.property'],

        // Identifiers
        [
          /[a-zA-Z_][a-zA-Z0-9_]*/,
          {
            cases: {
              '@keywords': 'keyword',
              '@builtinFunctions': 'predefined',
              '@default': 'identifier',
            },
          },
        ],

        // Delimiters and operators
        [/[{}()\[\]]/, '@brackets'],
        [/[<>](?!@symbols)/, '@brackets'],
        [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],

        // Delimiter
        [/[;,.]/, 'delimiter'],

        // Whitespace
        [/\s+/, 'white'],
      ],

      comment: [
        [/[^\/*]+/, 'comment'],
        [/\*\//, 'comment', '@pop'],
        [/[\/*]/, 'comment'],
      ],

      string_double: [
        [/[^\\"]+/, 'string'],
        [/@escapes/, 'string.escape'],
        [/\\./, 'string.escape.invalid'],
        [/"/, 'string', '@pop'],
      ],

      string_single: [
        [/[^\\']+/, 'string'],
        [/@escapes/, 'string.escape'],
        [/\\./, 'string.escape.invalid'],
        [/'/, 'string', '@pop'],
      ],
    },
  });

  // Cypher auto-completion
  monaco.languages.registerCompletionItemProvider('cypher', {
    provideCompletionItems: (model, position) => {
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };

      const keywords = [
        'MATCH',
        'WHERE',
        'RETURN',
        'CREATE',
        'DELETE',
        'DETACH DELETE',
        'SET',
        'REMOVE',
        'MERGE',
        'WITH',
        'UNWIND',
        'UNION',
        'ORDER BY',
        'SKIP',
        'LIMIT',
        'OPTIONAL MATCH',
        'CALL',
        'YIELD',
        'FOREACH',
        'LOAD CSV',
        'AS',
        'DISTINCT',
      ];

      const functions = [
        'count',
        'collect',
        'sum',
        'avg',
        'min',
        'max',
        'size',
        'length',
        'type',
        'id',
        'labels',
        'keys',
        'properties',
        'nodes',
        'relationships',
        'head',
        'last',
        'tail',
        'range',
        'coalesce',
        'toInteger',
        'toFloat',
        'toString',
        'toBoolean',
        'toLower',
        'toUpper',
        'trim',
        'substring',
        'replace',
        'split',
        'reverse',
        'abs',
        'ceil',
        'floor',
        'round',
        'sqrt',
        'rand',
        'timestamp',
        'date',
        'datetime',
        'point',
        'distance',
      ];

      const suggestions: monaco.languages.CompletionItem[] = [
        ...keywords.map((kw) => ({
          label: kw,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: kw,
          range,
        })),
        ...functions.map((fn) => ({
          label: fn,
          kind: monaco.languages.CompletionItemKind.Function,
          insertText: fn + '($0)',
          insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
          range,
        })),
      ];

      return { suggestions };
    },
  });
}

function initEditor(): void {
  if (!editorContainer.value) return;

  registerCypherLanguage();

  // Define custom dark theme
  monaco.editor.defineTheme('nexus-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '569CD6', fontStyle: 'bold' },
      { token: 'predefined', foreground: 'DCDCAA' },
      { token: 'type.identifier', foreground: '4EC9B0' },
      { token: 'variable.parameter', foreground: '9CDCFE' },
      { token: 'variable.property', foreground: '9CDCFE' },
      { token: 'string', foreground: 'CE9178' },
      { token: 'number', foreground: 'B5CEA8' },
      { token: 'comment', foreground: '6A9955' },
      { token: 'operator', foreground: 'D4D4D4' },
    ],
    colors: {
      'editor.background': '#1E1E2E',
      'editor.foreground': '#CDD6F4',
      'editor.lineHighlightBackground': '#313244',
      'editorCursor.foreground': '#F5E0DC',
      'editor.selectionBackground': '#45475A',
      'editorLineNumber.foreground': '#6C7086',
      'editorLineNumber.activeForeground': '#CDD6F4',
    },
  });

  monaco.editor.defineTheme('nexus-light', {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '0000FF', fontStyle: 'bold' },
      { token: 'predefined', foreground: '795E26' },
      { token: 'type.identifier', foreground: '267F99' },
      { token: 'variable.parameter', foreground: '001080' },
      { token: 'variable.property', foreground: '001080' },
      { token: 'string', foreground: 'A31515' },
      { token: 'number', foreground: '098658' },
      { token: 'comment', foreground: '008000' },
    ],
    colors: {
      'editor.background': '#FFFFFF',
      'editor.foreground': '#000000',
    },
  });

  editor = monaco.editor.create(editorContainer.value, {
    value: props.modelValue,
    language: props.language,
    theme: props.theme === 'vs-dark' ? 'nexus-dark' : 'nexus-light',
    readOnly: props.readOnly,
    minimap: { enabled: props.minimap },
    lineNumbers: props.lineNumbers,
    automaticLayout: true,
    fontSize: 14,
    fontFamily: "'JetBrains Mono', 'Fira Code', 'Consolas', monospace",
    tabSize: 2,
    scrollBeyondLastLine: false,
    wordWrap: 'on',
    padding: { top: 12, bottom: 12 },
    renderLineHighlight: 'all',
    cursorBlinking: 'smooth',
    smoothScrolling: true,
    contextmenu: true,
    suggest: {
      showKeywords: true,
      showFunctions: true,
    },
  });

  // Listen for content changes
  editor.onDidChangeModelContent(() => {
    const value = editor?.getValue() || '';
    emit('update:modelValue', value);
  });

  // Add Ctrl+Enter keybinding
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => {
    emit('execute');
  });
}

// Watch for external value changes
watch(
  () => props.modelValue,
  (newValue) => {
    if (editor && editor.getValue() !== newValue) {
      editor.setValue(newValue);
    }
  }
);

// Watch for theme changes
watch(
  () => props.theme,
  (newTheme) => {
    if (editor) {
      monaco.editor.setTheme(newTheme === 'vs-dark' ? 'nexus-dark' : 'nexus-light');
    }
  }
);

onMounted(() => {
  initEditor();
});

onBeforeUnmount(() => {
  editor?.dispose();
});

// Expose methods
defineExpose({
  focus: () => editor?.focus(),
  getValue: () => editor?.getValue() || '',
  setValue: (value: string) => editor?.setValue(value),
});
</script>

<style scoped>
.monaco-editor-wrapper {
  width: 100%;
  height: 100%;
  min-height: 120px;
  border-radius: 0.5rem;
  overflow: hidden;
}
</style>
