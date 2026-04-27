/**
 * NodeInspector — slide-in card on the right edge of the graph
 * pane. Shows the selected node's label badge, id, name, raw
 * properties, and degree (count of incident relationships in the
 * current result set). Carries Expand and Cypher buttons so the
 * user can pivot from the visualization back into the editor.
 */
import { LABEL_COLORS, type GraphNode, type GraphRelationship } from './GraphView';
import { CodeIcon, GraphIcon } from '../../icons';

interface NodeInspectorProps {
  node: GraphNode | null;
  relationships: GraphRelationship[];
  onClose: () => void;
  onExpand?: (node: GraphNode) => void;
  onCypher?: (node: GraphNode) => void;
}

function colorFor(label: string | null): string {
  if (label && LABEL_COLORS[label]) return LABEL_COLORS[label];
  return 'var(--accent)';
}

function degreeOf(node: GraphNode, rels: GraphRelationship[]): number {
  return rels.filter((r) => r.source === node.id || r.target === node.id).length;
}

export function NodeInspector({
  node,
  relationships,
  onClose,
  onExpand,
  onCypher,
}: NodeInspectorProps) {
  if (!node) return null;

  const name = (node.properties.name as string | undefined) ?? `#${node.id}`;
  const propEntries = Object.entries(node.properties);

  return (
    <aside className="node-inspector" aria-label="Node inspector">
      <div className="node-inspector-head">
        <span
          className="label-badge"
          style={{
            background: `${colorFor(node.label)}1f`,
            color: colorFor(node.label),
            borderColor: colorFor(node.label),
          }}
        >
          :{node.label ?? 'unlabelled'}
        </span>
        <span className="node-inspector-id mono">id={node.id}</span>
        <div className="grow" />
        <button
          className="hd-btn"
          type="button"
          aria-label="Close inspector"
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <div className="node-inspector-body">
        <div className="node-inspector-name mono">{name}</div>
        <div className="node-inspector-degree">
          <span>degree</span>
          <strong>{degreeOf(node, relationships)}</strong>
        </div>
        <div className="node-inspector-section">
          <span className="section-title">Properties</span>
          {propEntries.length === 0 ? (
            <span className="empty">No properties</span>
          ) : (
            <ul className="prop-list">
              {propEntries.map(([k, v]) => (
                <li key={k}>
                  <span className="prop-key mono">{k}</span>
                  <span className="prop-value mono">{JSON.stringify(v)}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="node-inspector-actions">
          <button
            className="btn"
            type="button"
            onClick={() => onExpand?.(node)}
            disabled={!onExpand}
          >
            <GraphIcon /> Expand
          </button>
          <button
            className="btn"
            type="button"
            onClick={() => onCypher?.(node)}
            disabled={!onCypher}
          >
            <CodeIcon /> Cypher
          </button>
        </div>
      </div>
    </aside>
  );
}
