/**
 * ActivityRail — leftmost 52 px column. Five view buttons + a
 * Tweaks toggle. Selection writes into `layoutStore.currentView`,
 * which the LeftColumn (item 5.1) reads to swap panels.
 */
import type { ReactElement } from 'react';
import { useLayoutStore, type ViewKey } from '../../stores/layoutStore';
import {
  AuditIcon,
  CodeIcon,
  DatabaseIcon,
  ReplicationIcon,
  SettingsIcon,
  VectorIcon,
  type IconProps,
} from '../../icons';

interface RailItem {
  id: ViewKey;
  Icon: (p: IconProps) => ReactElement;
  label: string;
}

const ITEMS: RailItem[] = [
  { id: 'connections', Icon: CodeIcon, label: 'Cypher' },
  { id: 'schema', Icon: DatabaseIcon, label: 'Schema' },
  { id: 'knn', Icon: VectorIcon, label: 'KNN Search' },
  { id: 'replication', Icon: ReplicationIcon, label: 'Replication' },
  { id: 'audit', Icon: AuditIcon, label: 'Audit Log' },
];

export function ActivityRail() {
  const currentView = useLayoutStore((s) => s.currentView);
  const setView = useLayoutStore((s) => s.setView);
  const toggleTweaks = useLayoutStore((s) => s.toggleTweaks);

  return (
    <div className="rail" role="toolbar" aria-label="Activity rail">
      {ITEMS.map(({ id, Icon, label }) => (
        <button
          key={id}
          type="button"
          className={`rail-btn ${currentView === id ? 'active' : ''}`}
          onClick={() => setView(id)}
          title={label}
          aria-label={label}
          aria-current={currentView === id ? 'page' : undefined}
        >
          <Icon />
        </button>
      ))}
      <div className="spacer" />
      <button
        type="button"
        className="rail-btn"
        onClick={toggleTweaks}
        title="Tweaks"
        aria-label="Toggle tweaks"
      >
        <SettingsIcon />
      </button>
    </div>
  );
}
