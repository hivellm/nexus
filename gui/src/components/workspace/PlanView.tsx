/**
 * PlanView — renders the optional `plan` field of a CypherResponse
 * as an indented tree. The server has not finalised the plan
 * payload shape yet, so the renderer accepts an unknown subtree
 * and walks any `children: unknown[]` it finds. When no plan is
 * present the view shows an empty state instead of a blank pane.
 */
import type { ReactElement } from 'react';
import type { CypherResponse } from '../../types/api';

interface PlanViewProps {
  result: CypherResponse | null;
}

interface PlanNode {
  op: string;
  details?: Record<string, unknown>;
  children?: PlanNode[];
}

function isPlanNode(v: unknown): v is PlanNode {
  return typeof v === 'object' && v !== null && 'op' in v;
}

function renderNode(node: PlanNode, depth: number, key: string): ReactElement {
  const detailEntries = node.details
    ? Object.entries(node.details).filter(([k]) => k !== 'children')
    : [];
  return (
    <div key={key} className="plan-node" style={{ paddingLeft: depth * 16 }}>
      <div className="plan-row">
        <span className="plan-op mono">{node.op}</span>
        {detailEntries.map(([k, v]) => (
          <span key={k} className="plan-detail">
            {k}=<span className="mono">{JSON.stringify(v)}</span>
          </span>
        ))}
      </div>
      {Array.isArray(node.children) &&
        node.children.map((c, i) =>
          isPlanNode(c) ? renderNode(c, depth + 1, `${key}.${i}`) : null,
        )}
    </div>
  );
}

export function PlanView({ result }: PlanViewProps) {
  // The server's CypherResponse shape does not yet expose a
  // dedicated `plan` field; once it does, route it through here.
  // Until then the view explains that, instead of rendering blank.
  const plan = (result as unknown as { plan?: unknown } | null)?.plan;

  if (!result) {
    return <div className="results-empty">Run a query to view the execution plan.</div>;
  }
  if (!isPlanNode(plan)) {
    return (
      <div className="results-empty">
        No plan in this response. The server emits a plan tree only when
        <span className="mono"> EXPLAIN </span>or<span className="mono"> PROFILE </span>
        prefixes the query.
      </div>
    );
  }
  return <div className="results-plan">{renderNode(plan, 0, 'root')}</div>;
}
