/**
 * Right drawer — composition of the four §7 sections plus the
 * metrics pump that feeds the ringbuffers off `useStats()` polling.
 */
import { useMetricsPump } from './useMetricsPump';
import { MetricsSection } from './MetricsSection';
import { ReplicationCompact } from './ReplicationCompact';
import { AuditFeed } from './AuditFeed';

export function RightDrawer() {
  useMetricsPump();
  return (
    <aside className="right-col" aria-label="Right drawer">
      <MetricsSection />
      <ReplicationCompact />
      <AuditFeed />
    </aside>
  );
}
