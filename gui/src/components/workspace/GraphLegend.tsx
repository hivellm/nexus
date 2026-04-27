/**
 * GraphLegend — overlaid bottom-left of the graph pane. Lists the
 * label color chips next to per-label node counts so the user can
 * see which palette entries map to which labels at a glance.
 */
import type { GraphNode } from './GraphView';
import { LABEL_COLORS } from './GraphView';

interface GraphLegendProps {
  nodes: GraphNode[];
}

interface LegendEntry {
  label: string;
  count: number;
  color: string;
}

function summarize(nodes: GraphNode[]): LegendEntry[] {
  const counts = new Map<string, number>();
  for (const n of nodes) {
    const key = n.label ?? '(unlabelled)';
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([label, count]) => ({
      label,
      count,
      color: LABEL_COLORS[label] ?? 'var(--accent)',
    }))
    .sort((a, b) => b.count - a.count);
}

export function GraphLegend({ nodes }: GraphLegendProps) {
  if (nodes.length === 0) return null;
  const entries = summarize(nodes);
  return (
    <div className="graph-legend" aria-label="Graph legend">
      {entries.map((e) => (
        <div key={e.label} className="graph-legend-row">
          <span className="chip" style={{ background: e.color }} />
          <span className="mono">{e.label}</span>
          <span className="count">{e.count}</span>
        </div>
      ))}
    </div>
  );
}
