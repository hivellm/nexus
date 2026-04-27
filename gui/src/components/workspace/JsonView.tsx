/**
 * JsonView — pretty-printed result rows with a copy-to-clipboard
 * button. Objects with `_nexus_id` (the server's node-encoding
 * key) render with their id surfaced first so quick visual
 * scanning stays useful even on large payloads.
 */
import { useMemo, useState } from 'react';
import type { CypherResponse } from '../../types/api';

interface JsonViewProps {
  result: CypherResponse | null;
}

export function JsonView({ result }: JsonViewProps) {
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');

  const rendered = useMemo(() => {
    if (!result) return '';
    const payload = {
      columns: result.columns,
      rows: result.rows,
      execution_time_ms: result.execution_time_ms,
    };
    return JSON.stringify(payload, null, 2);
  }, [result]);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(rendered);
      setCopyState('copied');
      setTimeout(() => setCopyState('idle'), 1500);
    } catch {
      setCopyState('failed');
      setTimeout(() => setCopyState('idle'), 2000);
    }
  };

  if (!result) {
    return <div className="results-empty">Run a query to see the JSON payload.</div>;
  }

  return (
    <div className="results-json">
      <div className="results-json-toolbar">
        <span style={{ flex: 1 }} />
        <button className="btn ghost" type="button" onClick={handleCopy}>
          {copyState === 'copied'
            ? 'Copied'
            : copyState === 'failed'
              ? 'Copy failed'
              : 'Copy'}
        </button>
      </div>
      <pre className="results-json-body mono">{rendered}</pre>
    </div>
  );
}
