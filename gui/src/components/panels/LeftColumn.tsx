/**
 * LeftColumn — dispatcher that swaps panel content based on
 * `layoutStore.currentView`. The Cypher view (`connections`) is a
 * stacked Connections + Schema column matching the mockup; every
 * other view renders a single full-height panel.
 */
import { useLayoutStore } from '../../stores/layoutStore';
import { ConnectionsPanel } from './ConnectionsPanel';
import { SchemaPanel } from './SchemaPanel';
import { KnnPanel } from './KnnPanel';
import { ReplicationLeftPanel } from './ReplicationLeftPanel';
import { AuditLeftPanel } from './AuditLeftPanel';

export function LeftColumn() {
  const view = useLayoutStore((s) => s.currentView);

  switch (view) {
    case 'knn':
      return <KnnPanel />;
    case 'replication':
      return <ReplicationLeftPanel />;
    case 'audit':
      return <AuditLeftPanel />;
    case 'schema':
      return <SchemaPanel />;
    case 'connections':
    default:
      return (
        <div
          className="panel"
          style={{ display: 'grid', gridTemplateRows: '220px 1fr', gridGap: 0 }}
        >
          <ConnectionsPanel />
          <SchemaPanel />
        </div>
      );
  }
}
