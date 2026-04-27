/**
 * Workspace — orchestrator that wires the editor, the run path,
 * and the four result views. Lives in `app/App.tsx`'s middle grid
 * cell.
 *
 * Run path:
 *   1. Run button (or ⌘↵ inside Monaco) calls `useExecuteCypher`.
 *   2. On success the response is held in component state and the
 *      results pane re-renders (default to Graph view).
 *   3. The query + latency + row count are also pushed onto
 *      `queryHistoryStore` so the Audit panel reflects the run.
 *
 * Mode state defaults to Graph and switches per ResultsTabs click.
 * Selection state lives here too so the inspector + GraphView stay
 * in lock-step without prop-drilling through ResultsTabs.
 */
import { useCallback, useMemo, useState } from 'react';
import { useExecuteCypher } from '../../services/queries';
import { useLayoutStore } from '../../stores/layoutStore';
import { useQueryHistoryStore } from '../../stores/queryHistoryStore';
import { CypherEditor } from './CypherEditor';
import { EditorHead } from './EditorHead';
import { ResultsTabs, type ResultMode } from './ResultsTabs';
import { TableView } from './TableView';
import { JsonView } from './JsonView';
import { PlanView } from './PlanView';
import { GraphView, extractGraph } from './GraphView';
import { GraphControls } from './GraphControls';
import { GraphLegend } from './GraphLegend';
import { NodeInspector } from './NodeInspector';
import type { CypherResponse } from '../../types/api';

const DEFAULT_TAB_BODY = `// Try a query
MATCH (n)
RETURN n
LIMIT 25
`;

export function Workspace() {
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeTab = useLayoutStore((s) => s.activeTab);
  const openTab = useLayoutStore((s) => s.openTab);

  const pushHistory = useQueryHistoryStore((s) => s.push);
  const exec = useExecuteCypher();

  const [mode, setMode] = useState<ResultMode>('graph');
  const [result, setResult] = useState<CypherResponse | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);
  const [zoom, setZoom] = useState(1);
  const [layoutSeed, setLayoutSeed] = useState(0);

  const tab = useMemo(
    () => editorTabs.find((t) => t.id === activeTab) ?? null,
    [editorTabs, activeTab],
  );

  const ensureTab = useCallback(() => {
    if (!tab) {
      const id = `tab-${Date.now().toString(36)}`;
      openTab({ id, title: 'query-1.cypher', body: DEFAULT_TAB_BODY });
    }
  }, [tab, openTab]);

  const handleRun = useCallback(() => {
    ensureTab();
    if (!tab || !tab.body.trim()) return;
    const startedAt = performance.now();
    exec.mutate(
      { query: tab.body },
      {
        onSuccess: (data) => {
          const elapsed = performance.now() - startedAt;
          setResult(data);
          setSelectedNodeId(null);
          pushHistory({
            query: tab.body,
            ms: Math.round(data.execution_time_ms || elapsed),
            rows: data.rows.length,
            ok: true,
          });
        },
        onError: (err) => {
          const elapsed = performance.now() - startedAt;
          pushHistory({
            query: tab.body,
            ms: Math.round(elapsed),
            rows: 0,
            ok: false,
          });
          // Surface the error inline by clearing the previous result so
          // a stale graph does not look fresh; the err itself is
          // surfaced via the `exec.error` branch below.
          setResult(null);
          // Re-throw is not needed; useMutation already stores the
          // error for `exec.error`.
          void err;
        },
      },
    );
  }, [ensureTab, tab, exec, pushHistory]);

  const { nodes, relationships } = useMemo(() => extractGraph(result), [result]);
  const selectedNode = useMemo(
    () => nodes.find((n) => n.id === selectedNodeId) ?? null,
    [nodes, selectedNodeId],
  );

  const ms = result?.execution_time_ms ?? 0;
  const rowCount = result?.rows.length ?? 0;

  return (
    <main className="workspace">
      <EditorHead onRun={handleRun} isRunning={exec.isPending} />
      <div className="workspace-split">
        <CypherEditor onRun={handleRun} />
        <div className="results-pane">
          <ResultsTabs
            mode={mode}
            onMode={setMode}
            rowCount={rowCount}
            nodeCount={nodes.length}
            ms={Math.round(ms)}
            planner={result ? 'heuristic' : undefined}
          />
          {exec.error && (
            <div className="results-error">
              <strong>{exec.error.code ?? 'error'}</strong> {exec.error.message}
            </div>
          )}
          {mode === 'graph' && (
            <div className="graph-pane" key={layoutSeed} style={{ transform: `scale(${zoom})` }}>
              <GraphView
                result={result}
                selectedId={selectedNodeId}
                onSelect={setSelectedNodeId}
              />
              <GraphControls
                onZoomIn={() => setZoom((z) => Math.min(2.5, z + 0.15))}
                onZoomOut={() => setZoom((z) => Math.max(0.5, z - 0.15))}
                onFit={() => setZoom(1)}
                onRefreshLayout={() => setLayoutSeed((s) => s + 1)}
              />
              <GraphLegend nodes={nodes} />
              <NodeInspector
                node={selectedNode}
                relationships={relationships}
                onClose={() => setSelectedNodeId(null)}
              />
            </div>
          )}
          {mode === 'table' && <TableView result={result} />}
          {mode === 'json' && <JsonView result={result} />}
          {mode === 'plan' && <PlanView result={result} />}
        </div>
      </div>
    </main>
  );
}
