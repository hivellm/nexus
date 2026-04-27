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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useApiBase, useExecuteCypher } from '../../services/queries';
import { api } from '../../services/api';
import { sanitizeCypher } from '../../services/cypher';
import { useLayoutStore } from '../../stores/layoutStore';
import {
  selectCurrentConnection,
  useConnectionsStore,
} from '../../stores/connectionsStore';
import { useQueryHistoryStore } from '../../stores/queryHistoryStore';
import { CypherEditor } from './CypherEditor';
import { EditorHead } from './EditorHead';
import { ResultsTabs, type ResultMode } from './ResultsTabs';
import { TableView } from './TableView';
import { JsonView } from './JsonView';
import { PlanView } from './PlanView';
import {
  GraphView,
  extractGraph,
  type GraphRelationship,
  type GraphViewHandle,
} from './GraphView';
import { GraphControls } from './GraphControls';
import { GraphLegend } from './GraphLegend';
import { NodeInspector } from './NodeInspector';
import type { CypherResponse } from '../../types/api';

export function Workspace() {
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeTab = useLayoutStore((s) => s.activeTab);
  const ensureDefaultTab = useLayoutStore((s) => s.ensureDefaultTab);

  const pushHistory = useQueryHistoryStore((s) => s.push);
  const exec = useExecuteCypher();
  const baseUrl = useApiBase();
  const apiKey = useConnectionsStore((s) => selectCurrentConnection(s)?.apiKey);

  const [mode, setMode] = useState<ResultMode>('graph');
  const [result, setResult] = useState<CypherResponse | null>(null);
  const [extraRels, setExtraRels] = useState<GraphRelationship[]>([]);
  const [extraLabels, setExtraLabels] = useState<Record<number, string>>({});
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);
  const graphRef = useRef<GraphViewHandle>(null);

  // Seed a starter tab with the sample query on first mount so the
  // editor is not a blank screen on a fresh / cleared session.
  useEffect(() => {
    ensureDefaultTab();
  }, [ensureDefaultTab]);

  const tab = useMemo(
    () => editorTabs.find((t) => t.id === activeTab) ?? null,
    [editorTabs, activeTab],
  );

  const handleRun = useCallback(() => {
    if (!tab || !tab.body.trim()) return;
    const sanitized = sanitizeCypher(tab.body);
    if (!sanitized) return;
    const startedAt = performance.now();
    exec.mutate(
      { query: sanitized },
      {
        onSuccess: (data) => {
          const elapsed = performance.now() - startedAt;
          setResult(data);
          setExtraRels([]);
          setExtraLabels({});
          setSelectedNodeId(null);
          pushHistory({
            query: sanitized,
            ms: Math.round(data.execution_time_ms || elapsed),
            rows: data.rows.length,
            ok: true,
          });
        },
        onError: (err) => {
          const elapsed = performance.now() - startedAt;
          pushHistory({
            query: sanitized,
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
  }, [tab, exec, pushHistory]);

  const { nodes, relationships } = useMemo(() => extractGraph(result), [result]);

  // Auto-fetch the relationships that connect the projected nodes
  // when the user's query returns nodes only (the common
  // `MATCH (n) RETURN n` pattern). Capped at 1000 ids and 5000 rels
  // to keep the SVG renderable; bigger projections need a Cypher
  // that selects edges explicitly.
  useEffect(() => {
    if (!result || !baseUrl) return;
    if (relationships.length > 0) return;
    if (nodes.length < 2) return;
    const ids = nodes.slice(0, 1000).map((n) => n.id);
    const idList = ids.join(', ');
    const probe = `MATCH (a)-[r]->(b) WHERE a._nexus_id IN [${idList}] AND b._nexus_id IN [${idList}] RETURN a._nexus_id AS s, r._nexus_id AS rid, type(r) AS t, b._nexus_id AS d LIMIT 5000`;
    let cancelled = false;
    api
      .executeCypher(baseUrl, { query: probe }, { apiKey })
      .then((data) => {
        if (cancelled) return;
        const edges: GraphRelationship[] = [];
        const cols = data.columns;
        const sIdx = cols.indexOf('s');
        const rIdx = cols.indexOf('rid');
        const tIdx = cols.indexOf('t');
        const dIdx = cols.indexOf('d');
        if (sIdx < 0 || rIdx < 0 || tIdx < 0 || dIdx < 0) return;
        for (const row of data.rows) {
          const s = row[sIdx];
          const rid = row[rIdx];
          const t = row[tIdx];
          const d = row[dIdx];
          if (
            typeof s === 'number' &&
            typeof rid === 'number' &&
            typeof t === 'string' &&
            typeof d === 'number'
          ) {
            edges.push({ id: rid, type: t, source: s, target: d });
          }
        }
        setExtraRels(edges);
      })
      .catch(() => {
        // Silently ignore — the workspace already renders the
        // primary result; rel auto-fetch is best-effort.
      });
    return () => {
      cancelled = true;
    };
  }, [result, baseUrl, apiKey, nodes, relationships.length]);

  const allRelationships = useMemo(
    () => (relationships.length > 0 ? relationships : extraRels),
    [relationships, extraRels],
  );

  // Auto-fetch `labels(n)` for any projected node that came back
  // without a label. Nexus does not embed labels on the node
  // object (Cortex returns just `_nexus_id` + properties), so the
  // graph would otherwise paint every node as "(unlabelled)".
  useEffect(() => {
    if (!result || !baseUrl) return;
    if (nodes.length === 0) return;
    const missing = nodes.filter((n) => !n.label).map((n) => n.id);
    if (missing.length === 0) return;
    const idList = missing.slice(0, 1000).join(', ');
    const probe = `MATCH (n) WHERE n._nexus_id IN [${idList}] RETURN n._nexus_id AS id, labels(n) AS lbls`;
    let cancelled = false;
    api
      .executeCypher(baseUrl, { query: probe }, { apiKey })
      .then((data) => {
        if (cancelled) return;
        const cols = data.columns;
        const idIdx = cols.indexOf('id');
        const lblsIdx = cols.indexOf('lbls');
        if (idIdx < 0 || lblsIdx < 0) return;
        const next: Record<number, string> = {};
        for (const row of data.rows) {
          const id = row[idIdx];
          const lbls = row[lblsIdx];
          if (typeof id === 'number' && Array.isArray(lbls) && typeof lbls[0] === 'string') {
            next[id] = lbls[0];
          }
        }
        setExtraLabels(next);
      })
      .catch(() => {
        // Best-effort — graph still renders without colors.
      });
    return () => {
      cancelled = true;
    };
  }, [result, baseUrl, apiKey, nodes]);

  // Merge auto-fetched labels onto the extracted nodes for the
  // GraphView + GraphLegend.
  const labeledNodes = useMemo(
    () =>
      nodes.map((n) =>
        n.label ?? extraLabels[n.id]
          ? { ...n, label: n.label ?? extraLabels[n.id] ?? null }
          : n,
      ),
    [nodes, extraLabels],
  );

  const selectedNode = useMemo(
    () => labeledNodes.find((n) => n.id === selectedNodeId) ?? null,
    [labeledNodes, selectedNodeId],
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
            <div className="graph-pane">
              <GraphView
                ref={graphRef}
                result={result}
                extraRelationships={extraRels}
                extraLabels={extraLabels}
                selectedId={selectedNodeId}
                onSelect={setSelectedNodeId}
              />
              <GraphControls
                onZoomIn={() => graphRef.current?.zoomBy(1.4)}
                onZoomOut={() => graphRef.current?.zoomBy(1 / 1.4)}
                onFit={() => graphRef.current?.zoomToFit()}
                onRefreshLayout={() => graphRef.current?.refreshLayout()}
              />
              <GraphLegend nodes={labeledNodes} />
              <NodeInspector
                node={selectedNode}
                relationships={allRelationships}
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
