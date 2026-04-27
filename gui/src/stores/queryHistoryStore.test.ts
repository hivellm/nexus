/**
 * `queryHistoryStore` — capped 200-entry ring of local Cypher
 * runs. Tests cover the push semantics, MRU ordering, and the
 * cap behaviour.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { useQueryHistoryStore } from './queryHistoryStore';

beforeEach(() => {
  localStorage.clear();
  useQueryHistoryStore.setState({ entries: [] });
});

describe('queryHistoryStore', () => {
  it('push prepends the entry and assigns id + ts', () => {
    useQueryHistoryStore.getState().push({
      query: 'MATCH (n) RETURN n',
      ms: 12,
      rows: 5,
      ok: true,
    });
    const entry = useQueryHistoryStore.getState().entries[0];
    expect(entry.query).toBe('MATCH (n) RETURN n');
    expect(entry.ms).toBe(12);
    expect(entry.id).toMatch(/^qh-/);
    expect(typeof entry.ts).toBe('string');
  });

  it('most recent push lands first', () => {
    const push = useQueryHistoryStore.getState().push;
    push({ query: 'A', ms: 1, rows: 1, ok: true });
    push({ query: 'B', ms: 2, rows: 2, ok: true });
    const queries = useQueryHistoryStore
      .getState()
      .entries.map((e) => e.query);
    expect(queries).toEqual(['B', 'A']);
  });

  it('ring caps at 200 entries', () => {
    const push = useQueryHistoryStore.getState().push;
    for (let i = 0; i < 250; i++) {
      push({ query: `Q${i}`, ms: i, rows: i, ok: true });
    }
    const entries = useQueryHistoryStore.getState().entries;
    expect(entries).toHaveLength(200);
    // Most recent is Q249 because push() prepends.
    expect(entries[0].query).toBe('Q249');
  });

  it('clear empties the ring', () => {
    useQueryHistoryStore.getState().push({
      query: 'X',
      ms: 1,
      rows: 0,
      ok: true,
    });
    useQueryHistoryStore.getState().clear();
    expect(useQueryHistoryStore.getState().entries).toEqual([]);
  });
});
