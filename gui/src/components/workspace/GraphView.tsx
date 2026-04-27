/**
 * GraphView — SVG renderer matching the mockup's empty/result
 * shape. Real production rendering swaps to `vis-network` once the
 * graph extraction in the workspace is wired through; for now we
 * extract nodes/relationships from a `CypherResponse` and lay them
 * out with a deterministic radial layout so the view is stable
 * across re-renders (no physics jitter on hover).
 *
 * Selection writes through `onSelect(nodeId)`; the workspace
 * surfaces a `NodeInspector` on the right edge keyed off the
 * selection.
 */
import { useMemo } from 'react';
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

const LABEL_COLORS: Record<string, string> = {
  Module: 'var(--label-module)',
  Function: 'var(--label-function)',
  Struct: 'var(--label-struct)',
  Trait: 'var(--label-trait)',
  Crate: 'var(--label-crate)',
};

function colorFor(label: string | null): string {
  if (label && LABEL_COLORS[label]) return LABEL_COLORS[label];
  return 'var(--accent)';
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
 * reconstruct edges from adjacent column positions matching the
 * Vue archive's heuristic. */
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

interface LaidOutNode extends GraphNode {
  x: number;
  y: number;
}

function layoutRadial(nodes: GraphNode[], width: number, height: number): LaidOutNode[] {
  if (nodes.length === 0) return [];
  const cx = width / 2;
  const cy = height / 2;
  if (nodes.length === 1) return [{ ...nodes[0], x: cx, y: cy }];
  const radius = Math.min(width, height) * 0.36;
  return nodes.map((n, i) => {
    const a = (i / nodes.length) * Math.PI * 2 - Math.PI / 2;
    return { ...n, x: cx + Math.cos(a) * radius, y: cy + Math.sin(a) * radius };
  });
}

interface GraphViewProps {
  result: CypherResponse | null;
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  width?: number;
  height?: number;
}

export function GraphView({
  result,
  selectedId,
  onSelect,
  width = 720,
  height = 480,
}: GraphViewProps) {
  const { nodes, relationships } = useMemo(() => extractGraph(result), [result]);
  const laidOut = useMemo(
    () => layoutRadial(nodes, width, height),
    [nodes, width, height],
  );
  const positionById = useMemo(() => {
    const m = new Map<number, LaidOutNode>();
    for (const n of laidOut) m.set(n.id, n);
    return m;
  }, [laidOut]);

  if (nodes.length === 0) {
    return (
      <div
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

  return (
    <div className="results-graph">
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        onClick={() => onSelect(null)}
        role="img"
        aria-label="Query graph result"
      >
        {relationships.map((r) => {
          const a = positionById.get(r.source);
          const b = positionById.get(r.target);
          if (!a || !b) return null;
          return (
            <g key={r.id}>
              <line
                x1={a.x}
                y1={a.y}
                x2={b.x}
                y2={b.y}
                stroke="var(--fg-3)"
                strokeWidth={1.2}
              />
              <text
                x={(a.x + b.x) / 2}
                y={(a.y + b.y) / 2 - 4}
                textAnchor="middle"
                fontSize={10}
                fill="var(--fg-3)"
                fontFamily="JetBrains Mono"
              >
                :{r.type}
              </text>
            </g>
          );
        })}
        {laidOut.map((n) => {
          const isSelected = n.id === selectedId;
          return (
            <g
              key={n.id}
              transform={`translate(${n.x},${n.y})`}
              onClick={(e) => {
                e.stopPropagation();
                onSelect(n.id);
              }}
              style={{ cursor: 'pointer' }}
            >
              <circle
                r={isSelected ? 22 : 18}
                fill={colorFor(n.label)}
                fillOpacity={0.18}
                stroke={colorFor(n.label)}
                strokeWidth={isSelected ? 2.5 : 1.5}
              />
              <text
                textAnchor="middle"
                y={4}
                fontSize={10}
                fontFamily="JetBrains Mono"
                fill="var(--fg-1)"
              >
                {(n.properties.name as string | undefined) ?? `#${n.id}`}
              </text>
              {n.label && (
                <text
                  textAnchor="middle"
                  y={36}
                  fontSize={9}
                  fontFamily="JetBrains Mono"
                  fill={colorFor(n.label)}
                >
                  :{n.label}
                </text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

export { LABEL_COLORS };
