/**
 * GraphView — force-directed Cypher result renderer backed by
 * `react-force-graph-2d` (canvas + d3-force, MIT-licensed). Replaces
 * the earlier hand-rolled SVG radial layout, which couldn't render
 * edges between large projections without falling back to a fixed
 * concentric ring.
 *
 * Data flow:
 *   1. `extractGraph(result)` walks the CypherResponse and pulls
 *      out `_nexus_id`-tagged nodes + `_nexus_id` + `type` rels.
 *   2. `extraRelationships` (out-of-band edges fetched by the
 *      Workspace when the user's projection is nodes-only) merges
 *      with #1, deduped by id.
 *   3. The merged set feeds ForceGraph2D's `graphData` prop.
 *
 * Node click → `onSelect(id)`; background click → `onSelect(null)`.
 * Selection highlights the node ring, label, and adjacent edges.
 */
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react';
import ForceGraph2D, { type ForceGraphMethods } from 'react-force-graph-2d';
import type { CypherResponse } from '../../types/api';

export interface GraphNode {
  id: number;
  label: string | null;
  properties: Record<string, unknown>;
}

export interface GraphRelationship {
  id: number;
  type: string;
  source: number;
  target: number;
}

interface RFGNode {
  id: number;
  label: string | null;
  display: string;
  color: string;
  properties: Record<string, unknown>;
}

interface RFGLink {
  id: number;
  source: number;
  target: number;
  type: string;
}

const LABEL_COLORS: Record<string, string> = {
  Module: 'var(--label-module)',
  Function: 'var(--label-function)',
  Struct: 'var(--label-struct)',
  Trait: 'var(--label-trait)',
  Crate: 'var(--label-crate)',
};

// CSS variables don't resolve inside <canvas>, so we mirror the
// token palette as static hex values for use by the renderer.
const FALLBACK_PALETTE = [
  '#00d4ff', // accent
  '#a78bfa', // function
  '#10b981', // ok
  '#f59e0b', // warn
  '#ef4444', // err
  '#3b82f6', // info
  '#ec4899',
  '#22d3ee',
  '#84cc16',
  '#f97316',
];

const UNLABELLED_COLOR = '#7a8290';

/** Stable hash → palette index so the same label always maps to
 * the same color across remounts and across the legend. */
function hashLabel(label: string): number {
  let h = 5381;
  for (let i = 0; i < label.length; i++) h = ((h << 5) + h + label.charCodeAt(i)) | 0;
  return Math.abs(h);
}

export function colorForLabel(label: string | null): string {
  if (!label) return UNLABELLED_COLOR;
  return FALLBACK_PALETTE[hashLabel(label) % FALLBACK_PALETTE.length];
}

function isNexusNode(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && '_nexus_id' in v;
}

function isNexusRel(v: unknown): v is Record<string, unknown> {
  return (
    typeof v === 'object' &&
    v !== null &&
    '_nexus_id' in v &&
    'type' in v &&
    typeof (v as { type: unknown }).type === 'string'
  );
}

/** Pull nodes + relationships out of a CypherResponse. The server
 * encodes nodes with `_nexus_id` and rels with `_nexus_id` + `type`;
 * we walk every cell of every row, deduplicate by id, and
 * reconstruct edges from adjacent column positions. */
export function extractGraph(result: CypherResponse | null): {
  nodes: GraphNode[];
  relationships: GraphRelationship[];
} {
  if (!result) return { nodes: [], relationships: [] };

  const nodes = new Map<number, GraphNode>();
  const rels: GraphRelationship[] = [];

  for (const row of result.rows) {
    for (let i = 0; i < row.length; i++) {
      const cell = row[i];
      if (isNexusRel(cell)) {
        const id = (cell as { _nexus_id: number })._nexus_id;
        const type = (cell as { type: string }).type;
        const prev = i > 0 ? row[i - 1] : undefined;
        const next = i + 1 < row.length ? row[i + 1] : undefined;
        const src = isNexusNode(prev)
          ? ((prev as { _nexus_id: number })._nexus_id as number)
          : null;
        const dst = isNexusNode(next)
          ? ((next as { _nexus_id: number })._nexus_id as number)
          : null;
        if (src !== null && dst !== null) {
          rels.push({ id, type, source: src, target: dst });
        }
      } else if (isNexusNode(cell)) {
        const id = (cell as { _nexus_id: number })._nexus_id;
        if (!nodes.has(id)) {
          const labelArr = (cell as { _nexus_labels?: unknown })._nexus_labels;
          const label =
            Array.isArray(labelArr) && typeof labelArr[0] === 'string'
              ? (labelArr[0] as string)
              : null;
          const props: Record<string, unknown> = {};
          for (const [k, v] of Object.entries(cell as Record<string, unknown>)) {
            if (k.startsWith('_nexus_')) continue;
            props[k] = v;
          }
          nodes.set(id, { id, label, properties: props });
        }
      }
    }
  }

  return { nodes: Array.from(nodes.values()), relationships: rels };
}

function pickDisplay(n: GraphNode): string {
  const p = n.properties;
  for (const k of ['name', 'title', 'natural_key', 'path']) {
    const v = p[k];
    if (typeof v === 'string') return v.length > 32 ? v.slice(-32) : v;
  }
  return `#${n.id}`;
}

export interface GraphViewHandle {
  zoomToFit: () => void;
  zoomBy: (factor: number) => void;
  refreshLayout: () => void;
}

interface GraphViewProps {
  result: CypherResponse | null;
  /** Edges fetched out-of-band; merged with the rels extracted
   *  from `result` and deduplicated by id. */
  extraRelationships?: GraphRelationship[];
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  width?: number;
  height?: number;
}

export const GraphView = forwardRef<GraphViewHandle, GraphViewProps>(function GraphView(
  { result, extraRelationships, selectedId, onSelect, width, height },
  ref,
) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [dims, setDims] = useState<{ w: number; h: number }>({ w: 600, h: 400 });

  // ForceGraph2D defaults to window size when width/height are
  // omitted, which overflows our flex layout. Watch the wrapper
  // div and feed the rendered size into the canvas.
  useEffect(() => {
    if (typeof width === 'number' && typeof height === 'number') {
      setDims({ w: width, h: height });
      return;
    }
    const el = containerRef.current;
    if (!el) return;
    const apply = () => {
      const r = el.getBoundingClientRect();
      setDims({ w: Math.max(1, Math.floor(r.width)), h: Math.max(1, Math.floor(r.height)) });
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return () => ro.disconnect();
  }, [width, height]);

  const { nodes, relationships } = useMemo(() => extractGraph(result), [result]);

  const data = useMemo(() => {
    const seen = new Set<number>();
    const links: RFGLink[] = [];
    for (const r of relationships) {
      if (seen.has(r.id)) continue;
      seen.add(r.id);
      links.push({ id: r.id, source: r.source, target: r.target, type: r.type });
    }
    if (extraRelationships) {
      for (const r of extraRelationships) {
        if (seen.has(r.id)) continue;
        seen.add(r.id);
        links.push({ id: r.id, source: r.source, target: r.target, type: r.type });
      }
    }

    const present = new Set(nodes.map((n) => n.id));
    const padded: RFGNode[] = nodes.map((n) => ({
      id: n.id,
      label: n.label,
      display: pickDisplay(n),
      color: colorForLabel(n.label),
      properties: n.properties,
    }));
    // Edges may reference nodes outside the projection (the auto-
    // fetch pulls every (a)-[r]->(b) where both endpoints are in
    // the set, but if `result` itself contains stray edges we want
    // their endpoints to render). Add placeholders for any
    // referenced id that's not already a known node.
    for (const l of links) {
      if (!present.has(l.source as number)) {
        padded.push({
          id: l.source as number,
          label: null,
          display: `#${l.source}`,
          color: UNLABELLED_COLOR,
          properties: {},
        });
        present.add(l.source as number);
      }
      if (!present.has(l.target as number)) {
        padded.push({
          id: l.target as number,
          label: null,
          display: `#${l.target}`,
          color: UNLABELLED_COLOR,
          properties: {},
        });
        present.add(l.target as number);
      }
    }
    return { nodes: padded, links };
  }, [nodes, relationships, extraRelationships]);

  // ForceGraph2D mutates the link source/target props in-place to
  // hold node *references* instead of ids after the first tick.
  // Track both forms on the selection set so highlight comparisons
  // work whether we're looking at the raw or the resolved shape.
  const selectedNeighbors = useMemo(() => {
    if (selectedId === null) return new Set<number>();
    const s = new Set<number>([selectedId]);
    for (const l of data.links) {
      const src = typeof l.source === 'object' ? (l.source as RFGNode).id : (l.source as number);
      const tgt = typeof l.target === 'object' ? (l.target as RFGNode).id : (l.target as number);
      if (src === selectedId) s.add(tgt);
      if (tgt === selectedId) s.add(src);
    }
    return s;
  }, [data.links, selectedId]);

  const fgRef = useRef<ForceGraphMethods<RFGNode, RFGLink>>(undefined);

  useImperativeHandle(
    ref,
    () => ({
      zoomToFit: () => {
        fgRef.current?.zoomToFit(400, 80);
      },
      zoomBy: (factor: number) => {
        const fg = fgRef.current;
        if (!fg) return;
        const z = fg.zoom();
        fg.zoom(Math.min(8, Math.max(0.05, z * factor)), 200);
      },
      refreshLayout: () => {
        fgRef.current?.d3ReheatSimulation();
      },
    }),
    [],
  );

  // Auto-fit after each result change so a fresh query lands
  // centered. The 400ms delay lets the simulation settle a bit
  // before measuring node positions.
  useEffect(() => {
    if (data.nodes.length === 0) return;
    const t = setTimeout(() => {
      fgRef.current?.zoomToFit(400, 80);
    }, 600);
    return () => clearTimeout(t);
  }, [data]);

  if (data.nodes.length === 0) {
    return (
      <div
        ref={containerRef}
        className="results-graph results-graph-empty"
        onClick={() => onSelect(null)}
      >
        <p>No graph data in this result.</p>
        <p className="results-graph-hint">
          Project at least one node (e.g. <span className="mono">RETURN n</span>)
          to populate the graph view.
        </p>
      </div>
    );
  }

  // Auto-tune node radius for very large graphs so 5000 dots don't
  // pile into a solid blob.
  const baseSize = data.nodes.length <= 100 ? 5 : data.nodes.length <= 500 ? 3.5 : 2.5;

  return (
    <div ref={containerRef} className="results-graph">
      <ForceGraph2D
        ref={fgRef}
        graphData={data}
        width={dims.w}
        height={dims.h}
        backgroundColor="#0e1114"
        nodeRelSize={baseSize}
        nodeColor={(n) => (n as RFGNode).color}
        nodeLabel={(n) => {
          const node = n as RFGNode;
          const lbl = node.label ? `:${node.label}` : '';
          return `<div style="font-family:var(--font-mono);font-size:11px"><strong>${node.display}</strong> ${lbl}</div>`;
        }}
        linkColor={() => 'rgba(160,170,180,0.45)'}
        linkDirectionalArrowLength={3}
        linkDirectionalArrowRelPos={1}
        linkWidth={(l) => {
          const link = l as RFGLink;
          const sId = typeof link.source === 'object' ? (link.source as RFGNode).id : (link.source as number);
          const tId = typeof link.target === 'object' ? (link.target as RFGNode).id : (link.target as number);
          if (selectedId !== null && (sId === selectedId || tId === selectedId)) return 2;
          return 1;
        }}
        linkLabel={(l) => `:${(l as RFGLink).type}`}
        onNodeClick={(n) => onSelect((n as RFGNode).id)}
        onBackgroundClick={() => onSelect(null)}
        nodeCanvasObjectMode={() => 'after'}
        nodeCanvasObject={(n, ctx, scale) => {
          const node = n as RFGNode & { x?: number; y?: number };
          const inSel = selectedId !== null && selectedNeighbors.has(node.id);
          const isFocus = node.id === selectedId;
          if (typeof node.x !== 'number' || typeof node.y !== 'number') return;

          // Selection ring.
          if (isFocus) {
            ctx.beginPath();
            ctx.arc(node.x, node.y, baseSize + 4, 0, 2 * Math.PI);
            ctx.strokeStyle = '#00d4ff';
            ctx.lineWidth = 2 / scale;
            ctx.stroke();
          }

          // Label rendering: skip for very large graphs; show
          // adjacent labels when a focus is selected.
          if (data.nodes.length > 200 && !inSel) return;
          const showLabel =
            data.nodes.length <= 100 || inSel || isFocus;
          if (!showLabel) return;
          const text = node.display;
          const fontSize = Math.max(8 / scale, 2);
          ctx.font = `${fontSize}px JetBrains Mono, monospace`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'top';
          ctx.fillStyle = '#cdd3da';
          ctx.fillText(text, node.x, node.y + baseSize + 2);
        }}
      />
    </div>
  );
});

export { LABEL_COLORS };
